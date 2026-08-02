use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::model::AuditEntry;
use crate::Store;

/// A computed rollup, not a raw table row (see `ProjectOverview` for the
/// same pattern) -- one row of `audit` left-joined against `sessions` on
/// `audit.session_id`, so the activity overlay (see the redesign plan's
/// stage 6) can group by session and show its ordinal/client/branch/live
/// state without an extra query per row. Every `session_*` field is `None`
/// for audit rows written before migration 0010 added `audit.session_id`
/// -- the UI groups those into an explicit "Earlier" bucket rather than
/// guessing which session they belonged to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEntryView {
    pub id: i64,
    pub at: String,
    pub actor: String,
    pub action: String,
    pub capture_id: Option<i64>,
    pub session_id: Option<String>,
    pub session_ordinal: Option<i64>,
    pub session_client: Option<String>,
    pub session_branch: Option<String>,
    pub session_live: Option<bool>,
}

impl Store {
    /// Every MCP action, most recent first -- what turns "what did the
    /// agent do while I was at lunch" from a mystery into a scroll (see
    /// docs/design.md "Agent trust"). Kept as the plain, ungrouped shape
    /// for the CLI and existing tests; the redesign's activity overlay
    /// uses `list_audit_enriched` instead.
    pub fn list_audit(&self, limit: i64) -> Result<Vec<AuditEntry>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, at, actor, action, capture_id, session_id FROM audit
                 ORDER BY at DESC, id DESC
                 LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit], |row| {
                Ok(AuditEntry {
                    id: row.get(0)?,
                    at: row.get(1)?,
                    actor: row.get(2)?,
                    action: row.get(3)?,
                    capture_id: row.get(4)?,
                    session_id: row.get(5)?,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Same rows as `list_audit`, joined against `sessions` so the caller
    /// gets everything needed to group by session in one round trip
    /// instead of N+1 lookups per distinct `session_id`. Grouping itself
    /// happens client-side (see the redesign plan's stage 6's
    /// ActivitySheet) -- this returns one flat, ordered list, not a
    /// pre-grouped shape, so there's one row format to reason about rather
    /// than two.
    pub fn list_audit_enriched(&self, limit: i64) -> Result<Vec<AuditEntryView>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT a.id, a.at, a.actor, a.action, a.capture_id, a.session_id,
                        s.ordinal, s.client, s.branch, s.ended_at IS NULL
                 FROM audit a
                 LEFT JOIN sessions s ON s.id = a.session_id
                 ORDER BY a.at DESC, a.id DESC
                 LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit], |row| {
                Ok(AuditEntryView {
                    id: row.get(0)?,
                    at: row.get(1)?,
                    actor: row.get(2)?,
                    action: row.get(3)?,
                    capture_id: row.get(4)?,
                    session_id: row.get(5)?,
                    session_ordinal: row.get(6)?,
                    session_client: row.get(7)?,
                    session_branch: row.get(8)?,
                    session_live: row.get(9)?,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::lease::LeaseIdentity;
    use crate::Store;

    #[test]
    fn queue_take_and_complete_are_both_recorded() {
        let store = Store::open_in_memory().unwrap();
        let c = store.capture("do the thing", None).unwrap();
        store.promote(c.id).unwrap();

        let identity = LeaseIdentity {
            session: "s1".into(),
            client: "claude-code".into(),
            pid: 1,
        };
        store.queue_take(None, None, &identity).unwrap();
        store.capture_complete(c.id, "s1").unwrap();

        let entries = store.list_audit(10).unwrap();
        let actions: Vec<&str> = entries.iter().map(|e| e.action.as_str()).collect();
        assert!(actions.contains(&"queue_take"));
        assert!(actions.contains(&"capture_done"));
        assert!(entries.iter().all(|e| e.actor == "claude-code"));
    }

    #[test]
    fn queue_take_records_the_leasing_session() {
        let store = Store::open_in_memory().unwrap();
        let c = store.capture("do the thing", None).unwrap();
        store.promote(c.id).unwrap();

        let identity = LeaseIdentity {
            session: "s1".into(),
            client: "claude-code".into(),
            pid: 1,
        };
        store.queue_take(None, None, &identity).unwrap();

        let entries = store.list_audit(10).unwrap();
        let take = entries.iter().find(|e| e.action == "queue_take").unwrap();
        assert_eq!(take.session_id.as_deref(), Some("s1"));
    }

    #[test]
    fn enriched_audit_joins_live_session_metadata() {
        let store = Store::open_in_memory().unwrap();
        let proj = store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();
        store
            .create_session("s1", 100, Some(proj.id), Some("feat/x"))
            .unwrap();
        store.touch_session_active("s1", "claude-code").unwrap();

        let c = store.capture("do the thing", None).unwrap();
        store.assign_project(c.id, Some(proj.id)).unwrap();
        store.promote(c.id).unwrap();
        let identity = LeaseIdentity {
            session: "s1".into(),
            client: "claude-code".into(),
            pid: 100,
        };
        let leased = store.queue_take(Some(proj.id), None, &identity).unwrap();
        assert!(
            leased.is_some(),
            "the capture must actually be leased for this test to mean anything"
        );

        let entries = store.list_audit_enriched(10).unwrap();
        let take = entries.iter().find(|e| e.action == "queue_take").unwrap();
        assert_eq!(take.session_ordinal, Some(1));
        assert_eq!(take.session_client.as_deref(), Some("claude-code"));
        assert_eq!(take.session_branch.as_deref(), Some("feat/x"));
        assert_eq!(take.session_live, Some(true));
    }

    #[test]
    fn enriched_audit_reflects_a_session_that_has_since_ended() {
        let store = Store::open_in_memory().unwrap();
        store.create_session("s1", 100, None, None).unwrap();
        let identity = LeaseIdentity {
            session: "s1".into(),
            client: "claude-code".into(),
            pid: 100,
        };
        let c = store.capture("do the thing", None).unwrap();
        store.promote(c.id).unwrap();
        store.queue_take(None, None, &identity).unwrap();
        store.capture_complete(c.id, "s1").unwrap();
        store.release_leases_for_session("s1").unwrap(); // no-op, nothing held
        store.end_session("s1").unwrap();

        let entries = store.list_audit_enriched(10).unwrap();
        let done = entries.iter().find(|e| e.action == "capture_done").unwrap();
        assert_eq!(done.session_live, Some(false));
    }

    #[test]
    fn enriched_audit_is_none_for_a_dead_pid_sweep_with_no_prior_session() {
        let store = Store::open_in_memory().unwrap();
        let c = store.capture("do the thing", None).unwrap();
        // No lease was ever taken, so releasing it has no prior session to
        // attribute to -- this must not error or fabricate one.
        store.release_lease(c.id).unwrap();

        let entries = store.list_audit_enriched(10).unwrap();
        let released = entries
            .iter()
            .find(|e| e.action == "lease_released_dead_process")
            .unwrap();
        assert_eq!(released.session_id, None);
        assert_eq!(released.session_ordinal, None);
    }
}
