use rusqlite::{params, OptionalExtension, Row};

use crate::db::now_iso;
use crate::error::{Error, Result};
use crate::model::Capture;
use crate::Store;

/// Optional provenance to attach at capture time.
#[derive(Debug, Clone, Default)]
pub struct NewSource {
    pub app_name: Option<String>,
    pub bundle_id: Option<String>,
    pub window_title: Option<String>,
    pub url: Option<String>,
}

pub(crate) fn capture_from_row(row: &Row) -> rusqlite::Result<Capture> {
    Ok(Capture {
        id: row.get("id")?,
        body: row.get("body")?,
        created_at: row.get("created_at")?,
        done_at: row.get("done_at")?,
        failed_reason: row.get("failed_reason")?,
        queue_pos: row.get("queue_pos")?,
        project_id: row.get("project_id")?,
        branch: row.get("branch")?,
        lease_session: row.get("lease_session")?,
        lease_client: row.get("lease_client")?,
        lease_pid: row.get("lease_pid")?,
        lease_at: row.get("lease_at")?,
        source_id: row.get("source_id")?,
        merged_into: row.get("merged_into")?,
    })
}

pub(crate) const CAPTURE_COLUMNS: &str =
    "id, body, created_at, done_at, failed_reason, queue_pos, \
     project_id, branch, lease_session, lease_client, lease_pid, lease_at, source_id, merged_into";

impl Store {
    /// Capture something into the stream (Inbox: no project until assigned).
    /// Never auto-filed into Now -- see docs/design.md "one stream, one
    /// working set" for why that's a deliberate choice, not an omission.
    pub fn capture(&self, body: &str, source: Option<NewSource>) -> Result<Capture> {
        self.with_conn(|conn| {
            let source_id = match source {
                Some(s) => {
                    conn.execute(
                        "INSERT INTO sources (app_name, bundle_id, window_title, url, captured_at)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![s.app_name, s.bundle_id, s.window_title, s.url, now_iso()],
                    )?;
                    Some(conn.last_insert_rowid())
                }
                None => None,
            };

            let now = now_iso();
            conn.execute(
                "INSERT INTO captures (body, created_at, source_id) VALUES (?1, ?2, ?3)",
                params![body, now, source_id],
            )?;
            let id = conn.last_insert_rowid();
            get_capture_tx(conn, id)
        })
    }

    pub fn get_capture(&self, id: i64) -> Result<Capture> {
        self.with_conn(|conn| get_capture_tx(conn, id))
    }

