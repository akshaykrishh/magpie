# Session Entity (Backend) — Phase 2 of the magpie Explorations canonical design Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give an MCP-connected agent session a persistent row (who, since when, what project/branch, how many items leased/completed/failed, when it ended) instead of an in-memory-only identity string that vanishes when the connection closes — so a future UI can show "who's doing what" without any UI work happening in this plan.

**Architecture:** A new `sessions` table in magpie-core, populated at connection time by `magpie-mcp`'s `MagpieServer::new` (created, not just held in memory), updated on every `queue_take`/`capture_done`/`capture_fail` call (activity + counters, inside the same transaction as the existing capture mutation), and closed either gracefully (stdio close, in `serve_stdio`) or by the existing dead-pid sweep (extended to also end sessions, not just release leases). This plan is backend-only: no Tauri window, no React component. It adds one new Tauri command (`list_sessions`) purely as an IPC contract for a UI to consume later.

**Tech Stack:** Rust (magpie-core: rusqlite/SQLite; magpie-mcp: rmcp over stdio; apps/desktop/src-tauri: Tauri v2 commands).

## Global Constraints

- **No timeout/expiry model.** `docs/design.md`'s "MCP contract" section is explicit and already decided: *"Recovery uses liveness, not timers... For non-idempotent consumers, at-most-once with human-initiated retry is correct."* A session is never auto-ended by elapsed time — only by graceful stdio-close or the dead-pid sweep confirming its process is gone. Do not add a `last_active_at`-based expiry anywhere.
- **Session digests are out of scope for this plan.** A synthetic "S4 ended — drained 2, refused 1" entry written into the capture stream is real, planned work, but belongs to the Stream/main-window phase, which consumes a session's final counters after this plan makes them queryable. This plan ends a session by setting `ended_at`; it does not write anything to `captures` or any stream-visible table.
- **No UI/frontend work.** Everything in `apps/desktop/src/**` is out of scope. The one Tauri command this plan adds (`list_sessions`) is an IPC contract only — nothing calls it from the frontend yet.
- **Attribution stays MCP-`clientInfo`-sourced**, matching the existing `client_name` mechanism — do not invent a different way to identify which tool/agent is connected.
- Migration files go in `crates/magpie-core/migrations/000N_<name>.sql`, registered as a new tuple appended to `MIGRATIONS` in `crates/magpie-core/src/db.rs`. The next free number is `0005`.
- Match existing code style: doc comments explain *why*, not *what*.
- The CLI (`crates/magpie-cli`) never leases anything (confirmed: zero references to `LeaseIdentity` or lease methods) and is not a session participant — no changes needed there.

---

### Task 1: `sessions` table, `Session` model, and CRUD in magpie-core

**Files:**
- Create: `crates/magpie-core/migrations/0005_sessions.sql`
- Modify: `crates/magpie-core/src/db.rs` (register migration)
- Modify: `crates/magpie-core/src/model.rs` (new `Session` struct)
- Modify: `crates/magpie-core/src/error.rs` (new `SessionNotFound` variant)
- Create: `crates/magpie-core/src/sessions.rs`
- Modify: `crates/magpie-core/src/lib.rs` (register module, export `Session`)
- Test: inline `#[cfg(test)]` module in `sessions.rs`

**Interfaces:**
- Produces: `Store::create_session(&self, id: &str, pid: i64, project_id: Option<i64>, branch: Option<&str>) -> Result<Session>` — consumed by Task 3.
- Produces: `Store::touch_session_active(&self, session_id: &str, client: &str) -> Result<()>` — consumed by Task 3.
- Produces: `Store::end_session(&self, id: &str) -> Result<()>` — consumed by Task 3 and Task 4.
- Produces: `Store::list_active_sessions(&self) -> Result<Vec<(String, i64)>>` (id, pid pairs) — consumed by Task 4.
- Produces: `Store::list_sessions(&self, project_id: Option<i64>) -> Result<Vec<Session>>` — consumed by Task 4's Tauri command.
- Produces: `Store::get_session(&self, id: &str) -> Result<Session>` — used by this task's own tests and Task 2's tests.
- Produces (crate-internal, for Task 2): `pub(crate) fn touch_session_active_tx`, `bump_session_leased_tx`, `bump_session_completed_tx`, `bump_session_failed_tx` in `sessions.rs`, each `(conn: &rusqlite::Connection, session_id: &str[, client: &str]) -> rusqlite::Result<()>`.

