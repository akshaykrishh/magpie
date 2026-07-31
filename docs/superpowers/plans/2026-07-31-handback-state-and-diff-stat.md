# Handed-Back State + Git-Computed Diff Stat (Backend) — Phase 3 of the magpie Explorations canonical design Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give an agent a third way to resolve a leased item — `capture_handback`, for "I did something, but a human should look before this counts as done" — alongside the existing `capture_done`/`capture_fail`, with a diff stat magpie computes itself (via git, in the item's own worktree) rather than trusting an agent's self-report.

**Architecture:** `captures` gains four columns: `lease_head_commit` (the git HEAD at the moment an item was leased, recorded best-effort by magpie-mcp right after `queue_take` succeeds) and `handback_note`/`diff_stat`/`handback_at` (set together by a new `Store::capture_handback`). At handback time, magpie-mcp shells `git diff --stat <lease_head_commit>` in its own working directory — the same technique `project::detect()` already uses for `git remote`/`git rev-parse` — rather than accepting a number from the agent. A handed-back item stays in `Now` (visible, not done) but is excluded from `queue_take`/`queue_peek` until a human closes it out via the pre-existing `mark_done` — no new "review" mechanism is built in this plan.

**Tech Stack:** Rust (magpie-core: rusqlite/SQLite; magpie-mcp: rmcp over stdio, `std::process::Command` for git).

## Global Constraints

- **The diff stat is computed by magpie, never trusted from the agent.** This mirrors the reasoning behind Phase 1's confidence-aware toast: a self-reported number is a claim, not a fact. `capture_handback`'s MCP-facing arguments are `{id, note}` only — there is no `diff_stat` parameter an agent can pass in.
- **No new "review" tool or UI.** A human closes out a handed-back item with the exact same `mark_done` path (`Store::mark_done`, already exposed via the Tauri `mark_capture_done` command) that closes out any other item. This plan adds no new Tauri commands and touches nothing under `apps/desktop/src/**`.
- **A handed-back item must not be immediately re-takeable.** `queue_take` and `queue_peek` both need a `handback_at IS NULL` filter alongside their existing `lease_session IS NULL` filter — without this, `capture_handback` clearing the lease would make the item pop right back into the take-queue, defeating the point of a review state.
- **Git failures degrade to `diff_stat: None`, never an error.** `crates/magpie-mcp/src/project.rs`'s existing `git()` helper already treats "no git", "not a repo", and "command failed" as `None` rather than propagating an error (see `detect()`). New git-shelling helpers in this plan follow the identical convention — a handback with no computable diff still succeeds, just without a stat.
- Migration files go in `crates/magpie-core/migrations/000N_<name>.sql`, registered as a new tuple appended to `MIGRATIONS` in `crates/magpie-core/src/db.rs`. The next free number is `0006`.
- Match existing code style: doc comments explain *why*, not *what*.
- `require_lease_tx`, `Error::NotLeased`, and `Error::LeaseMismatch` already exist and are reused as-is for `capture_handback`'s lease check — no new error variant is needed.

---

### Task 1: Schema, `Capture`/`Session` model fields, and column plumbing

**Files:**
- Create: `crates/magpie-core/migrations/0006_handback.sql`
- Modify: `crates/magpie-core/src/db.rs` (register migration)
- Modify: `crates/magpie-core/src/model.rs` (`Capture` fields, `Session.handback_count`)
- Modify: `crates/magpie-core/src/captures.rs` (`CAPTURE_COLUMNS`, `capture_from_row`, new test)
- Modify: `crates/magpie-core/src/sessions.rs` (`SESSION_COLUMNS`, `session_from_row`, new `bump_session_handback_tx`, extend an existing test)
- Test: inline, in `captures.rs` and `sessions.rs`