    /// The full stream, reverse-chronological. `project_id: Some(None)`
    /// means "Inbox only"; `None` means "every project, unfiltered" -- the
    /// stream is meant to stay searchable across everything.
    pub fn list_stream(
        &self,
        project_id: Option<Option<i64>>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Capture>> {
        self.with_conn(|conn| {
            let sql = format!(
                "SELECT {CAPTURE_COLUMNS} FROM captures
                 WHERE merged_into IS NULL AND (?1 = 0 OR project_id IS ?2)
                 ORDER BY created_at DESC, id DESC
                 LIMIT ?3 OFFSET ?4"
            );
            let mut stmt = conn.prepare(&sql)?;
            let filter_on = project_id.is_some();
            let want: Option<i64> = project_id.flatten();
            let rows = stmt.query_map(
                params![filter_on as i64, want, limit, offset],
                capture_from_row,
            )?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Active Now items for a project (or the unscoped/Inbox Now when
    /// `project_id` is `None`), ordered by queue_pos. Done items are
    /// excluded -- the queue is a working set that drains, not a log.
    pub fn list_now(&self, project_id: Option<i64>) -> Result<Vec<Capture>> {
        self.with_conn(|conn| {
            let sql = format!(
                "SELECT {CAPTURE_COLUMNS} FROM captures
                 WHERE project_id IS ?1 AND queue_pos IS NOT NULL AND done_at IS NULL
                 ORDER BY queue_pos ASC"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![project_id], capture_from_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Promote a stream item into its project's Now, at the end of the
    /// queue. Uses the capture's existing project_id -- assign a project
    /// first via `assign_project` if it needs one.
    pub fn promote(&self, id: i64) -> Result<Capture> {
        self.with_conn(|conn| {
            let capture = get_capture_tx(conn, id)?;
            let max_pos: Option<f64> = conn.query_row(
                "SELECT MAX(queue_pos) FROM captures WHERE project_id IS ?1",
                params![capture.project_id],
                |r| r.get(0),
            )?;
            let new_pos = max_pos.unwrap_or(0.0) + 1024.0;
            conn.execute(
                "UPDATE captures SET queue_pos = ?1 WHERE id = ?2",
                params![new_pos, id],
            )?;
            get_capture_tx(conn, id)
        })
    }

    /// Remove from Now back into the plain stream.
    pub fn demote(&self, id: i64) -> Result<Capture> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE captures SET queue_pos = NULL WHERE id = ?1",
                params![id],
            )?;
            get_capture_tx(conn, id)
        })
    }

    /// Reorder within Now: move `id` to sit immediately after `after_id`
    /// (or to the front, if `after_id` is `None`). Fractional indexing --
    /// the new position is the midpoint between its new neighbors, so
    /// reordering never requires renumbering the rest of the list.
    pub fn reorder(&self, id: i64, after_id: Option<i64>) -> Result<Capture> {
        self.with_conn(|conn| {
            let capture = get_capture_tx(conn, id)?;
            let project_id = capture.project_id;

            let prev_pos: Option<f64> = match after_id {
                Some(after_id) => Some(
                    conn.query_row(
                        "SELECT queue_pos FROM captures WHERE id = ?1",
                        params![after_id],
                        |r| r.get(0),
                    )
                    .optional()?
                    .flatten()
                    .ok_or(Error::CaptureNotFound(after_id))?,
                ),
                None => None,
            };

            let next_pos: Option<f64> = conn
                .query_row(
                    "SELECT MIN(queue_pos) FROM captures
                     WHERE project_id IS ?1 AND queue_pos IS NOT NULL
                       AND id != ?2
                       AND (?3 IS NULL OR queue_pos > ?3)",
                    params![project_id, id, prev_pos],
                    |r| r.get(0),
                )
                .optional()?
                .flatten();

            let new_pos = match (prev_pos, next_pos) {
                (None, None) => 1024.0,
                (None, Some(next)) => next / 2.0,
                (Some(prev), None) => prev + 1024.0,
                (Some(prev), Some(next)) => (prev + next) / 2.0,
            };

            conn.execute(
                "UPDATE captures SET queue_pos = ?1 WHERE id = ?2",
                params![new_pos, id],
            )?;
            get_capture_tx(conn, id)
        })
    }

    pub fn mark_done(&self, id: i64) -> Result<Capture> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE captures SET done_at = ?1 WHERE id = ?2",
                params![now_iso(), id],
            )?;
            get_capture_tx(conn, id)
        })
    }

    pub fn reopen(&self, id: i64) -> Result<Capture> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE captures SET done_at = NULL WHERE id = ?1",
                params![id],
            )?;
            get_capture_tx(conn, id)
        })
    }

    pub fn assign_project(&self, id: i64, project_id: Option<i64>) -> Result<Capture> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE captures SET project_id = ?1 WHERE id = ?2",
                params![project_id, id],
            )?;
            if let Some(project_id) = project_id {
                crate::projects::touch_project_active_tx(conn, project_id)?;
            }
            get_capture_tx(conn, id)
        })
    }
}