- [ ] **Step 1: Write the migration**

Create `crates/magpie-core/migrations/0005_sessions.sql`:

```sql
-- One row per MCP connection (see crates/magpie-mcp/src/lib.rs's
-- MagpieServer::new / serve_stdio) -- what used to be an in-memory-only
-- UUID string now persists long enough for a future UI to show "who's
-- doing what". `client` starts NULL: MCP's clientInfo handshake result
-- isn't available until the first tool call, not at connection time (see
-- Store::touch_session_active). No expiry column on purpose -- sessions
-- end via liveness (graceful stdio-close or the dead-pid sweep), never a
-- timer; see docs/design.md "MCP contract".
CREATE TABLE sessions (
    id              TEXT PRIMARY KEY,
    client          TEXT,
    pid             INTEGER NOT NULL,
    project_id      INTEGER REFERENCES projects(id) ON DELETE SET NULL,
    branch          TEXT,
    started_at      TEXT NOT NULL,
    last_active_at  TEXT,
    ended_at        TEXT,
    leased_count    INTEGER NOT NULL DEFAULT 0,
    completed_count INTEGER NOT NULL DEFAULT 0,
    failed_count    INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX sessions_project_id_idx ON sessions (project_id);
CREATE INDEX sessions_active_idx ON sessions (id) WHERE ended_at IS NULL;
```

- [ ] **Step 2: Register the migration**

In `crates/magpie-core/src/db.rs`, change:

```rust
const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_init", include_str!("../migrations/0001_init.sql")),
    (
        "0002_screenshots",
        include_str!("../migrations/0002_screenshots.sql"),
    ),
    ("0003_packs", include_str!("../migrations/0003_packs.sql")),
    (
        "0004_project_recency",
        include_str!("../migrations/0004_project_recency.sql"),
    ),
];
```

to:

```rust
const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_init", include_str!("../migrations/0001_init.sql")),
    (
        "0002_screenshots",
        include_str!("../migrations/0002_screenshots.sql"),
    ),
    ("0003_packs", include_str!("../migrations/0003_packs.sql")),
    (
        "0004_project_recency",
        include_str!("../migrations/0004_project_recency.sql"),
    ),
    ("0005_sessions", include_str!("../migrations/0005_sessions.sql")),
];
```

- [ ] **Step 3: Run the existing migration test to verify it still passes**

Run: `cargo test -p magpie-core migrates_cleanly_and_is_idempotent`
Expected: PASS.

- [ ] **Step 4: Add the `Session` struct**

In `crates/magpie-core/src/model.rs`, add (after the `Project` struct, before `Source`):

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub client: Option<String>,
    pub pid: i64,
    pub project_id: Option<i64>,
    pub branch: Option<String>,
    pub started_at: String,
    pub last_active_at: Option<String>,
    pub ended_at: Option<String>,
    pub leased_count: i64,
    pub completed_count: i64,
    pub failed_count: i64,
}
```

- [ ] **Step 5: Add the `SessionNotFound` error variant**

In `crates/magpie-core/src/error.rs`, change:

```rust
    #[error("project {0} not found")]
    ProjectNotFound(i64),

    #[error("merge needs at least two captures")]
    MergeNeedsAtLeastTwo,
```

to:

```rust
    #[error("project {0} not found")]
    ProjectNotFound(i64),

    #[error("session {0} not found")]
    SessionNotFound(String),

    #[error("merge needs at least two captures")]
    MergeNeedsAtLeastTwo,
```

- [ ] **Step 6: Write the failing tests**

Create `crates/magpie-core/src/sessions.rs` with just the test module first (the real implementation is Step 8):

```rust
use rusqlite::{params, OptionalExtension, Row};

use crate::db::now_iso;
use crate::error::Result;
use crate::model::Session;
use crate::Store;

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
```

- [ ] **Step 7: Run the tests to verify they fail**

Run: `cargo test -p magpie-core sessions::`
Expected: FAIL to compile — `create_session`, `get_session`, `touch_session_active`, `end_session`, `list_active_sessions`, `list_sessions` don't exist yet.

- [ ] **Step 8: Implement the module**

Add above the test module in `crates/magpie-core/src/sessions.rs`:

```rust
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
    })
}

