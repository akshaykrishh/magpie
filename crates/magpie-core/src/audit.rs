use rusqlite::params;

use crate::error::Result;
use crate::model::AuditEntry;
use crate::Store;

impl Store {
    /// Every MCP action, most recent first -- what turns "what did the
    /// agent do while I was at lunch" from a mystery into a scroll (see
    /// docs/design.md "Agent trust").
    pub fn list_audit(&self, limit: i64) -> Result<Vec<AuditEntry>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, at, actor, action, capture_id FROM audit
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
}
