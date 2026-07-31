use rusqlite::{params, OptionalExtension, Row};

use crate::db::now_iso;
use crate::error::Result;
use crate::model::Session;
use crate::Store;

fn session_from_row(row: &Row) -> rusqlite::Result<Session> {
    Ok(Session {
        id: row.get("id")?,
        client: row.get("client")?,
        pid: row.get("pid")?,
        project_id: row.get("project_id")?,
        branch: row.get("branch")?,
        started_at: row.get("started_at")?,
        last_active_at: row.get("last_active_at")?,
        ended_at: row.get("ended_at")?,
        leased_count: row.get("leased_count")?,
        completed_count: row.get("completed_count")?,
        failed_count: row.get("failed_count")?,
        handback_count: row.get("handback_count")?,
        captures_during_session: row.get("captures_during_session")?,
        unpromoted_at_end: row.get("unpromoted_at_end")?,
    })
}

const SESSION_COLUMNS: &str = "id, client, pid, project_id, branch, started_at, \
     last_active_at, ended_at, leased_count, completed_count, failed_count, handback_count, \
     captures_during_session, unpromoted_at_end";

impl Store {
    /// Records a new MCP connection -- called once, at connection time, by
    /// magpie-mcp's `MagpieServer::new`. `client` starts `NULL`: MCP's
    /// `clientInfo` handshake result isn't available until the first tool
    /// call resolves it (see `touch_session_active`), not at connection
    /// time.
    pub fn create_session(
        &self,
        id: &str,
        pid: i64,
        project_id: Option<i64>,
        branch: Option<&str>,
    ) -> Result<Session> {
        self.with_conn(|conn| {
            let now = now_iso();
            conn.execute(
                "INSERT INTO sessions (id, pid, project_id, branch, started_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, pid, project_id, branch, now],
            )?;
            get_session_tx(conn, id)
        })
    }

    pub fn get_session(&self, id: &str) -> Result<Session> {
        self.with_conn(|conn| get_session_tx(conn, id))
    }

    /// Public wrapper around `touch_session_active_tx` for callers outside
    /// this crate (magpie-mcp, once per tool call) -- see that function's
    /// doc comment for what this does and why `client` is re-supplied every
    /// call rather than cached.
    pub fn touch_session_active(&self, session_id: &str, client: &str) -> Result<()> {
        self.with_conn(|conn| {
            touch_session_active_tx(conn, session_id, client)?;
            Ok(())
        })
    }

    /// Marks a session ended (graceful stdio-close, or the dead-pid sweep
    /// confirming its process is gone). Idempotent -- ending an
    /// already-ended session just re-sets the same timestamp, since a
    /// dead-pid sweep and a graceful close could plausibly race on the same
    /// session and neither side should error over losing that race.
    pub fn end_session(&self, id: &str) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE sessions SET ended_at = ?1 WHERE id = ?2",
                params![now_iso(), id],
            )?;
            Ok(())
        })
    }

    /// Every session that hasn't ended, for the dead-pid sweep to check pid
    /// liveness against (see apps/desktop/src-tauri/src/dead_pid_sweep.rs).
    pub fn list_active_sessions(&self) -> Result<Vec<(String, i64)>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT id, pid FROM sessions WHERE ended_at IS NULL")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Every session for a project (or every session across all projects,
    /// if `project_id` is `None`), most recently started first.
    pub fn list_sessions(&self, project_id: Option<i64>) -> Result<Vec<Session>> {
        self.with_conn(|conn| {
            let sql = format!(
                "SELECT {SESSION_COLUMNS} FROM sessions
                 WHERE ?1 = 0 OR project_id IS ?2
                 ORDER BY started_at DESC, rowid DESC"
            );
            let mut stmt = conn.prepare(&sql)?;
            let filter_on = project_id.is_some();
            let rows = stmt.query_map(params![filter_on as i64, project_id], session_from_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }
}

/// Backfills `client` (unknown until MCP's `clientInfo` handshake resolves,
/// on the first tool call) and bumps `last_active_at`. Crate-internal so it
/// can be called from inside an existing transaction in `lease.rs`; use
/// `Store::touch_session_active` from outside this crate.
pub(crate) fn touch_session_active_tx(
    conn: &rusqlite::Connection,
    session_id: &str,
    client: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sessions SET client = ?1, last_active_at = ?2 WHERE id = ?3",
        params![client, now_iso(), session_id],
    )?;
    Ok(())
}

pub(crate) fn bump_session_leased_tx(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sessions SET leased_count = leased_count + 1, last_active_at = ?1 WHERE id = ?2",
        params![now_iso(), session_id],
    )?;
    Ok(())
}