const SESSION_COLUMNS: &str = "id, client, pid, project_id, branch, started_at, \
     last_active_at, ended_at, leased_count, completed_count, failed_count";

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
            let rows =
                stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
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
                 ORDER BY started_at DESC"
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

fn get_session_tx(conn: &rusqlite::Connection, id: &str) -> Result<Session> {
    let sql = format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1");
    conn.query_row(&sql, params![id], session_from_row)
        .optional()?
        .ok_or_else(|| crate::error::Error::SessionNotFound(id.to_string()))
}
```

- [ ] **Step 9: Register the module and export `Session`**

In `crates/magpie-core/src/lib.rs`, change:

```rust
mod projects;
mod search;
mod sources;
```

to:

```rust
mod projects;
mod search;
mod sessions;
mod sources;
```

and change:

```rust
pub use model::{AuditEntry, Blob, Capture, Pack, Project, Source, Tag, Template};
```

to:

```rust
pub use model::{AuditEntry, Blob, Capture, Pack, Project, Session, Source, Tag, Template};
```

- [ ] **Step 10: Run the tests to verify they pass**

Run: `cargo test -p magpie-core sessions::`
Expected: PASS (7/7). Then run: `cargo test -p magpie-core`
Expected: PASS, all pre-existing tests unaffected.

- [ ] **Step 11: Commit**

```bash
git add crates/magpie-core/migrations/0005_sessions.sql \
        crates/magpie-core/src/db.rs \
        crates/magpie-core/src/model.rs \
        crates/magpie-core/src/error.rs \
        crates/magpie-core/src/sessions.rs \
        crates/magpie-core/src/lib.rs
git commit -m "Add sessions table and CRUD to magpie-core"
```

---

### Task 2: Wire lease activity into session counters

**Files:**
- Modify: `crates/magpie-core/src/lease.rs` (`queue_take`, `capture_complete`, `capture_fail`)
- Test: extend the existing `#[cfg(test)] mod tests` in `lease.rs`

**Interfaces:**
- Consumes: `crate::sessions::bump_session_leased_tx`, `bump_session_completed_tx`, `bump_session_failed_tx` from Task 1 — all `(conn: &rusqlite::Connection, session_id: &str) -> rusqlite::Result<()>`.

- [ ] **Step 1: Write the failing tests**

Add to the existing `#[cfg(test)] mod tests` block in `crates/magpie-core/src/lease.rs`:

```rust
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

        store.capture_fail(c.id, "sess-1", "couldn't find the file").unwrap();

        let session = store.get_session("sess-1").unwrap();
        assert_eq!(session.failed_count, 1);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p magpie-core queue_take_bumps`