**Interfaces:**
- Produces: `Capture.lease_head_commit: Option<String>`, `Capture.handback_note: Option<String>`, `Capture.diff_stat: Option<String>`, `Capture.handback_at: Option<String>` — consumed by Task 2.
- Produces: `Capture::needs_review(&self) -> bool` — `true` when `handback_at.is_some() && done_at.is_none()`.
- Produces: `Session.handback_count: i64`.
- Produces: `pub(crate) fn bump_session_handback_tx(conn: &rusqlite::Connection, session_id: &str) -> rusqlite::Result<()>` in `sessions.rs` — consumed by Task 2.

- [ ] **Step 1: Write the migration**

Create `crates/magpie-core/migrations/0006_handback.sql`:

```sql
-- A third way to resolve a leased item, alongside done/fail: the agent did
-- something but wants a human to look before it counts as finished. See
-- Store::capture_handback (crates/magpie-core/src/lease.rs) and
-- capture_handback in crates/magpie-mcp/src/lib.rs.
--
-- lease_head_commit is set best-effort right after queue_take succeeds
-- (crates/magpie-mcp/src/lib.rs), and is what a later capture_handback
-- diffs against via `git diff --stat <commit>` -- never a number the agent
-- self-reports. It's cleared everywhere the other lease_* columns already
-- are (capture_complete, capture_fail, capture_handback itself,
-- release_leases_for_session, release_lease), since it's part of the same
-- "who currently holds this, and from when" lease state.
ALTER TABLE captures ADD COLUMN lease_head_commit TEXT;
ALTER TABLE captures ADD COLUMN handback_note TEXT;
ALTER TABLE captures ADD COLUMN diff_stat TEXT;
ALTER TABLE captures ADD COLUMN handback_at TEXT;

ALTER TABLE sessions ADD COLUMN handback_count INTEGER NOT NULL DEFAULT 0;
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
    (
        "0005_sessions",
        include_str!("../migrations/0005_sessions.sql"),
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
    (
        "0005_sessions",
        include_str!("../migrations/0005_sessions.sql"),
    ),
    ("0006_handback", include_str!("../migrations/0006_handback.sql")),
];
```

- [ ] **Step 3: Run the existing migration test**

Run: `cargo test -p magpie-core migrates_cleanly_and_is_idempotent`
Expected: PASS.

- [ ] **Step 4: Add the new `Capture` fields and `Session.handback_count`**

In `crates/magpie-core/src/model.rs`, change the `Capture` struct from:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capture {
    pub id: i64,
    pub body: String,
    pub created_at: String,
    pub done_at: Option<String>,
    pub failed_reason: Option<String>,

    pub queue_pos: Option<f64>,
    pub project_id: Option<i64>,
    pub branch: Option<String>,

    pub lease_session: Option<String>,
    pub lease_client: Option<String>,
    pub lease_pid: Option<i64>,
    pub lease_at: Option<String>,

    pub source_id: Option<i64>,
    pub merged_into: Option<i64>,
}

impl Capture {
    pub fn in_now(&self) -> bool {
        self.queue_pos.is_some()
    }

    pub fn is_leased(&self) -> bool {
        self.lease_session.is_some()
    }
}
```

to:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capture {
    pub id: i64,
    pub body: String,
    pub created_at: String,
    pub done_at: Option<String>,
    pub failed_reason: Option<String>,

    pub queue_pos: Option<f64>,
    pub project_id: Option<i64>,
    pub branch: Option<String>,

    pub lease_session: Option<String>,
    pub lease_client: Option<String>,
    pub lease_pid: Option<i64>,
    pub lease_at: Option<String>,
    pub lease_head_commit: Option<String>,

    pub handback_note: Option<String>,
    pub diff_stat: Option<String>,
    pub handback_at: Option<String>,

    pub source_id: Option<i64>,
    pub merged_into: Option<i64>,
}

impl Capture {
    pub fn in_now(&self) -> bool {
        self.queue_pos.is_some()
    }

    pub fn is_leased(&self) -> bool {
        self.lease_session.is_some()
    }

    /// A handed-back item stays in Now (it isn't done) but isn't active
    /// work either -- this is what a future UI checks to render the
    /// "needs review" state distinctly from "leased" or "open".
    pub fn needs_review(&self) -> bool {
        self.handback_at.is_some() && self.done_at.is_none()
    }
}
```