pub(crate) fn get_capture_tx(conn: &rusqlite::Connection, id: i64) -> Result<Capture> {
    let sql = format!("SELECT {CAPTURE_COLUMNS} FROM captures WHERE id = ?1");
    conn.query_row(&sql, params![id], capture_from_row)
        .optional()?
        .ok_or(Error::CaptureNotFound(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_lands_in_stream_not_now() {
        let store = Store::open_in_memory().unwrap();
        let c = store.capture("remember this", None).unwrap();
        assert!(!c.in_now());
        assert_eq!(c.project_id, None);

        let stream = store.list_stream(None, 10, 0).unwrap();
        assert_eq!(stream.len(), 1);
        assert_eq!(stream[0].id, c.id);

        let now = store.list_now(None).unwrap();
        assert!(now.is_empty());
    }

    #[test]
    fn capture_with_source_records_provenance() {
        let store = Store::open_in_memory().unwrap();
        let c = store
            .capture(
                "hello",
                Some(NewSource {
                    app_name: Some("Cursor".into()),
                    bundle_id: Some("com.todesktop.cursor".into()),
                    window_title: None,
                    url: None,
                }),
            )
            .unwrap();
        assert!(c.source_id.is_some());
    }

    #[test]
    fn promote_demote_round_trip() {
        let store = Store::open_in_memory().unwrap();
        let c = store.capture("do the thing", None).unwrap();

        let promoted = store.promote(c.id).unwrap();
        assert!(promoted.in_now());
        assert_eq!(store.list_now(None).unwrap().len(), 1);

        let demoted = store.demote(c.id).unwrap();
        assert!(!demoted.in_now());
        assert!(store.list_now(None).unwrap().is_empty());
    }

    #[test]
    fn now_is_scoped_per_project() {
        let store = Store::open_in_memory().unwrap();
        let proj_a = store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();
        let proj_b = store
            .get_or_create_project("b", Some("git@github.com:x/b.git"), None)
            .unwrap();

        let a = store.capture("for project A", None).unwrap();
        let b = store.capture("for project B", None).unwrap();

        store.assign_project(a.id, Some(proj_a.id)).unwrap();
        store.assign_project(b.id, Some(proj_b.id)).unwrap();
        store.promote(a.id).unwrap();
        store.promote(b.id).unwrap();

        let now_a = store.list_now(Some(proj_a.id)).unwrap();
        assert_eq!(now_a.len(), 1);
        assert_eq!(now_a[0].id, a.id);

        let now_b = store.list_now(Some(proj_b.id)).unwrap();
        assert_eq!(now_b.len(), 1);
        assert_eq!(now_b[0].id, b.id);

        // An agent scoped to project A must never see project B's item,
        // and vice versa -- this is the correctness property the whole
        // project-scoping design exists to guarantee.
        assert!(!now_a.iter().any(|c| c.id == b.id));
        assert!(!now_b.iter().any(|c| c.id == a.id));
    }

    #[test]
    fn reorder_moves_item_between_neighbors() {
        let store = Store::open_in_memory().unwrap();
        let a = store.capture("A", None).unwrap();
        let b = store.capture("B", None).unwrap();
        let c = store.capture("C", None).unwrap();
        store.promote(a.id).unwrap();
        store.promote(b.id).unwrap();
        store.promote(c.id).unwrap();

        // Starting order: A, B, C. Move C to sit right after A -> A, C, B.
        store.reorder(c.id, Some(a.id)).unwrap();

        let order: Vec<i64> = store.list_now(None).unwrap().iter().map(|c| c.id).collect();
        assert_eq!(order, vec![a.id, c.id, b.id]);
    }

    #[test]
    fn reorder_to_front_when_after_id_is_none() {
        let store = Store::open_in_memory().unwrap();
        let a = store.capture("A", None).unwrap();
        let b = store.capture("B", None).unwrap();
        store.promote(a.id).unwrap();
        store.promote(b.id).unwrap();

        store.reorder(b.id, None).unwrap();

        let order: Vec<i64> = store.list_now(None).unwrap().iter().map(|c| c.id).collect();
        assert_eq!(order, vec![b.id, a.id]);
    }

    #[test]
    fn mark_done_removes_from_active_now_but_keeps_the_row() {
        let store = Store::open_in_memory().unwrap();
        let c = store.capture("finish me", None).unwrap();
        store.promote(c.id).unwrap();
        assert_eq!(store.list_now(None).unwrap().len(), 1);

        let done = store.mark_done(c.id).unwrap();
        assert!(done.done_at.is_some());
        assert!(store.list_now(None).unwrap().is_empty());

        let reopened = store.reopen(c.id).unwrap();
        assert!(reopened.done_at.is_none());
        assert_eq!(store.list_now(None).unwrap().len(), 1);
    }

    #[test]
    fn get_capture_missing_id_errors() {
        let store = Store::open_in_memory().unwrap();
        let err = store.get_capture(999).unwrap_err();
        assert!(matches!(err, Error::CaptureNotFound(999)));
    }

    #[test]
    fn assigning_a_capture_touches_its_project_recency() {
        let store = Store::open_in_memory().unwrap();
        store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();
        let b = store
            .get_or_create_project("b", Some("git@github.com:x/b.git"), None)
            .unwrap();
        // Re-touch a so it outranks b by recency despite its lower id --
        // this establishes that assigning a capture to b (below) is what
        // moves b back to the top, not creation order or an id tie-break.
        store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();

        let capture = store.capture("something", None).unwrap();
        store.assign_project(capture.id, Some(b.id)).unwrap();

        let ranked = store.list_projects_by_recency(10).unwrap();
        assert_eq!(ranked[0].id, b.id);
    }
}