Expected: FAIL — `leased_count` stays 0 (the bump doesn't exist yet).

- [ ] **Step 3: Wire the bumps into `queue_take`, `capture_complete`, `capture_fail`**

In `crates/magpie-core/src/lease.rs`, change `queue_take`'s body from:

```rust
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

            let leased = get_capture_tx(&tx, candidate.id)?;
            tx.commit()?;
            Ok(Some(leased))
```

to:

```rust
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
```

Change `capture_complete` from:

```rust
    pub fn capture_complete(&self, id: i64, session: &str) -> Result<Capture> {
        self.with_conn(|conn| {
            let client = require_lease_tx(conn, id, session)?;
            conn.execute(
                "UPDATE captures
                 SET done_at = ?1, lease_session = NULL, lease_client = NULL,
                     lease_pid = NULL, lease_at = NULL
                 WHERE id = ?2",
                params![now_iso(), id],
            )?;
            record_audit_tx(conn, &client, "capture_done", Some(id))?;
            get_capture_tx(conn, id)
        })
    }
```

to:

```rust
    pub fn capture_complete(&self, id: i64, session: &str) -> Result<Capture> {
        self.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            let client = require_lease_tx(&tx, id, session)?;
            tx.execute(
                "UPDATE captures
                 SET done_at = ?1, lease_session = NULL, lease_client = NULL,
                     lease_pid = NULL, lease_at = NULL
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
```

Change `capture_fail` from:

```rust
    pub fn capture_fail(&self, id: i64, session: &str, reason: &str) -> Result<Capture> {
        self.with_conn(|conn| {
            let client = require_lease_tx(conn, id, session)?;
            conn.execute(
                "UPDATE captures
                 SET failed_reason = ?1, lease_session = NULL, lease_client = NULL,
                     lease_pid = NULL, lease_at = NULL
                 WHERE id = ?2",
                params![reason, id],
            )?;
            record_audit_tx(conn, &client, "capture_fail", Some(id))?;
            get_capture_tx(conn, id)
        })
    }
```

to:

```rust
    pub fn capture_fail(&self, id: i64, session: &str, reason: &str) -> Result<Capture> {
        self.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            let client = require_lease_tx(&tx, id, session)?;
            tx.execute(
                "UPDATE captures
                 SET failed_reason = ?1, lease_session = NULL, lease_client = NULL,
                     lease_pid = NULL, lease_at = NULL
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
```

Note: `require_lease_tx`, `record_audit_tx`, and `get_capture_tx` all take `&rusqlite::Connection` — passing `&tx` (a `rusqlite::Transaction`) works via `Deref` coercion, the exact same pattern `queue_take` already uses above. No signature changes needed to those three helper functions.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p magpie-core` (full suite — this touches shared transaction logic, run everything, not just the new tests)
Expected: PASS, including every pre-existing test in `lease.rs` (`take_leases_one_item_in_queue_order`, `two_sessions_never_take_the_same_item`, `complete_requires_holding_the_lease`, `fail_releases_the_lease_and_is_immediately_retakeable`, `release_leases_for_session_only_touches_that_session`, `queue_scoped_by_project_never_leaks_across_projects`, `branch_constrained_items_are_invisible_to_other_branches`, `dead_pid_sweep_releases_stale_leases`) — these encode the lease concurrency/attribution contract and must be unaffected by wrapping `capture_complete`/`capture_fail` in an explicit transaction.

- [ ] **Step 5: Commit**

```bash
git add crates/magpie-core/src/lease.rs
git commit -m "Bump session counters on queue_take, capture_complete, capture_fail"
```

---

### Task 3: Create and end sessions from magpie-mcp

**Files:**
- Modify: `crates/magpie-mcp/src/lib.rs` (`MagpieServer::new`, `queue_take`/`capture_done`/`capture_fail` handlers, `serve_stdio`)

**Interfaces:**
- Consumes: `Store::create_session`, `Store::touch_session_active`, `Store::end_session` from Task 1.

- [ ] **Step 1: Make `MagpieServer::new` fallible and create the session row**

In `crates/magpie-mcp/src/lib.rs`, change:

```rust
    pub fn new(store: Arc<Store>, project: &DetectedProject, project_id: Option<i64>) -> Self {
        Self {
            store,
            session: uuid::Uuid::new_v4().to_string(),
            pid: std::process::id() as i64,
            project_id,
            branch: project.branch.clone(),
            tool_router: Self::tool_router(),
        }
    }
```

to:

```rust
    pub fn new(
        store: Arc<Store>,
        project: &DetectedProject,
        project_id: Option<i64>,
    ) -> magpie_core::Result<Self> {
        let session = uuid::Uuid::new_v4().to_string();
        let pid = std::process::id() as i64;
        let branch = project.branch.clone();
        store.create_session(&session, pid, project_id, branch.as_deref())?;
        Ok(Self {
            store,
            session,
            pid,
            project_id,
            branch,
            tool_router: Self::tool_router(),
        })
    }
```

- [ ] **Step 2: Add a best-effort activity-touch helper**

In the same `#[tool_router] impl MagpieServer` block, add near `identity`/`client_name`:

```rust
    /// Best-effort activity/client backfill for this session's row --
    /// failures here are logged, never surfaced as a tool error, since
    /// session tracking is observability, not correctness (leasing
    /// correctness never depended on this -- see docs/design.md "MCP
    /// contract").
    fn touch_session(&self, client: &str) {
        if let Err(e) = self.store.touch_session_active(&self.session, client) {
            eprintln!("magpie: failed to update session activity: {e}");
        }
    }
```

- [ ] **Step 3: Call it from `queue_take`, `capture_done`, `capture_fail`**

Change `queue_take` from:

```rust
    async fn queue_take(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client_name(&ctx);
        let identity = self.identity(&client);
        let item = self
            .store
            .queue_take(self.project_id, self.branch.as_deref(), &identity)
            .map_err(to_error)?
            .map(|c| McpCapture::from_capture(&self.store, c, "human-vetted (promoted to Now)"));
        to_json_result(&item)
    }
```

to:

```rust
    async fn queue_take(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client_name(&ctx);
        self.touch_session(&client);
        let identity = self.identity(&client);
        let item = self
            .store
            .queue_take(self.project_id, self.branch.as_deref(), &identity)
            .map_err(to_error)?
            .map(|c| McpCapture::from_capture(&self.store, c, "human-vetted (promoted to Now)"));
        to_json_result(&item)
    }
```

Change `capture_done` from:

```rust
    async fn capture_done(
        &self,
        Parameters(args): Parameters<CaptureIdArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client_name(&ctx);
        self.store
            .capture_complete(args.id, &self.identity(&client).session)
            .map_err(to_error)?;
        to_json_result(&serde_json::json!({ "ok": true }))
    }
```

to:

```rust
    async fn capture_done(
        &self,
        Parameters(args): Parameters<CaptureIdArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client_name(&ctx);
        self.touch_session(&client);
        self.store
            .capture_complete(args.id, &self.identity(&client).session)
            .map_err(to_error)?;
        to_json_result(&serde_json::json!({ "ok": true }))
    }
```

Change `capture_fail` from:

```rust
    async fn capture_fail(
        &self,
        Parameters(args): Parameters<FailArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client_name(&ctx);
        self.store
            .capture_fail(args.id, &self.identity(&client).session, &args.reason)
            .map_err(to_error)?;
        to_json_result(&serde_json::json!({ "ok": true }))
    }
```

to:

```rust
    async fn capture_fail(
        &self,
        Parameters(args): Parameters<FailArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client_name(&ctx);
        self.touch_session(&client);
        self.store
            .capture_fail(args.id, &self.identity(&client).session, &args.reason)
            .map_err(to_error)?;
        to_json_result(&serde_json::json!({ "ok": true }))
    }
```

`queue_peek`, `capture_add`, and `capture_search` are deliberately left untouched — they never derive `client_name`/`identity` today, and adding session-touch to them is out of scope for this plan (see Global Constraints: session activity tracking is scoped to the three lease-attributed tools).

- [ ] **Step 4: Update `serve_stdio`**

Change:

```rust
    let server = MagpieServer::new(store.clone(), &project, project_id);
    let session = server.session.clone();

    let running = server.serve(rmcp::transport::stdio()).await?;
    running.waiting().await?;

    store.release_leases_for_session(&session)?;
    Ok(())
```

to:

```rust
    let server = MagpieServer::new(store.clone(), &project, project_id)?;
    let session = server.session.clone();

    let running = server.serve(rmcp::transport::stdio()).await?;
    running.waiting().await?;

    store.release_leases_for_session(&session)?;
    store.end_session(&session)?;
    Ok(())
```

- [ ] **Step 5: Type-check and run existing tests**

Run: `cargo check -p magpie-mcp && cargo test -p magpie-mcp`
Expected: compiles cleanly (the `MagpieServer::new` signature change is a breaking API change within this same file/crate — the compiler will catch any call site this plan didn't already update); existing `project::tests` (4 tests) still pass.

Note: `MagpieServer::new`, `queue_take`, `capture_done`, `capture_fail`, and `touch_session` are `RequestContext`/`AppHandle`-style async glue with no direct unit tests in this codebase today (mirroring how `apps/desktop/src-tauri/src/capture_flow.rs`'s Tauri-coupled functions have none either — the underlying `Store` logic is what's tested, in Tasks 1 and 2). Verify this task by tracing the logic: for each of `queue_take`/`capture_done`/`capture_fail`, confirm `touch_session` runs before the store call that could error, so activity is recorded even if the subsequent store call fails (e.g. `capture_complete` erroring on a lease mismatch shouldn't suppress the fact that this session was just active).

- [ ] **Step 6: Commit**

```bash
git add crates/magpie-mcp/src/lib.rs
git commit -m "Create and end session rows from magpie-mcp's connection lifecycle"
```

---

### Task 4: Dead-pid sweep ends dead sessions; expose `list_sessions` to the desktop app

**Files:**
- Modify: `apps/desktop/src-tauri/src/dead_pid_sweep.rs`
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs` (register the new command)

**Interfaces:**
- Consumes: `Store::list_active_sessions`, `Store::end_session` from Task 1 (sweep); `Store::list_sessions` from Task 1 (Tauri command).

- [ ] **Step 1: Extend the sweep**

In `apps/desktop/src-tauri/src/dead_pid_sweep.rs`, change `sweep` from:

```rust
pub fn sweep(store: &Store) {
    let leases = match store.list_active_leases() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("magpie: could not list active leases for dead-pid sweep: {e}");
            return;
        }
    };

    for (capture_id, pid) in leases {
        if !process_is_alive(pid) {
            if let Err(e) = store.release_lease(capture_id) {
                eprintln!("magpie: failed to release dead lease on capture {capture_id}: {e}");
            } else {
                eprintln!(
                    "magpie: released lease on capture {capture_id} -- holding process {pid} is gone"
                );
            }
        }
    }
}
```

to:

```rust
pub fn sweep(store: &Store) {
    let leases = match store.list_active_leases() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("magpie: could not list active leases for dead-pid sweep: {e}");
            return;
        }
    };

    for (capture_id, pid) in leases {
        if !process_is_alive(pid) {
            if let Err(e) = store.release_lease(capture_id) {
                eprintln!("magpie: failed to release dead lease on capture {capture_id}: {e}");
            } else {
                eprintln!(
                    "magpie: released lease on capture {capture_id} -- holding process {pid} is gone"
                );
            }
        }
    }

    let sessions = match store.list_active_sessions() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("magpie: could not list active sessions for dead-pid sweep: {e}");
            return;
        }
    };

    for (session_id, pid) in sessions {
        if !process_is_alive(pid) {
            if let Err(e) = store.end_session(&session_id) {
                eprintln!("magpie: failed to end dead session {session_id}: {e}");
            } else {
                eprintln!("magpie: ended session {session_id} -- holding process {pid} is gone");
            }
        }
    }
}
```

- [ ] **Step 2: Add a test for the session half of the sweep**

Add to the existing `#[cfg(test)] mod tests` block in `dead_pid_sweep.rs` (the file currently only tests `process_is_alive` directly, since `sweep` itself needs a `Store` and isn't exercised there yet — this is the first `sweep`-level test in this file):

