use rusqlite::{params, OptionalExtension, TransactionBehavior};

use crate::captures::{capture_from_row, get_capture_tx, CAPTURE_COLUMNS};
use crate::db::now_iso;
use crate::error::{Error, Result};
use crate::model::Capture;
use crate::Store;

/// Who is asking to take work, for lease attribution -- shown on the dock
/// as e.g. "claude-code · 2 min" (see docs/design.md "MCP contract").
#[derive(Debug, Clone)]
pub struct LeaseIdentity {
    pub session: String,
    pub client: String,
    pub pid: i64,
}

impl Store {
    /// Lease exactly one Now item for the given project/branch scope --
    /// never a batch. Agents work serially; leasing several at once would
    /// leave the rest sitting leased-but-idle, and the dock would report
    /// work nobody has actually started. Runs inside BEGIN IMMEDIATE so two
    /// concurrent sessions (two agents in the same project) can never take
    /// the same item -- SQLite serializes the writers.
    ///
    /// No auto-expiry: see docs/design.md "MCP contract" for why a timeout
    /// is wrong here (an LLM editing a codebase is a non-idempotent
    /// consumer; a lease timing out mid-task would mean two agents running
    /// the same change). Recovery is stdio-close (the MCP server releases
    /// its own leases when its host disconnects) plus a dead-pid sweep for
    /// `kill -9`, both driven from the columns this sets, never a timer.
    pub fn queue_take(
        &self,
        project_id: Option<i64>,
        branch: Option<&str>,
        identity: &LeaseIdentity,
    ) -> Result<Option<Capture>> {
        self.with_conn_mut(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

            let sql = format!(
                "SELECT {CAPTURE_COLUMNS} FROM captures
                 WHERE project_id IS ?1
                   AND queue_pos IS NOT NULL
                   AND done_at IS NULL
                   AND lease_session IS NULL
                   AND handback_at IS NULL
                   AND (branch IS NULL OR branch IS ?2)
                 ORDER BY queue_pos ASC
                 LIMIT 1"
            );
            let candidate: Option<Capture> = tx
                .query_row(&sql, params![project_id, branch], capture_from_row)
                .optional()?;

            let Some(candidate) = candidate else {
                tx.commit()?;
                return Ok(None);
            };

            tx.execute(
                "UPDATE captures
                 SET lease_session = ?1, lease_client = ?2, lease_pid = ?3,
                     lease_at = ?4, failed_reason = NULL
                 WHERE id = ?5",
                params![
                    identity.session,
                    identity.client,
                    identity.pid,
                    now_iso(),
                    candidate.id
                ],
            )?;
            record_audit_tx(&tx, &identity.client, "queue_take", Some(candidate.id))?;
            crate::sessions::bump_session_leased_tx(&tx, &identity.session)?;

            let leased = get_capture_tx(&tx, candidate.id)?;
            tx.commit()?;
            Ok(Some(leased))
        })
    }

    /// Records the git HEAD at the moment an item was leased, so a later
    /// `capture_handback` can diff against it -- best-effort, not lease-
    /// transactional: a mismatched or stale `session` is a silent no-op
    /// rather than an error, since losing this metadata never blocks the
    /// agent's actual work (see docs/design.md "MCP contract").
    pub fn record_lease_head_commit(&self, id: i64, session: &str, commit: &str) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE captures SET lease_head_commit = ?1 WHERE id = ?2 AND lease_session = ?3",
                params![commit, id, session],
            )?;
            Ok(())
        })
    }

    /// Like `queue_take`, but read-only -- for an agent that wants to see
    /// what's queued before deciding what to work on next, without
    /// claiming anything.
    pub fn queue_peek(
        &self,
        project_id: Option<i64>,
        branch: Option<&str>,
        n: i64,
    ) -> Result<Vec<Capture>> {
        self.with_conn(|conn| {
            let sql = format!(
                "SELECT {CAPTURE_COLUMNS} FROM captures
                 WHERE project_id IS ?1
                   AND queue_pos IS NOT NULL
                   AND done_at IS NULL
                   AND lease_session IS NULL
                   AND handback_at IS NULL
                   AND (branch IS NULL OR branch IS ?2)
                 ORDER BY queue_pos ASC
                 LIMIT ?3"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![project_id, branch, n], capture_from_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Completes a leased item. Must be called by the session holding the
    /// lease -- an agent can't complete work it never took.
    pub fn capture_complete(&self, id: i64, session: &str) -> Result<Capture> {
        self.with_conn_mut(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let client = require_lease_tx(&tx, id, session)?;
            tx.execute(
                "UPDATE captures
                 SET done_at = ?1, lease_session = NULL, lease_client = NULL,
                     lease_pid = NULL, lease_at = NULL, lease_head_commit = NULL
                 WHERE id = ?2",
                params![now_iso(), id],
            )?;
            record_audit_tx(&tx, &client, "capture_done", Some(id))?;
            crate::sessions::bump_session_completed_tx(&tx, session)?;
            let capture = get_capture_tx(&tx, id)?;
            tx.commit()?;
            Ok(capture)
        })
    }

    /// Releases a leased item back to the pool with a reason, instead of
    /// leaving a silent zombie -- see docs/design.md: "failed: couldn't
    /// find src/auth.rs" is actionable, "in progress · 11 min forever" is a
    /// mystery you have to investigate. The item stays in Now and is
    /// immediately eligible for `queue_take` again.
    pub fn capture_fail(&self, id: i64, session: &str, reason: &str) -> Result<Capture> {
        self.with_conn_mut(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let client = require_lease_tx(&tx, id, session)?;
            tx.execute(
                "UPDATE captures
                 SET failed_reason = ?1, lease_session = NULL, lease_client = NULL,
                     lease_pid = NULL, lease_at = NULL, lease_head_commit = NULL
                 WHERE id = ?2",
                params![reason, id],
            )?;
            record_audit_tx(&tx, &client, "capture_fail", Some(id))?;
            crate::sessions::bump_session_failed_tx(&tx, session)?;
            let capture = get_capture_tx(&tx, id)?;
            tx.commit()?;
            Ok(capture)
        })
    }

    /// A third way to resolve a leased item, alongside `capture_complete`
    /// and `capture_fail`: the agent made a real attempt but wants a human
    /// to look before this counts as finished. Stays in Now (unlike
    /// `capture_complete`) but is excluded from `queue_take`/`queue_peek`
    /// (unlike `capture_fail`, which is immediately retakeable) -- see
    /// this query's `handback_at IS NULL` filters. `diff_stat` is whatever
    /// the caller computed via git; this function never computes it itself
    /// (see docs/design.md "MCP contract" -- magpie-core never shells out
    /// to git, only magpie-mcp does).
    pub fn capture_handback(
        &self,
        id: i64,
        session: &str,
        note: &str,
        diff_stat: Option<&str>,
    ) -> Result<Capture> {
        self.with_conn_mut(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let client = require_lease_tx(&tx, id, session)?;
            tx.execute(
                "UPDATE captures
                 SET handback_note = ?1, diff_stat = ?2, handback_at = ?3,
                     lease_session = NULL, lease_client = NULL,
                     lease_pid = NULL, lease_at = NULL, lease_head_commit = NULL
                 WHERE id = ?4",
                params![note, diff_stat, now_iso(), id],
            )?;
            record_audit_tx(&tx, &client, "capture_handback", Some(id))?;
            crate::sessions::bump_session_handback_tx(&tx, session)?;
            let capture = get_capture_tx(&tx, id)?;
            tx.commit()?;
            Ok(capture)
        })
    }

    /// Releases every lease held by `session`, unconditionally. Called by
    /// the MCP server right before it exits (stdio closed -- its host
    /// disconnected), so work it never finished goes back to the pool
    /// instead of sitting leased forever.
    pub fn release_leases_for_session(&self, session: &str) -> Result<usize> {
        self.with_conn(|conn| {
            let n = conn.execute(
                "UPDATE captures
                 SET lease_session = NULL, lease_client = NULL,
                     lease_pid = NULL, lease_at = NULL, lease_head_commit = NULL
                 WHERE lease_session = ?1",
                params![session],
            )?;
            if n > 0 {
                record_audit_tx(conn, session, "session_disconnected_released_leases", None)?;
            }
            Ok(n)
        })
    }

    /// Every lease currently held, for the dead-pid sweep (the `kill -9`
    /// backstop -- stdio-close can't run if the process was killed rather
    /// than exited). Returns (capture_id, pid) pairs; the caller checks
    /// each pid's liveness (platform-specific) and releases the dead ones.
    pub fn list_active_leases(&self) -> Result<Vec<(i64, i64)>> {
        self.with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT id, lease_pid FROM captures WHERE lease_session IS NOT NULL")?;
            let rows =
                stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Releases one specific lease unconditionally -- used by the dead-pid
    /// sweep once it's confirmed the holding process no longer exists.
    pub fn release_lease(&self, id: i64) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE captures
                 SET lease_session = NULL, lease_client = NULL,
                     lease_pid = NULL, lease_at = NULL, lease_head_commit = NULL
                 WHERE id = ?1",
                params![id],
            )?;
            record_audit_tx(
                conn,
                "dead-pid-sweep",
                "lease_released_dead_process",
                Some(id),
            )?;
            Ok(())
        })
    }
}

