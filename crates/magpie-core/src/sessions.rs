use rusqlite::{params, OptionalExtension, Row, TransactionBehavior};

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
        ordinal: row.get("ordinal")?,
    })
}

const SESSION_COLUMNS: &str = "id, client, pid, project_id, branch, started_at, \
     last_active_at, ended_at, leased_count, completed_count, failed_count, handback_count, \
     captures_during_session, unpromoted_at_end, ordinal";

/// The smallest positive ordinal not already held by a live session in
/// `project_id`'s scope (NULL normalized the same way the partial unique
/// index does, via `coalesce(..., -1)`) -- so `S1`/`S2` stay small and
/// stable rather than growing forever, and a freed slot (a session ending)
/// is reused by the next one to connect. See
/// migrations/0010_ui_provenance.sql for why reuse is the right call here.
fn next_ordinal_tx(conn: &rusqlite::Connection, project_id: Option<i64>) -> rusqlite::Result<i64> {
    let mut stmt = conn.prepare(
        "SELECT ordinal FROM sessions
         WHERE coalesce(project_id, -1) = coalesce(?1, -1)
           AND ended_at IS NULL AND ordinal IS NOT NULL
         ORDER BY ordinal ASC",
    )?;
    let used: Vec<i64> = stmt
        .query_map(params![project_id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut candidate = 1;
    for o in used {
        if o == candidate {
            candidate += 1;
        } else if o > candidate {
            break;
        }
    }
    Ok(candidate)
}

impl Store {
    /// Records a new MCP connection -- called once, at connection time, by
    /// magpie-mcp's `MagpieServer::new`. `client` starts `NULL`: MCP's
    /// `clientInfo` handshake result isn't available until the first tool
    /// call resolves it (see `touch_session_active`), not at connection
    /// time.
    ///
    /// Runs inside `BEGIN IMMEDIATE` (like `queue_take`) so two sessions
    /// connecting to the same project at nearly the same moment can't both
    /// compute the same "smallest free ordinal" and collide on the unique
    /// index -- SQLite serializes the writers instead.
    pub fn create_session(
        &self,
        id: &str,
        pid: i64,
        project_id: Option<i64>,
        branch: Option<&str>,
    ) -> Result<Session> {
        self.with_conn_mut(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let ordinal = next_ordinal_tx(&tx, project_id)?;
            let now = now_iso();
            tx.execute(
                "INSERT INTO sessions (id, pid, project_id, branch, started_at, ordinal)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, pid, project_id, branch, now, ordinal],
            )?;
            let session = get_session_tx(&tx, id)?;
            tx.commit()?;
            Ok(session)
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
    /// confirming its process is gone) and writes a digest capture
    /// summarizing what happened -- see `format_digest_body`. Idempotent in
    /// the sense that ending an already-ended session never errors (a dead-
    /// pid sweep and a graceful close could plausibly race on the same
    /// session), though calling this twice does write a second digest and
    /// re-run the counts against a later `now` -- acceptable since the
    /// realistic race is "ends once, from whichever path notices first."
    pub fn end_session(&self, id: &str) -> Result<()> {
        self.with_conn_mut(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let session = get_session_tx(&tx, id)?;
            let now = now_iso();

            let captures_during: i64 = tx.query_row(
                "SELECT COUNT(*) FROM captures
                 WHERE created_at >= ?1 AND created_at <= ?2 AND kind = 'capture'",
                params![session.started_at, now],
                |r| r.get(0),
            )?;
            let unpromoted: i64 = tx.query_row(
                "SELECT COUNT(*) FROM captures
                 WHERE created_at >= ?1 AND created_at <= ?2 AND kind = 'capture'
                   AND queue_pos IS NULL AND done_at IS NULL",
                params![session.started_at, now],
                |r| r.get(0),
            )?;

            tx.execute(
                "UPDATE sessions
                 SET ended_at = ?1, captures_during_session = ?2, unpromoted_at_end = ?3
                 WHERE id = ?4",
                params![now, captures_during, unpromoted, id],
            )?;

            let body = format_digest_body(&session, captures_during, unpromoted);
            tx.execute(
                "INSERT INTO captures (kind, body, created_at, project_id, branch)
                 VALUES ('session_digest', ?1, ?2, ?3, ?4)",
                params![body, now, session.project_id, session.branch],
            )?;

            tx.commit()?;
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

    /// How many real (non-digest) captures have landed since `since` --
    /// the same window `end_session` uses to compute `captures_during`,
    /// exposed here for the desktop app to synthesize an S0 "session" card
    /// for the human's own use of the app, from its own process-start time
    /// rather than a stored `sessions` row (see the redesign plan's stage 7:
    /// a real row would drop a digest capture into the stream on every
    /// quit). System-wide, not scoped to a project, matching
    /// `end_session`'s own query.
    pub fn count_captures_since(&self, since: &str) -> Result<i64> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM captures WHERE created_at >= ?1 AND kind = 'capture'",
                params![since],
                |r| r.get(0),
            )
            .map_err(Into::into)
        })
    }

    /// The most-captured-from source apps since `since`, most-frequent
    /// first -- same synthesized-S0 use case as `count_captures_since`.
    /// Captures with no resolved source (clipboard-only mode, or a source
    /// magpie couldn't identify) are excluded rather than counted under a
    /// fabricated "Unknown" label.
    pub fn top_source_apps_since(&self, since: &str, limit: i64) -> Result<Vec<(String, i64)>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT s.app_name, COUNT(*) as n
                 FROM captures c
                 JOIN sources s ON s.id = c.source_id
                 WHERE c.created_at >= ?1 AND c.kind = 'capture' AND s.app_name IS NOT NULL
                 GROUP BY s.app_name
                 ORDER BY n DESC, s.app_name ASC
                 LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![since, limit], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }
}

/// The digest's body text -- deliberately doesn't claim to know who made
/// each capture during the session (a human's typed prompt and an agent's
/// `capture_add` are indistinguishable at this layer, both landing via
/// `Store::capture(&body, None)`), so this says what's actually known
/// rather than guessing "you" vs. "the agent".
fn format_digest_body(session: &Session, captures_during: i64, unpromoted: i64) -> String {
    let client = session.client.as_deref().unwrap_or("unknown client");
    format!(
        "Session ended -- {client}. {} completed, {} failed, {} handed back. \
         {captures_during} captured while it ran, {unpromoted} still unpromoted.",
        session.completed_count, session.failed_count, session.handback_count,
    )
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

    #[test]
    fn end_session_writes_a_digest_capture() {
        let store = Store::open_in_memory().unwrap();
        let proj = store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();
        store
            .create_session("sess-1", 111, Some(proj.id), Some("main"))
            .unwrap();

        store.end_session("sess-1").unwrap();

        let stream = store.list_stream(None, 10, 0).unwrap();
        let digests: Vec<_> = stream.iter().filter(|c| c.is_session_digest()).collect();
        assert_eq!(digests.len(), 1);
        assert_eq!(digests[0].project_id, Some(proj.id));
        assert!(!digests[0].in_now(), "a digest is never promoted into Now");
    }

    #[test]
    fn end_session_records_captures_during_session_and_unpromoted_at_end() {
        let store = Store::open_in_memory().unwrap();
        store.create_session("sess-1", 111, None, None).unwrap();

        let a = store.capture("first thing", None).unwrap();
        let _b = store.capture("second thing", None).unwrap();
        store.promote(a.id).unwrap(); // promoted: no longer "unpromoted"

        store.end_session("sess-1").unwrap();

        let session = store.get_session("sess-1").unwrap();
        assert_eq!(session.captures_during_session, Some(2));
        assert_eq!(session.unpromoted_at_end, Some(1));
    }

    #[test]
    fn end_session_digest_does_not_count_itself_or_other_digests() {
        let store = Store::open_in_memory().unwrap();
        store.create_session("sess-1", 111, None, None).unwrap();
        store.end_session("sess-1").unwrap(); // writes one digest, counts 0

        store.create_session("sess-2", 222, None, None).unwrap();
        store.end_session("sess-2").unwrap();

        let session_two = store.get_session("sess-2").unwrap();
        // sess-1's digest already exists in the stream by the time sess-2
        // ends; it must not be counted as something "captured" during
        // sess-2's lifetime.
        assert_eq!(session_two.captures_during_session, Some(0));
        assert_eq!(session_two.unpromoted_at_end, Some(0));
    }

    #[test]
    fn digest_capture_is_findable_via_search() {
        let store = Store::open_in_memory().unwrap();
        store.create_session("sess-1", 111, None, None).unwrap();
        store.touch_session_active("sess-1", "claude-code").unwrap();
        store.end_session("sess-1").unwrap();

        let results = store.search("claude-code", 10).unwrap();
        assert!(
            results.iter().any(|c| c.is_session_digest()),
            "the digest body should mention the client name and be findable by it"
        );
    }

    #[test]
    fn count_captures_since_counts_only_real_captures_in_the_window() {
        let store = Store::open_in_memory().unwrap();
        let before = crate::db::now_iso();
        store.capture("first", None).unwrap();
        store.capture("second", None).unwrap();
        // A session digest is `kind = 'session_digest'`, not `'capture'` --
        // must not be counted alongside real captures.
        store.create_session("sess-1", 111, None, None).unwrap();
        store.end_session("sess-1").unwrap();

        assert_eq!(store.count_captures_since(&before).unwrap(), 2);
    }

    #[test]
    fn count_captures_since_excludes_captures_before_the_window() {
        let store = Store::open_in_memory().unwrap();
        store.capture("too early", None).unwrap();
        let cutoff = crate::db::now_iso();

        assert_eq!(store.count_captures_since(&cutoff).unwrap(), 0);
    }

    #[test]
    fn top_source_apps_since_ranks_by_frequency_and_excludes_unknown_source() {
        let store = Store::open_in_memory().unwrap();
        let before = crate::db::now_iso();
        store
            .capture(
                "a",
                Some(crate::captures::NewSource {
                    app_name: Some("Cursor".into()),
                    ..Default::default()
                }),
            )
            .unwrap();
        store
            .capture(
                "b",
                Some(crate::captures::NewSource {
                    app_name: Some("Terminal".into()),
                    ..Default::default()
                }),
            )
            .unwrap();
        store
            .capture(
                "c",
                Some(crate::captures::NewSource {
                    app_name: Some("Cursor".into()),
                    ..Default::default()
                }),
            )
            .unwrap();
        // No source at all -- must not appear as a fabricated "Unknown" entry.
        store.capture("d", None).unwrap();

        let top = store.top_source_apps_since(&before, 10).unwrap();
        assert_eq!(
            top,
            vec![("Cursor".to_string(), 2), ("Terminal".to_string(), 1)]
        );
    }

    #[test]
    fn digest_is_invisible_to_queue_take_and_queue_peek() {
        let store = Store::open_in_memory().unwrap();
        let proj = store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();
        store
            .create_session("sess-1", 111, Some(proj.id), None)
            .unwrap();
        store.end_session("sess-1").unwrap();

        assert!(store
            .queue_peek(Some(proj.id), None, 10)
            .unwrap()
            .is_empty());
        let identity = crate::lease::LeaseIdentity {
            session: "sess-2".to_string(),
            client: "someone-else".to_string(),
            pid: 222,
        };
        store
            .create_session("sess-2", 222, Some(proj.id), None)
            .unwrap();
        assert!(store
            .queue_take(Some(proj.id), None, &identity)
            .unwrap()
            .is_none());
    }

    #[test]
    fn ordinals_are_assigned_sequentially_per_project() {
        let store = Store::open_in_memory().unwrap();
        let proj = store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();

        let s1 = store
            .create_session("sess-1", 100, Some(proj.id), None)
            .unwrap();
        let s2 = store
            .create_session("sess-2", 200, Some(proj.id), None)
            .unwrap();
        assert_eq!(s1.ordinal, Some(1));
        assert_eq!(s2.ordinal, Some(2));
    }

    #[test]
    fn ordinals_are_scoped_per_project_not_global() {
        let store = Store::open_in_memory().unwrap();
        let a = store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();
        let b = store
            .get_or_create_project("b", Some("git@github.com:x/b.git"), None)
            .unwrap();

        let s1 = store
            .create_session("sess-1", 100, Some(a.id), None)
            .unwrap();
        // A live session already holds ordinal 1 in project `a`, but `b`
        // is a different scope -- its first session also starts at 1
        // rather than continuing a single global counter.
        let s2 = store
            .create_session("sess-2", 200, Some(b.id), None)
            .unwrap();
        assert_eq!(s1.ordinal, Some(1));
        assert_eq!(s2.ordinal, Some(1));

        // The Inbox scope (project_id None) is its own scope too.
        let s3 = store.create_session("sess-3", 300, None, None).unwrap();
        assert_eq!(s3.ordinal, Some(1));
    }

    #[test]
    fn ending_a_session_frees_its_ordinal_for_reuse() {
        let store = Store::open_in_memory().unwrap();
        let proj = store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();

        store
            .create_session("sess-1", 100, Some(proj.id), None)
            .unwrap();
        let s2 = store
            .create_session("sess-2", 200, Some(proj.id), None)
            .unwrap();
        assert_eq!(s2.ordinal, Some(2));

        // sess-1 (ordinal 1) ends -- its slot is now free.
        store.end_session("sess-1").unwrap();

        let s3 = store
            .create_session("sess-3", 300, Some(proj.id), None)
            .unwrap();
        assert_eq!(
            s3.ordinal,
            Some(1),
            "a freed ordinal is reused by the next session to connect, \
             not skipped in favor of an ever-growing counter"
        );

        // sess-2 (still live, ordinal 2) is untouched by sess-3 connecting.
        let s2_again = store.get_session("sess-2").unwrap();
        assert_eq!(s2_again.ordinal, Some(2));
    }

    #[test]
    fn ordinal_fills_the_lowest_gap_not_just_the_end() {
        let store = Store::open_in_memory().unwrap();
        let proj = store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();

        store
            .create_session("sess-1", 100, Some(proj.id), None)
            .unwrap(); // ordinal 1
        store
            .create_session("sess-2", 200, Some(proj.id), None)
            .unwrap(); // ordinal 2
        store
            .create_session("sess-3", 300, Some(proj.id), None)
            .unwrap(); // ordinal 3

        store.end_session("sess-2").unwrap(); // frees ordinal 2, a gap

        let s4 = store
            .create_session("sess-4", 400, Some(proj.id), None)
            .unwrap();
        assert_eq!(s4.ordinal, Some(2), "the lowest free slot, not 4");
    }
}