Change the `Session` struct from:

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

to:

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
    pub handback_count: i64,
}
```

- [ ] **Step 5: Write the failing tests**

Add to `crates/magpie-core/src/captures.rs`'s existing `#[cfg(test)] mod tests` block:

```rust
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
```

In `crates/magpie-core/src/sessions.rs`'s existing test module, change:

```rust
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
```

to:

```rust
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
```

- [ ] **Step 6: Run the tests to verify they fail**

Run: `cargo test -p magpie-core new_captures_have_no_lease_or_handback_state`
Expected: FAIL to compile — `Capture` has no `lease_head_commit`/`handback_note`/`diff_stat`/`handback_at`/`needs_review` yet.

- [ ] **Step 7: Update `CAPTURE_COLUMNS`/`capture_from_row` and `SESSION_COLUMNS`/`session_from_row`**

In `crates/magpie-core/src/captures.rs`, change:

```rust
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
```

to:

```rust
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
        lease_head_commit: row.get("lease_head_commit")?,
        handback_note: row.get("handback_note")?,
        diff_stat: row.get("diff_stat")?,
        handback_at: row.get("handback_at")?,
        source_id: row.get("source_id")?,
        merged_into: row.get("merged_into")?,
    })
}

pub(crate) const CAPTURE_COLUMNS: &str =
    "id, body, created_at, done_at, failed_reason, queue_pos, \
     project_id, branch, lease_session, lease_client, lease_pid, lease_at, lease_head_commit, \
     handback_note, diff_stat, handback_at, source_id, merged_into";
```

In `crates/magpie-core/src/sessions.rs`, change:

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
```

to:

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
        handback_count: row.get("handback_count")?,
    })
}

const SESSION_COLUMNS: &str = "id, client, pid, project_id, branch, started_at, \
     last_active_at, ended_at, leased_count, completed_count, failed_count, handback_count";
```

- [ ] **Step 8: Add `bump_session_handback_tx`**

In `crates/magpie-core/src/sessions.rs`, add after `bump_session_failed_tx`:

```rust
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
```

- [ ] **Step 9: Run the tests to verify they pass**

Run: `cargo test -p magpie-core`
Expected: PASS, all tests (including every pre-existing one) — this is an additive-columns change plus one modified assertion in an existing test, not a behavior change to anything else.

- [ ] **Step 10: Commit**

```bash
git add crates/magpie-core/migrations/0006_handback.sql \
        crates/magpie-core/src/db.rs \
        crates/magpie-core/src/model.rs \
        crates/magpie-core/src/captures.rs \
        crates/magpie-core/src/sessions.rs
git commit -m "Add handback columns to captures and sessions"
```

---

### Task 2: `capture_handback`, `record_lease_head_commit`, and excluding handed-back items from the take-queue

**Files:**
- Modify: `crates/magpie-core/src/lease.rs`

**Interfaces:**
- Consumes: `crate::sessions::bump_session_handback_tx` from Task 1.
- Produces: `Store::record_lease_head_commit(&self, id: i64, session: &str, commit: &str) -> Result<()>` — consumed by Task 4.
- Produces: `Store::capture_handback(&self, id: i64, session: &str, note: &str, diff_stat: Option<&str>) -> Result<Capture>` — consumed by Task 4.

- [ ] **Step 1: Write the failing tests**

Add to `crates/magpie-core/src/lease.rs`'s existing `#[cfg(test)] mod tests` block:

```rust
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
        assert_eq!(handed_back.handback_note.as_deref(), Some("not sure this is right"));
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

        store.capture_handback(c.id, "sess-1", "note", None).unwrap();

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
        store.capture_handback(c.id, "sess-1", "note", None).unwrap();

        assert!(store.queue_peek(None, None, 10).unwrap().is_empty());
        assert!(store.queue_take(None, None, &identity("sess-1")).unwrap().is_none());
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
            store.get_capture(c.id).unwrap().lease_head_commit.as_deref(),
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
        store.record_lease_head_commit(a.id, "sess-1", "deadbeef").unwrap();
        store.queue_take(None, None, &identity("sess-1")).unwrap();
        store.record_lease_head_commit(b.id, "sess-1", "deadbeef").unwrap();

        store.capture_complete(a.id, "sess-1").unwrap();
        store.capture_fail(b.id, "sess-1", "nope").unwrap();

        assert!(store.get_capture(a.id).unwrap().lease_head_commit.is_none());
        assert!(store.get_capture(b.id).unwrap().lease_head_commit.is_none());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p magpie-core capture_handback`
Expected: FAIL to compile — `capture_handback`/`record_lease_head_commit` don't exist yet.

- [ ] **Step 3: Add `handback_at IS NULL` to `queue_take` and `queue_peek`**

In `crates/magpie-core/src/lease.rs`, change `queue_take`'s candidate query from:

```rust
            let sql = format!(
                "SELECT {CAPTURE_COLUMNS} FROM captures
                 WHERE project_id IS ?1
                   AND queue_pos IS NOT NULL
                   AND done_at IS NULL
                   AND lease_session IS NULL
                   AND (branch IS NULL OR branch IS ?2)
                 ORDER BY queue_pos ASC
                 LIMIT 1"
            );
```

to:

```rust
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
```

Change `queue_peek`'s query from:

```rust
            let sql = format!(
                "SELECT {CAPTURE_COLUMNS} FROM captures
                 WHERE project_id IS ?1
                   AND queue_pos IS NOT NULL
                   AND done_at IS NULL
                   AND lease_session IS NULL
                   AND (branch IS NULL OR branch IS ?2)
                 ORDER BY queue_pos ASC
                 LIMIT ?3"
            );
```

to:

```rust
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
```

- [ ] **Step 4: Clear `lease_head_commit` alongside the other `lease_*` columns**

In `capture_complete`, change:

```rust
            tx.execute(
                "UPDATE captures
                 SET done_at = ?1, lease_session = NULL, lease_client = NULL,
                     lease_pid = NULL, lease_at = NULL
                 WHERE id = ?2",
                params![now_iso(), id],
            )?;
```

to:

```rust
            tx.execute(
                "UPDATE captures
                 SET done_at = ?1, lease_session = NULL, lease_client = NULL,
                     lease_pid = NULL, lease_at = NULL, lease_head_commit = NULL
                 WHERE id = ?2",
                params![now_iso(), id],
            )?;
```

In `capture_fail`, change:

```rust
            tx.execute(
                "UPDATE captures
                 SET failed_reason = ?1, lease_session = NULL, lease_client = NULL,
                     lease_pid = NULL, lease_at = NULL
                 WHERE id = ?2",
                params![reason, id],
            )?;
```

to:

```rust
            tx.execute(
                "UPDATE captures
                 SET failed_reason = ?1, lease_session = NULL, lease_client = NULL,
                     lease_pid = NULL, lease_at = NULL, lease_head_commit = NULL
                 WHERE id = ?2",
                params![reason, id],
            )?;
```

In `release_leases_for_session`, change:

```rust
            let n = conn.execute(
                "UPDATE captures
                 SET lease_session = NULL, lease_client = NULL,
                     lease_pid = NULL, lease_at = NULL
                 WHERE lease_session = ?1",
                params![session],
            )?;
```

to:

```rust
            let n = conn.execute(
                "UPDATE captures
                 SET lease_session = NULL, lease_client = NULL,
                     lease_pid = NULL, lease_at = NULL, lease_head_commit = NULL
                 WHERE lease_session = ?1",
                params![session],
            )?;
```

In `release_lease`, change:

```rust
            conn.execute(
                "UPDATE captures
                 SET lease_session = NULL, lease_client = NULL,
                     lease_pid = NULL, lease_at = NULL
                 WHERE id = ?1",
                params![id],
            )?;
```

to:

```rust
            conn.execute(
                "UPDATE captures
                 SET lease_session = NULL, lease_client = NULL,
                     lease_pid = NULL, lease_at = NULL, lease_head_commit = NULL
                 WHERE id = ?1",
                params![id],
            )?;
```