```rust
    #[test]
    fn sweep_ends_sessions_whose_pid_is_dead() {
        let store = magpie_core::Store::open_in_memory().unwrap();
        store
            .create_session("sess-dead", i32::MAX as i64, None, None)
            .unwrap();
        store
            .create_session("sess-alive", std::process::id() as i64, None, None)
            .unwrap();

        sweep(&store);

        let dead = store.get_session("sess-dead").unwrap();
        assert!(dead.ended_at.is_some());
        let alive = store.get_session("sess-alive").unwrap();
        assert!(alive.ended_at.is_none());
    }
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p desktop dead_pid_sweep::`
Expected: PASS (3/3 — the two pre-existing `process_is_alive` tests plus the new `sweep_ends_sessions_whose_pid_is_dead`).

- [ ] **Step 4: Add the `list_sessions` Tauri command**

In `apps/desktop/src-tauri/src/commands.rs`, change the import line:

```rust
use magpie_core::{AuditEntry, Blob, Capture, Project, Tag, Template};
```

to:

```rust
use magpie_core::{AuditEntry, Blob, Capture, Project, Session, Tag, Template};
```

Add, near `list_projects`:

```rust
#[tauri::command]
pub fn list_sessions(state: State<AppState>, project_id: Option<i64>) -> CmdResult<Vec<Session>> {
    map_err(state.store.list_sessions(project_id))
}
```

