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
        kind: row.get("kind")?,
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
        lease_head_commit: row.get("lease_head_commit")?,
        handback_note: row.get("handback_note")?,
        diff_stat: row.get("diff_stat")?,
        handback_at: row.get("handback_at")?,
        source_id: row.get("source_id")?,
        merged_into: row.get("merged_into")?,
        section_id: row.get("section_id")?,
        deleted_at: row.get("deleted_at")?,
    })
}

pub(crate) const CAPTURE_COLUMNS: &str =
    "id, kind, body, created_at, done_at, failed_reason, queue_pos, \
     project_id, branch, lease_session, lease_client, lease_pid, lease_at, lease_head_commit, \
     handback_note, diff_stat, handback_at, source_id, merged_into, section_id, deleted_at";

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

    /// The text to show/copy for a capture: its body, or (for a screenshot,
    /// whose body is always empty -- see capture_flow.rs's
    /// on_screenshot_hotkey) its OCR text, or an honest placeholder if OCR
    /// hasn't finished yet. Never a silent blank line -- see
    /// docs/superpowers/specs/2026-07-31-capture-list-v2-design.md's
    /// "Copy / Copy as List".
    pub fn capture_display_text(&self, id: i64) -> Result<String> {
        let capture = self.get_capture(id)?;
        if !capture.body.is_empty() {
            return Ok(capture.body);
        }
        let blob = self.get_blob_for_capture(id)?;
        Ok(blob
            .and_then(|b| b.ocr_text)
            .unwrap_or_else(|| "[screenshot — OCR pending]".to_string()))
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
                   AND deleted_at IS NULL
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
                   AND deleted_at IS NULL
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
            if capture.is_session_digest() {
                return Err(Error::CannotPromoteDigest(id));
            }
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

    /// Edit a capture's body in place. Rejects session digests -- they're
    /// system-generated summaries, not user-authored content (mirrors the
    /// `promote` guard above). The plain `UPDATE ... SET body` fires the
    /// `captures_fts_au` trigger, keeping full-text search in sync.
    pub fn update_capture_body(&self, id: i64, body: &str) -> Result<Capture> {
        self.with_conn(|conn| {
            let capture = get_capture_tx(conn, id)?;
            // A soft-deleted capture must look not-found, same as every other
            // id-based lookup on captures/templates -- otherwise the UPDATE
            // below (scoped to `deleted_at IS NULL`) would silently match zero
            // rows and this would return Ok with the edit discarded.
            if capture.deleted_at.is_some() {
                return Err(Error::CaptureNotFound(id));
            }
            if capture.is_session_digest() {
                return Err(Error::CannotEditDigest(id));
            }
            conn.execute(
                "UPDATE captures SET body = ?1 WHERE id = ?2 AND deleted_at IS NULL",
                params![body, id],
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

    pub fn assign_capture_section(&self, id: i64, section_id: Option<i64>) -> Result<Capture> {
        self.with_conn(|conn| {
            if let Some(section_id) = section_id {
                conn.query_row(
                    "SELECT id FROM sections WHERE id = ?1 AND deleted_at IS NULL",
                    params![section_id],
                    |_| Ok(()),
                )
                .optional()?
                .ok_or(Error::SectionNotFound(section_id))?;
            }
            conn.execute(
                "UPDATE captures SET section_id = ?1 WHERE id = ?2 AND deleted_at IS NULL",
                params![section_id, id],
            )?;
            let sql = format!("SELECT {CAPTURE_COLUMNS} FROM captures WHERE id = ?1");
            conn.query_row(&sql, params![id], capture_from_row)
                .optional()?
                .ok_or(Error::CaptureNotFound(id))
        })
    }

    /// Cascades to any capture this one absorbed via merge (`merged_into`
    /// pointing at it) -- otherwise Undo would restore a capture whose
    /// merge history silently vanished into orphaned, invisible rows. See
    /// docs/superpowers/specs/2026-07-31-capture-list-v2-design.md.
    pub fn soft_delete_capture(&self, id: i64) -> Result<Capture> {
        self.with_conn(|conn| {
            let now = now_iso();
            conn.execute(
                "UPDATE captures SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
                params![now, id],
            )?;
            conn.execute(
                "UPDATE captures SET deleted_at = ?1 WHERE merged_into = ?2 AND deleted_at IS NULL",
                params![now, id],
            )?;
            get_capture_tx(conn, id)
        })
    }

    pub fn restore_capture(&self, id: i64) -> Result<Capture> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE captures SET deleted_at = NULL WHERE id = ?1",
                params![id],
            )?;
            conn.execute(
                "UPDATE captures SET deleted_at = NULL WHERE merged_into = ?1",
                params![id],
            )?;
            get_capture_tx(conn, id)
        })
    }

    /// `merged_into IS NULL` excludes merge-absorbed sources -- when a
    /// merged-result capture is soft-deleted, the cascade also marks its
    /// absorbed sources deleted, but they must stay invisible here just as
    /// they're invisible everywhere else (stream, search, export); otherwise
    /// this view would show both the merged result and its now-redundant
    /// originals as separate "deleted" entries.
    pub fn list_recently_deleted_captures(&self) -> Result<Vec<Capture>> {
        self.with_conn(|conn| {
            let sql = format!(
                "SELECT {CAPTURE_COLUMNS} FROM captures
                 WHERE deleted_at IS NOT NULL AND merged_into IS NULL
                 ORDER BY deleted_at DESC"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], capture_from_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn purge_expired_captures(&self, cutoff: &str) -> Result<usize> {
        self.with_conn(|conn| {
            Ok(conn.execute(
                "DELETE FROM captures WHERE deleted_at IS NOT NULL AND deleted_at < ?1",
                params![cutoff],
            )?)
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
    fn new_captures_have_no_lease_or_handback_state() {
        let store = Store::open_in_memory().unwrap();
        let c = store.capture("something", None).unwrap();
        assert!(c.lease_head_commit.is_none());
        assert!(c.handback_note.is_none());
        assert!(c.diff_stat.is_none());
        assert!(c.handback_at.is_none());
        assert!(!c.needs_review());
    }

    #[test]
    fn new_captures_default_to_kind_capture() {
        let store = Store::open_in_memory().unwrap();
        let c = store.capture("something", None).unwrap();
        assert_eq!(c.kind, "capture");
        assert!(!c.is_session_digest());
    }

    #[test]
    fn promote_rejects_a_session_digest() {
        let store = Store::open_in_memory().unwrap();
        store.create_session("sess-1", 111, None, None).unwrap();
        store.end_session("sess-1").unwrap();

        let stream = store.list_stream(None, 10, 0).unwrap();
        let digest = stream.iter().find(|c| c.is_session_digest()).unwrap();

        let err = store.promote(digest.id).unwrap_err();
        assert!(matches!(err, Error::CannotPromoteDigest(id) if id == digest.id));
    }

    #[test]
    fn update_capture_body_changes_the_body_and_stays_searchable() {
        let store = Store::open_in_memory().unwrap();
        let c = store.capture("original", None).unwrap();
        let updated = store.update_capture_body(c.id, "edited text").unwrap();
        assert_eq!(updated.body, "edited text");
        let results = store.search("edited", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(store.search("original", 10).unwrap().is_empty());
    }

    #[test]
    fn update_capture_body_rejects_a_session_digest() {
        // Mirrors promote_rejects_a_session_digest's setup exactly: end_session
        // returns (), so a digest is located by scanning the stream afterward,
        // not by capturing an id from end_session's return value.
        let store = Store::open_in_memory().unwrap();
        store.create_session("sess-1", 111, None, None).unwrap();
        store.end_session("sess-1").unwrap();

        let stream = store.list_stream(None, 10, 0).unwrap();
        let digest = stream.iter().find(|c| c.is_session_digest()).unwrap();

        let err = store.update_capture_body(digest.id, "edited").unwrap_err();
        assert!(matches!(err, Error::CannotEditDigest(id) if id == digest.id));
    }

    #[test]
    fn update_capture_body_treats_a_soft_deleted_capture_as_not_found() {
        // Consistent with the global rule that every id-based lookup on a
        // capture/template must include `deleted_at IS NULL`: a soft-deleted
        // row must error like a genuinely missing one, not silently discard
        // the edit and return Ok with the unchanged body.
        let store = Store::open_in_memory().unwrap();
        let c = store.capture("original", None).unwrap();
        store.soft_delete_capture(c.id).unwrap();

        let err = store.update_capture_body(c.id, "edited").unwrap_err();
        assert!(matches!(err, Error::CaptureNotFound(id) if id == c.id));
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

    #[test]
    fn new_captures_default_to_no_section_and_not_deleted() {
        let store = Store::open_in_memory().unwrap();
        let c = store.capture("hello", None).unwrap();
        assert_eq!(c.section_id, None);
        assert_eq!(c.deleted_at, None);
    }

    #[test]
    fn assign_capture_section_round_trips_and_clears_previous() {
        let store = Store::open_in_memory().unwrap();
        let c = store.capture("note", None).unwrap();
        let a = store.create_section("A").unwrap();
        let b = store.create_section("B").unwrap();

        let assigned = store.assign_capture_section(c.id, Some(a.id)).unwrap();
        assert_eq!(assigned.section_id, Some(a.id));

        // Single membership: assigning to B replaces A, doesn't add to it.
        let reassigned = store.assign_capture_section(c.id, Some(b.id)).unwrap();
        assert_eq!(reassigned.section_id, Some(b.id));

        let cleared = store.assign_capture_section(c.id, None).unwrap();
        assert_eq!(cleared.section_id, None);
    }

    #[test]
    fn assign_capture_section_rejects_a_nonexistent_section() {
        let store = Store::open_in_memory().unwrap();
        let c = store.capture("note", None).unwrap();
        let err = store.assign_capture_section(c.id, Some(999)).unwrap_err();
        assert!(matches!(err, Error::SectionNotFound(999)));
    }

    #[test]
    fn soft_delete_hides_from_stream_and_restore_reverses_it() {
        let store = Store::open_in_memory().unwrap();
        let c = store.capture("delete me", None).unwrap();

        let deleted = store.soft_delete_capture(c.id).unwrap();
        assert!(deleted.deleted_at.is_some());
        assert!(store.list_stream(None, 100, 0).unwrap().is_empty());

        let restored = store.restore_capture(c.id).unwrap();
        assert_eq!(restored.deleted_at, None);
        assert_eq!(store.list_stream(None, 100, 0).unwrap().len(), 1);
    }

    #[test]
    fn soft_delete_excludes_from_search() {
        let store = Store::open_in_memory().unwrap();
        let c = store.capture("unique needle text", None).unwrap();
        store.soft_delete_capture(c.id).unwrap();
        assert!(store.search("needle", 10).unwrap().is_empty());
    }

    #[test]
    fn deleting_a_merged_result_cascades_to_its_absorbed_sources() {
        let store = Store::open_in_memory().unwrap();
        let a = store.capture("part one", None).unwrap();
        let b = store.capture("part two", None).unwrap();
        let merged = store.merge(&[a.id, b.id]).unwrap();

        store.soft_delete_capture(merged.id).unwrap();

        // Absorbed sources aren't independently listed even when active
        // (existing merge behavior), but their own deleted_at should now
        // be set too, so a direct restore of the merge result also
        // restores their state consistently. Check both sources, not just
        // one, since the cascade UPDATE could in principle miss one of them.
        let a_row = store.get_capture(a.id).unwrap();
        let b_row = store.get_capture(b.id).unwrap();
        assert!(a_row.deleted_at.is_some());
        assert!(b_row.deleted_at.is_some());

        store.restore_capture(merged.id).unwrap();
        let a_row = store.get_capture(a.id).unwrap();
        let b_row = store.get_capture(b.id).unwrap();
        assert_eq!(a_row.deleted_at, None);
        assert_eq!(b_row.deleted_at, None);
    }

    #[test]
    fn list_recently_deleted_captures_returns_only_deleted_ones_newest_first() {
        let store = Store::open_in_memory().unwrap();
        let a = store.capture("a", None).unwrap();
        let b = store.capture("b", None).unwrap();
        store.soft_delete_capture(a.id).unwrap();
        store.soft_delete_capture(b.id).unwrap();

        let deleted = store.list_recently_deleted_captures().unwrap();
        assert_eq!(deleted.len(), 2);
        assert_eq!(deleted[0].id, b.id); // most-recently-deleted first
    }

    #[test]
    fn list_recently_deleted_captures_excludes_merge_absorbed_sources() {
        let store = Store::open_in_memory().unwrap();
        let a = store.capture("part one", None).unwrap();
        let b = store.capture("part two", None).unwrap();
        let merged = store.merge(&[a.id, b.id]).unwrap();

        // Deleting the merged result cascades deleted_at to its absorbed
        // sources too (see soft_delete_capture), but they must not surface
        // here as independent entries -- same "merged-away captures are
        // never independently visible" rule as the stream, search, and
        // export views.
        store.soft_delete_capture(merged.id).unwrap();

        let deleted = store.list_recently_deleted_captures().unwrap();
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].id, merged.id);
    }

    #[test]
    fn purge_expired_captures_only_removes_rows_past_the_cutoff() {
        let store = Store::open_in_memory().unwrap();
        let old = store.capture("old", None).unwrap();
        let recent = store.capture("recent", None).unwrap();
        store.soft_delete_capture(old.id).unwrap();
        store.soft_delete_capture(recent.id).unwrap();

        // Force `old`'s deleted_at far into the past directly, since both
        // were just soft-deleted "now" in this test.
        store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE captures SET deleted_at = '2000-01-01T00:00:00Z' WHERE id = ?1",
                    rusqlite::params![old.id],
                )?;
                Ok(())
            })
            .unwrap();

        let purged = store.purge_expired_captures("2020-01-01T00:00:00Z").unwrap();
        assert_eq!(purged, 1);
        assert!(store.get_capture(old.id).is_err());
        assert!(store.get_capture(recent.id).is_ok());
    }

    #[test]
    fn capture_display_text_falls_back_to_ocr_then_placeholder() {
        let store = Store::open_in_memory().unwrap();
        let text_capture = store.capture("hello", None).unwrap();
        assert_eq!(store.capture_display_text(text_capture.id).unwrap(), "hello");

        let shot = store
            .capture_screenshot("/tmp/shot.png", "image/png", None, None, None)
            .unwrap();
        assert_eq!(
            store.capture_display_text(shot.id).unwrap(),
            "[screenshot — OCR pending]"
        );

        let blob = store.get_blob_for_capture(shot.id).unwrap().unwrap();
        store.set_blob_ocr_text(blob.id, "receipt total").unwrap();
        assert_eq!(store.capture_display_text(shot.id).unwrap(), "receipt total");
    }
}