- [ ] **Step 5: Add `record_lease_head_commit`**

Add to the `impl Store` block in `lease.rs`, after `queue_take`:

```rust
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
```

- [ ] **Step 6: Add `capture_handback`**

Add to the `impl Store` block, after `capture_fail`:

```rust
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
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test -p magpie-core`
Expected: PASS, all tests including every pre-existing lease test (`take_leases_one_item_in_queue_order`, `two_sessions_never_take_the_same_item`, `complete_requires_holding_the_lease`, `fail_releases_the_lease_and_is_immediately_retakeable`, `release_leases_for_session_only_touches_that_session`, `queue_scoped_by_project_never_leaks_across_projects`, `branch_constrained_items_are_invisible_to_other_branches`, `dead_pid_sweep_releases_stale_leases`, and the four Session-counter tests from the prior phase) — the new `handback_at IS NULL` filter must not change behavior for any item that was never handed back (`handback_at` is `NULL` by default for every existing row and test fixture).

- [ ] **Step 8: Commit**

```bash
git add crates/magpie-core/src/lease.rs
git commit -m "Add capture_handback and record_lease_head_commit to magpie-core"
```

---

### Task 3: Git helpers for HEAD commit and diff stat

**Files:**
- Modify: `crates/magpie-mcp/src/project.rs`

**Interfaces:**
- Produces: `pub fn head_commit() -> Option<String>` — consumed by Task 4.
- Produces: `pub fn diff_stat(against: &str) -> Option<String>` — consumed by Task 4.

- [ ] **Step 1: Write the failing tests**