- [ ] **Step 5: Register the command**

In `apps/desktop/src-tauri/src/lib.rs`, add `commands::list_sessions,` to the `tauri::generate_handler![...]` list (append after `commands::instantiate_template_with_values,`, the current last entry).

- [ ] **Step 6: Type-check**

Run: `cargo check -p desktop`
Expected: compiles cleanly.

- [ ] **Step 7: Commit**

```bash
git add apps/desktop/src-tauri/src/dead_pid_sweep.rs \
        apps/desktop/src-tauri/src/commands.rs \
        apps/desktop/src-tauri/src/lib.rs
git commit -m "End dead sessions in the dead-pid sweep; expose list_sessions to the desktop app"
```

---

### Task 5: Update docs/design.md

**Files:**
- Modify: `docs/design.md`

**Interfaces:** None — documentation only, reflecting Tasks 1-4's finished behavior.

- [ ] **Step 1: Refresh the stale schema block**

In `docs/design.md`, the schema block under "Storage — SQLite, WAL, FTS5" is already stale (missing `projects.last_active_at` from a prior phase). Change:

```
projects(id, name, remote_url, common_git_dir)        -- identity from git remote
sources(id, app_name, bundle_id, window_title, url, captured_at)
captures(id, body, created_at, done_at, failed_reason,
         queue_pos NULLABLE,      -- non-null ⇒ in Now; value is order
         project_id NULLABLE,     -- null ⇒ Inbox
         branch NULLABLE,         -- non-null ⇒ only matching sessions may take
         lease_session NULLABLE, lease_client, lease_pid, lease_at,
         source_id, merged_into)
templates(id, title, body, created_at)                 -- persist; instantiate into Now
tags(id, name)  /  capture_tags(capture_id, tag_id)
blobs(id, capture_id, path, mime, width, height, ocr_text)
audit(id, at, actor, action, capture_id)               -- every MCP action, shown in GUI
captures_fts(body, ocr_text)                            -- FTS5
```