/// Confirms `session` holds the lease on `id`, returning the client name
/// that took it -- used as the audit actor, since the session id itself is
/// an opaque identifier, not something a human reading the audit log
/// should have to recognize.
fn require_lease_tx(conn: &rusqlite::Connection, id: i64, session: &str) -> Result<String> {
    let row: (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT lease_session, lease_client FROM captures WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or(Error::CaptureNotFound(id))?;

    match row {
        (Some(s), client) if s == session => Ok(client.unwrap_or_default()),
        (Some(_), _) => Err(Error::LeaseMismatch(id, session.to_string())),
        (None, _) => Err(Error::NotLeased(id)),
    }
}

fn record_audit_tx(
    conn: &rusqlite::Connection,
    actor: &str,
    action: &str,
    capture_id: Option<i64>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO audit (at, actor, action, capture_id) VALUES (?1, ?2, ?3, ?4)",
        params![now_iso(), actor, action, capture_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(session: &str) -> LeaseIdentity {
        LeaseIdentity {
            session: session.to_string(),
            client: "test-client".to_string(),
            pid: 12345,
        }
    }

    #[test]
    fn take_leases_one_item_in_queue_order() {
        let store = Store::open_in_memory().unwrap();
        let a = store.capture("first", None).unwrap();
        let b = store.capture("second", None).unwrap();
        store.promote(a.id).unwrap();
        store.promote(b.id).unwrap();

        let taken = store
            .queue_take(None, None, &identity("s1"))
            .unwrap()
            .unwrap();
        assert_eq!(taken.id, a.id);
        assert_eq!(taken.lease_session.as_deref(), Some("s1"));
    }

    #[test]
    fn two_sessions_never_take_the_same_item() {
        let store = Store::open_in_memory().unwrap();
        let a = store.capture("only item", None).unwrap();
        store.promote(a.id).unwrap();

        let first = store.queue_take(None, None, &identity("s1")).unwrap();
        let second = store.queue_take(None, None, &identity("s2")).unwrap();

        assert!(first.is_some());
        assert!(
            second.is_none(),
            "a leased item must not be handed out again"
        );
    }

    #[test]
    fn complete_requires_holding_the_lease() {
        let store = Store::open_in_memory().unwrap();
        let a = store.capture("do it", None).unwrap();
        store.promote(a.id).unwrap();
        store.queue_take(None, None, &identity("s1")).unwrap();

        let err = store.capture_complete(a.id, "s2").unwrap_err();
        assert!(matches!(err, Error::LeaseMismatch(_, _)));

        let done = store.capture_complete(a.id, "s1").unwrap();
        assert!(done.done_at.is_some());
        assert!(done.lease_session.is_none());
    }

    #[test]
    fn fail_releases_the_lease_and_is_immediately_retakeable() {
        let store = Store::open_in_memory().unwrap();
        let a = store.capture("tricky", None).unwrap();
        store.promote(a.id).unwrap();
        store.queue_take(None, None, &identity("s1")).unwrap();

        let failed = store
            .capture_fail(a.id, "s1", "couldn't find the file")
            .unwrap();
        assert_eq!(
            failed.failed_reason.as_deref(),
            Some("couldn't find the file")
        );
        assert!(failed.lease_session.is_none());

        // Immediately eligible again -- no auto-expiry needed, no zombie.
        let retaken = store
            .queue_take(None, None, &identity("s2"))
            .unwrap()
            .unwrap();
        assert_eq!(retaken.id, a.id);
        // Retaking clears the stale failure note so it doesn't look failed
        // and in-progress at once.
        assert!(retaken.failed_reason.is_none());
    }

    #[test]
    fn release_leases_for_session_only_touches_that_session() {
        let store = Store::open_in_memory().unwrap();
        let a = store.capture("a", None).unwrap();
        let b = store.capture("b", None).unwrap();
        store.promote(a.id).unwrap();
        store.promote(b.id).unwrap();
        store.queue_take(None, None, &identity("s1")).unwrap();
        store.queue_take(None, None, &identity("s2")).unwrap();

        let released = store.release_leases_for_session("s1").unwrap();
        assert_eq!(released, 1);

        let a_after = store.get_capture(a.id).unwrap();
        let b_after = store.get_capture(b.id).unwrap();
        assert!(a_after.lease_session.is_none());
        assert_eq!(b_after.lease_session.as_deref(), Some("s2"));
    }

    #[test]
    fn queue_scoped_by_project_never_leaks_across_projects() {
        let store = Store::open_in_memory().unwrap();
        let proj_a = store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();
        let proj_b = store
            .get_or_create_project("b", Some("git@github.com:x/b.git"), None)
            .unwrap();
        let a = store.capture("for a", None).unwrap();
        let b = store.capture("for b", None).unwrap();
        store.assign_project(a.id, Some(proj_a.id)).unwrap();
        store.assign_project(b.id, Some(proj_b.id)).unwrap();
        store.promote(a.id).unwrap();
        store.promote(b.id).unwrap();

        let taken = store
            .queue_take(Some(proj_a.id), None, &identity("agent-in-a"))
            .unwrap()
            .unwrap();
        assert_eq!(taken.id, a.id);

        // An agent scoped to project A must never be handed project B's
        // item -- this is the correctness property the whole MCP design
        // depends on.
        let none_left_for_a = store
            .queue_take(Some(proj_a.id), None, &identity("agent2"))
            .unwrap();
        assert!(none_left_for_a.is_none());
    }

    #[test]
    fn branch_constrained_items_are_invisible_to_other_branches() {
        let store = Store::open_in_memory().unwrap();
        let a = store.capture("main only", None).unwrap();
        store.promote(a.id).unwrap();
        store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE captures SET branch = 'main' WHERE id = ?1",
                    params![a.id],
                )?;
                Ok(())
            })
            .unwrap();

        let on_feature_branch = store
            .queue_take(None, Some("feature-x"), &identity("s1"))
            .unwrap();
        assert!(on_feature_branch.is_none());

        let on_main = store
            .queue_take(None, Some("main"), &identity("s1"))
            .unwrap();
        assert!(on_main.is_some());
    }

    #[test]
    fn dead_pid_sweep_releases_stale_leases() {
        let store = Store::open_in_memory().unwrap();
        let a = store.capture("orphaned", None).unwrap();
        store.promote(a.id).unwrap();
        store.queue_take(None, None, &identity("s1")).unwrap();

        let leases = store.list_active_leases().unwrap();
        assert_eq!(leases.len(), 1);

        store.release_lease(leases[0].0).unwrap();
        let after = store.get_capture(a.id).unwrap();
        assert!(after.lease_session.is_none());
    }

    #[test]
    fn queue_take_bumps_the_taking_sessions_leased_count() {
        let store = Store::open_in_memory().unwrap();
        store.create_session("sess-1", 111, None, None).unwrap();
        let c = store.capture("do the thing", None).unwrap();
        store.promote(c.id).unwrap();

        let identity = LeaseIdentity {
            session: "sess-1".to_string(),
            client: "claude-code".to_string(),
            pid: 111,
        };
        store.queue_take(None, None, &identity).unwrap();

        let session = store.get_session("sess-1").unwrap();
        assert_eq!(session.leased_count, 1);
        assert!(session.last_active_at.is_some());
    }

    #[test]
    fn queue_take_does_not_bump_leased_count_when_nothing_is_queued() {
        let store = Store::open_in_memory().unwrap();
        store.create_session("sess-1", 111, None, None).unwrap();

        let identity = LeaseIdentity {
            session: "sess-1".to_string(),
            client: "claude-code".to_string(),
            pid: 111,
        };
        let result = store.queue_take(None, None, &identity).unwrap();
        assert!(result.is_none());

        let session = store.get_session("sess-1").unwrap();
        assert_eq!(session.leased_count, 0);
    }

    #[test]
    fn capture_complete_bumps_the_completing_sessions_completed_count() {
        let store = Store::open_in_memory().unwrap();
        store.create_session("sess-1", 111, None, None).unwrap();
        let c = store.capture("do the thing", None).unwrap();
        store.promote(c.id).unwrap();
        let identity = LeaseIdentity {
            session: "sess-1".to_string(),
            client: "claude-code".to_string(),
            pid: 111,
        };
        store.queue_take(None, None, &identity).unwrap();

        store.capture_complete(c.id, "sess-1").unwrap();

        let session = store.get_session("sess-1").unwrap();
        assert_eq!(session.completed_count, 1);
    }

    #[test]
    fn capture_fail_bumps_the_failing_sessions_failed_count() {
        let store = Store::open_in_memory().unwrap();
        store.create_session("sess-1", 111, None, None).unwrap();
        let c = store.capture("do the thing", None).unwrap();
        store.promote(c.id).unwrap();
        let identity = LeaseIdentity {
            session: "sess-1".to_string(),
            client: "claude-code".to_string(),
            pid: 111,
        };
        store.queue_take(None, None, &identity).unwrap();

        store
            .capture_fail(c.id, "sess-1", "couldn't find the file")
            .unwrap();

        let session = store.get_session("sess-1").unwrap();
        assert_eq!(session.failed_count, 1);
    }

    #[test]
    fn capture_handback_clears_the_lease_and_sets_review_fields() {
        let store = Store::open_in_memory().unwrap();
        store.create_session("sess-1", 111, None, None).unwrap();
        let c = store.capture("do the thing", None).unwrap();
        store.promote(c.id).unwrap();
        store.queue_take(None, None, &identity("sess-1")).unwrap();

        let handed_back = store
            .capture_handback(c.id, "sess-1", "not sure this is right", Some("+64 -11"))
            .unwrap();

        assert!(handed_back.lease_session.is_none());
        assert!(handed_back.lease_head_commit.is_none());
        assert_eq!(
            handed_back.handback_note.as_deref(),
            Some("not sure this is right")
        );
        assert_eq!(handed_back.diff_stat.as_deref(), Some("+64 -11"));
        assert!(handed_back.handback_at.is_some());
        assert!(handed_back.needs_review());
        assert!(handed_back.in_now(), "a handed-back item stays in Now");
        assert!(handed_back.done_at.is_none());
    }

    #[test]
    fn capture_handback_requires_holding_the_lease() {
        let store = Store::open_in_memory().unwrap();
        store.create_session("s1", 111, None, None).unwrap();
        store.create_session("s2", 222, None, None).unwrap();
        let c = store.capture("do the thing", None).unwrap();
        store.promote(c.id).unwrap();
        store.queue_take(None, None, &identity("s1")).unwrap();

        let err = store
            .capture_handback(c.id, "s2", "not my item", None)
            .unwrap_err();
        assert!(matches!(err, Error::LeaseMismatch(id, session) if id == c.id && session == "s2"));
    }

    #[test]
    fn capture_handback_bumps_the_handback_sessions_handback_count() {
        let store = Store::open_in_memory().unwrap();
        store.create_session("sess-1", 111, None, None).unwrap();
        let c = store.capture("do the thing", None).unwrap();
        store.promote(c.id).unwrap();
        store.queue_take(None, None, &identity("sess-1")).unwrap();

        store
            .capture_handback(c.id, "sess-1", "note", None)
            .unwrap();

        let session = store.get_session("sess-1").unwrap();
        assert_eq!(session.handback_count, 1);
    }

    #[test]
    fn handed_back_items_are_invisible_to_queue_take_and_queue_peek() {
        let store = Store::open_in_memory().unwrap();
        store.create_session("sess-1", 111, None, None).unwrap();
        let c = store.capture("do the thing", None).unwrap();
        store.promote(c.id).unwrap();
        store.queue_take(None, None, &identity("sess-1")).unwrap();
        store
            .capture_handback(c.id, "sess-1", "note", None)
            .unwrap();

        assert!(store.queue_peek(None, None, 10).unwrap().is_empty());
        assert!(store
            .queue_take(None, None, &identity("sess-1"))
            .unwrap()
            .is_none());
    }

    #[test]
    fn record_lease_head_commit_only_applies_to_the_holding_session() {
        let store = Store::open_in_memory().unwrap();
        store.create_session("s1", 111, None, None).unwrap();
        store.create_session("s2", 222, None, None).unwrap();
        let c = store.capture("do the thing", None).unwrap();
        store.promote(c.id).unwrap();
        store.queue_take(None, None, &identity("s1")).unwrap();

        // A mismatched session's attempt to record a head commit is a
        // silent no-op, not an error -- this is best-effort metadata, not
        // a correctness-critical write.
        store
            .record_lease_head_commit(c.id, "s2", "deadbeef")
            .unwrap();
        assert!(store.get_capture(c.id).unwrap().lease_head_commit.is_none());

        store
            .record_lease_head_commit(c.id, "s1", "deadbeef")
            .unwrap();
        assert_eq!(
            store
                .get_capture(c.id)
                .unwrap()
                .lease_head_commit
                .as_deref(),
            Some("deadbeef")
        );
    }

    #[test]
    fn capture_complete_and_capture_fail_clear_lease_head_commit() {
        let store = Store::open_in_memory().unwrap();
        store.create_session("sess-1", 111, None, None).unwrap();
        let a = store.capture("a", None).unwrap();
        let b = store.capture("b", None).unwrap();
        store.promote(a.id).unwrap();
        store.promote(b.id).unwrap();
        store.queue_take(None, None, &identity("sess-1")).unwrap();
        store
            .record_lease_head_commit(a.id, "sess-1", "deadbeef")
            .unwrap();
        store.queue_take(None, None, &identity("sess-1")).unwrap();
        store
            .record_lease_head_commit(b.id, "sess-1", "deadbeef")
            .unwrap();

        store.capture_complete(a.id, "sess-1").unwrap();
        store.capture_fail(b.id, "sess-1", "nope").unwrap();

        assert!(store.get_capture(a.id).unwrap().lease_head_commit.is_none());
        assert!(store.get_capture(b.id).unwrap().lease_head_commit.is_none());
    }
}