Add to `crates/magpie-mcp/src/project.rs`'s existing `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn head_commit_runs_without_panicking_outside_a_repo() {
        let _ = head_commit();
    }

    #[test]
    fn diff_stat_runs_without_panicking_outside_a_repo() {
        let _ = diff_stat("HEAD");
    }

    #[test]
    fn diff_stat_against_garbage_ref_returns_none_not_a_panic() {
        assert!(diff_stat("not-a-real-ref-xyz").is_none());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p magpie-mcp head_commit_runs_without_panicking`
Expected: FAIL to compile — `head_commit`/`diff_stat` don't exist yet.

- [ ] **Step 3: Implement the helpers**

In `crates/magpie-mcp/src/project.rs`, add after `detect()` (before the private `git()` helper):

```rust
/// The current commit, for stamping onto a freshly leased item (see
/// Store::record_lease_head_commit) so a later handback can diff against
/// it. `None` outside a repo or if `git` isn't on PATH -- same fallback
/// `detect()` already uses.
pub fn head_commit() -> Option<String> {
    git(&["rev-parse", "HEAD"])
}

/// A summary of what changed since `against` (typically a `lease_head_commit`)
/// in the current working tree -- staged, unstaged, and committed changes
/// all included, since `git diff --stat <ref>` (with no second ref) compares
/// a single commit against the live working tree rather than two commits.
/// `None` on any failure (bad ref, no git, not a repo, nothing changed) --
/// callers treat a missing diff stat as "couldn't compute one", never as an
/// error (see docs/design.md "MCP contract").
pub fn diff_stat(against: &str) -> Option<String> {
    git(&["diff", "--stat", against])
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p magpie-mcp`
Expected: PASS, all tests including the pre-existing `project::tests` (`extracts_name_from_ssh_remote`, `extracts_name_from_https_remote`, `extracts_name_from_common_git_dir`, `detect_runs_without_panicking_outside_a_repo`).

- [ ] **Step 5: Commit**

```bash
git add crates/magpie-mcp/src/project.rs
git commit -m "Add head_commit and diff_stat git helpers to magpie-mcp"
```

---

### Task 4: Wire `queue_take` to record the lease head commit; add the `capture_handback` tool

**Files:**
- Modify: `crates/magpie-mcp/src/lib.rs`

**Interfaces:**
- Consumes: `Store::record_lease_head_commit`, `Store::capture_handback` from Task 2; `project::head_commit`, `project::diff_stat` from Task 3.

- [ ] **Step 1: Add `HandbackArgs`**

In `crates/magpie-mcp/src/lib.rs`, add after `FailArgs`:

```rust
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct HandbackArgs {
    id: i64,
    /// What you did and why it needs a human look before this counts as
    /// done -- e.g. "renamed the config loader but couldn't verify the
    /// migration path still works". The diff itself is computed
    /// automatically; describe the *why*, not the *what changed*.
    note: String,
}
```

- [ ] **Step 2: Record the lease head commit after a successful `queue_take`**

Change `queue_take` from:

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
            .map_err(to_error)?;
        if let (Some(capture), Some(commit)) = (&item, project::head_commit()) {
            if let Err(e) =
                self.store
                    .record_lease_head_commit(capture.id, &identity.session, &commit)
            {
                eprintln!("magpie: failed to record lease head commit: {e}");
            }
        }
        let item =
            item.map(|c| McpCapture::from_capture(&self.store, c, "human-vetted (promoted to Now)"));
        to_json_result(&item)
    }
```

- [ ] **Step 3: Add the `capture_handback` tool**

Add after `capture_fail`:

```rust
    #[tool(
        description = "Hand a leased item back for human review instead of marking it done -- \
                        use when you made a real attempt but want a human to look before this \
                        counts as finished. A diff stat is computed automatically from what \
                        changed since the item was leased; just explain why it needs a look."
    )]
    async fn capture_handback(
        &self,
        Parameters(args): Parameters<HandbackArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client_name(&ctx);
        self.touch_session(&client);
        let session = self.identity(&client).session;
        let capture = self.store.get_capture(args.id).map_err(to_error)?;
        let diff_stat = capture
            .lease_head_commit
            .as_deref()
            .and_then(project::diff_stat);
        self.store
            .capture_handback(args.id, &session, &args.note, diff_stat.as_deref())
            .map_err(to_error)?;
        to_json_result(&serde_json::json!({ "ok": true }))
    }
```

- [ ] **Step 4: Type-check and run existing tests**

Run: `cargo check -p magpie-mcp && cargo check --workspace && cargo test -p magpie-mcp`
Expected: compiles cleanly; all 4 pre-existing `project::tests` plus Task 3's 3 new ones pass (7/7 total in that crate).

Note: like `queue_take`/`capture_done`/`capture_fail`, `capture_handback` is `RequestContext`-coupled async glue with no direct unit test in this codebase (the underlying `Store` logic is what Task 2 tests). Verify this task by tracing: does `capture_handback` read the capture's `lease_head_commit` (set by Task 4's own `queue_take` change) before computing the diff, and does a missing `lease_head_commit` (e.g. a pre-migration item, or a git failure at lease time) degrade to `diff_stat: None` rather than failing the whole handback?

- [ ] **Step 5: Commit**

```bash
git add crates/magpie-mcp/src/lib.rs
git commit -m "Wire queue_take to record lease head commit; add capture_handback tool"
```

---

### Task 5: Update docs/design.md

**Files:**
- Modify: `docs/design.md`

**Interfaces:** None — documentation only, reflecting Tasks 1-4's finished behavior.

- [ ] **Step 1: Refresh the schema block**

In `docs/design.md`, change the `captures(...)` and `sessions(...)` lines (as they stand after the prior phase's edit) from:

```
captures(id, body, created_at, done_at, failed_reason,
         queue_pos NULLABLE,      -- non-null ⇒ in Now; value is order
         project_id NULLABLE,     -- null ⇒ Inbox
         branch NULLABLE,         -- non-null ⇒ only matching sessions may take
         lease_session NULLABLE, lease_client, lease_pid, lease_at,
         source_id, merged_into)
sessions(id, client, pid, project_id, branch, started_at,       -- one row per MCP
         last_active_at, ended_at,                              -- connection; no expiry --
         leased_count, completed_count, failed_count)            -- ends via liveness only
```

to:

```
captures(id, body, created_at, done_at, failed_reason,
         queue_pos NULLABLE,      -- non-null ⇒ in Now; value is order
         project_id NULLABLE,     -- null ⇒ Inbox
         branch NULLABLE,         -- non-null ⇒ only matching sessions may take
         lease_session NULLABLE, lease_client, lease_pid, lease_at, lease_head_commit,
         handback_note, diff_stat, handback_at,  -- set by capture_handback; diff_stat
                                                  -- is git-computed, never agent-reported
         source_id, merged_into)
sessions(id, client, pid, project_id, branch, started_at,       -- one row per MCP
         last_active_at, ended_at,                              -- connection; no expiry --
         leased_count, completed_count, failed_count, handback_count)  -- liveness-ended only
```

- [ ] **Step 2: Update the MCP contract tool list**

In `docs/design.md`, change:

```
queue_peek(n)          read-only, for planning across items
queue_take()           leases exactly ONE item
capture_done(id)
capture_fail(id, why)  visible and actionable, not a zombie
capture_add(text)
capture_search(query)
```

to:

```
queue_peek(n)               read-only, for planning across items
queue_take()                 leases exactly ONE item
capture_done(id)
capture_fail(id, why)        visible and actionable, not a zombie
capture_handback(id, note)   done, but wants a human look -- diff stat computed by magpie
capture_add(text)
capture_search(query)
```

- [ ] **Step 3: Add a short paragraph after the tool list**

In `docs/design.md`, immediately after the code block from Step 2 (before the next paragraph, `**Lease and acknowledge, with no auto-expiry.**`), add:

```markdown

**Handback is a third outcome, not a review workflow.** `capture_handback` clears the lease and
sets a note plus a diff stat, but the item stays in Now and drops out of `queue_take`/`queue_peek`
until a human closes it with the same `capture_done` action that closes anything else -- no
separate review tool or UI exists yet. The diff stat is `git diff --stat` against the commit that
was HEAD when the item was leased, computed by magpie-mcp itself (never trusted from the agent) --
the same reasoning as the toast's confidence-aware filing: a number the agent could be wrong about
is worse than no number, so magpie computes what it can verify and says nothing when it can't.
```

- [ ] **Step 4: Commit**

```bash
git add docs/design.md
git commit -m "Document capture_handback and the diff-stat mechanism in docs/design.md"
```

---

## Self-Review Notes

- **Spec coverage:** Task 1 lays the schema/model groundwork. Task 2 builds the core `capture_handback`/`record_lease_head_commit` logic and — critically — closes the "immediately re-takeable" gap by adding `handback_at IS NULL` to both queue-reading functions, which the plan's own Global Constraints call out as a correctness requirement, not a nice-to-have. Task 3 adds the two git helpers, isolated and tested independently of the MCP layer. Task 4 wires them together at the only two call sites that need git access. Task 5 documents the finished mechanism, including the reasoning (matching Phase 1's toast) for why the diff stat is computed, not self-reported.
- **Explicitly deferred, not silently dropped:** no new "review" tool/UI is built — Global Constraints state a handed-back item closes via the pre-existing `mark_done`/`capture_done` Tauri path, unchanged by this plan.
- **Type consistency check:** `Capture.lease_head_commit`/`handback_note`/`diff_stat`/`handback_at` (Task 1) match `CAPTURE_COLUMNS`/`capture_from_row` exactly, and are the same field names `capture_handback` (Task 2) writes and `capture_handback`'s MCP handler (Task 4) reads (`capture.lease_head_commit`). `Store::record_lease_head_commit`'s `(id: i64, session: &str, commit: &str)` signature matches its one call site in Task 4's `queue_take` exactly. `project::head_commit() -> Option<String>` and `project::diff_stat(against: &str) -> Option<String>` (Task 3) match their call sites in Task 4 (`project::head_commit()` with no args; `project::diff_stat` called via `.and_then(project::diff_stat)` on an `Option<&str>` produced by `.as_deref()`, which matches `diff_stat`'s `&str` parameter).