to:

```
projects(id, name, remote_url, common_git_dir, last_active_at)  -- identity from git remote
sources(id, app_name, bundle_id, window_title, url, captured_at)
captures(id, body, created_at, done_at, failed_reason,
         queue_pos NULLABLE,      -- non-null ⇒ in Now; value is order
         project_id NULLABLE,     -- null ⇒ Inbox
         branch NULLABLE,         -- non-null ⇒ only matching sessions may take
         lease_session NULLABLE, lease_client, lease_pid, lease_at,
         source_id, merged_into)
sessions(id, client, pid, project_id, branch, started_at,       -- one row per MCP
         last_active_at, ended_at,                              -- connection; no expiry --
         leased_count, completed_count, failed_count)            -- ends via liveness only
templates(id, title, body, created_at)                 -- persist; instantiate into Now
tags(id, name)  /  capture_tags(capture_id, tag_id)
blobs(id, capture_id, path, mime, width, height, ocr_text)
audit(id, at, actor, action, capture_id)               -- every MCP action, shown in GUI
captures_fts(body, ocr_text)                            -- FTS5
```

- [ ] **Step 2: Add a short subsection after "Projects and multi-session"**

In `docs/design.md`, after the existing paragraph that ends `"Auto-follows when one session is live; clickable to switch when several are. Degrades to a plain list when nothing is running."` (end of the "Projects and multi-session" section, right before the next `###` heading), add:

```markdown

**Sessions persist past the connection.** What used to be an in-memory-only UUID (held by
`MagpieServer` for the life of one stdio connection) is now a `sessions` row: client name
(backfilled from MCP's `clientInfo` on the first tool call), pid, project/branch, and running
counts of items leased/completed/failed. It ends the same two ways leases already recover —
gracefully on stdio close, or via the dead-pid sweep — never on a timer, for the same
non-idempotent-consumer reason leases have no expiry. This is what a future dock/main-window UI
reads to show "who's doing what" instead of reconstructing it from raw lease columns.
```

- [ ] **Step 3: Commit**

```bash
git add docs/design.md
git commit -m "Document the sessions table in docs/design.md"
```

---

## Self-Review Notes

- **Spec coverage:** Task 1 builds the table/model/CRUD. Task 2 wires the three lease-attributed tools' counters transactionally. Task 3 wires session creation/ending into the MCP connection lifecycle. Task 4 extends the existing dead-pid sweep (liveness-based recovery, no new mechanism) and exposes a read path for a future UI. Task 5 brings `docs/design.md` back in sync, including the block that was already stale before this plan.
- **Explicitly deferred, not silently dropped:** session-end digests (writing a stream-visible summary) and all UI/frontend work are named in Global Constraints as out of scope, with the reason each belongs to a later phase.
- **Type consistency check:** `Session` (Task 1, `model.rs`) fields match `SESSION_COLUMNS`/`session_from_row` exactly. `Store::create_session`'s signature `(id: &str, pid: i64, project_id: Option<i64>, branch: Option<&str>)` is used identically in Task 3's `MagpieServer::new` and Task 1's own tests. The `bump_session_*_tx`/`touch_session_active_tx` functions all take `(conn: &rusqlite::Connection, session_id: &str[, client: &str])` and are called with matching argument order from both Task 1's `impl Store` wrappers and Task 2's `lease.rs` call sites.