pub(crate) fn bump_session_completed_tx(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sessions SET completed_count = completed_count + 1, last_active_at = ?1 WHERE id = ?2",
        params![now_iso(), session_id],
    )?;
    Ok(())
}

pub(crate) fn bump_session_failed_tx(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sessions SET failed_count = failed_count + 1, last_active_at = ?1 WHERE id = ?2",
        params![now_iso(), session_id],
    )?;
    Ok(())
}

pub(crate) fn bump_session_handback_tx(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sessions SET handback_count = handback_count + 1, last_active_at = ?1 WHERE id = ?2",
        params![now_iso(), session_id],
    )?;
    Ok(())
}

fn get_session_tx(conn: &rusqlite::Connection, id: &str) -> Result<Session> {
    let sql = format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1");
    conn.query_row(&sql, params![id], session_from_row)
        .optional()?
        .ok_or_else(|| crate::error::Error::SessionNotFound(id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_session_starts_with_no_client_and_zero_counters() {
        let store = Store::open_in_memory().unwrap();
        let s = store.create_session("sess-1", 4242, None, None).unwrap();
        assert_eq!(s.id, "sess-1");
        assert_eq!(s.pid, 4242);
        assert!(s.client.is_none());
        assert!(s.ended_at.is_none());
        assert_eq!(s.leased_count, 0);
        assert_eq!(s.completed_count, 0);
        assert_eq!(s.failed_count, 0);
        assert_eq!(s.handback_count, 0);
    }

    #[test]
    fn touch_session_active_backfills_client_and_bumps_last_active() {
        let store = Store::open_in_memory().unwrap();
        store.create_session("sess-1", 4242, None, None).unwrap();
        store.touch_session_active("sess-1", "claude-code").unwrap();

        let s = store.get_session("sess-1").unwrap();
        assert_eq!(s.client.as_deref(), Some("claude-code"));
        assert!(s.last_active_at.is_some());
    }

    #[test]
    fn end_session_sets_ended_at_and_is_idempotent() {
        let store = Store::open_in_memory().unwrap();
        store.create_session("sess-1", 4242, None, None).unwrap();
        store.end_session("sess-1").unwrap();
        let first = store.get_session("sess-1").unwrap().ended_at;
        assert!(first.is_some());

        // Calling it again must not error -- a graceful close and the
        // dead-pid sweep could plausibly race on the same session.
        store.end_session("sess-1").unwrap();
        let second = store.get_session("sess-1").unwrap().ended_at;
        assert!(second.is_some());
    }

    #[test]
    fn list_active_sessions_excludes_ended_ones() {
        let store = Store::open_in_memory().unwrap();
        store.create_session("sess-1", 100, None, None).unwrap();
        store.create_session("sess-2", 200, None, None).unwrap();
        store.end_session("sess-2").unwrap();

        let active = store.list_active_sessions().unwrap();
        assert_eq!(active, vec![("sess-1".to_string(), 100)]);
    }

    #[test]
    fn list_sessions_filters_by_project_when_given() {
        let store = Store::open_in_memory().unwrap();
        let proj = store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();
        store
            .create_session("sess-1", 100, Some(proj.id), None)
            .unwrap();
        store.create_session("sess-2", 200, None, None).unwrap();

        let scoped = store.list_sessions(Some(proj.id)).unwrap();
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].id, "sess-1");

        let all = store.list_sessions(None).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn list_sessions_orders_most_recently_started_first() {
        let store = Store::open_in_memory().unwrap();
        store.create_session("sess-1", 100, None, None).unwrap();
        store.create_session("sess-2", 200, None, None).unwrap();

        let all = store.list_sessions(None).unwrap();
        // sess-2 was created after sess-1, so it sorts first.
        assert_eq!(all[0].id, "sess-2");
        assert_eq!(all[1].id, "sess-1");
    }

    #[test]
    fn get_session_missing_id_errors() {
        let store = Store::open_in_memory().unwrap();
        let err = store.get_session("nope").unwrap_err();
        assert!(matches!(err, crate::error::Error::SessionNotFound(id) if id == "nope"));
    }
}
