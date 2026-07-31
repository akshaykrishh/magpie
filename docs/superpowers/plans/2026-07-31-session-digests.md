# Session Digests (Backend) — Phase 4 of the magpie Explorations canonical design Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a session ends, write a synthetic "here's what happened" summary that is a first-class, searchable citizen of the Stream — not a separate list a UI has to merge in — matching the original design's "SESSION DIGEST · WRITTEN BY MAGPIE · SEARCHABLE LIKE ANY CAPTURE."

**Architecture:** A session digest is not a new table — it's a `captures` row with a new `kind` discriminator column (`'capture'` for everything that exists today, `'session_digest'` for this). This is deliberately the cheapest design that satisfies "searchable like any capture": `list_stream`, `capture_search`, and the FTS sync triggers all already operate generically over every row in `captures` with no column-specific filtering, so a digest interleaves chronologically and becomes searchable with zero changes to any of those three. It's excluded from `Now`/`queue_take`/`queue_peek` for the same structural reason a capture is excluded before being promoted: nothing ever calls `promote()` on a digest, so its `queue_pos` stays `NULL`, which those queries already require to be non-null. `Store::end_session` (already the single method both the graceful stdio-close path and the dead-pid sweep call) is where the digest gets written, so both termination paths get this behavior for free — no other file needs to change.

**Tech Stack:** Rust (magpie-core: rusqlite/SQLite).

## Global Constraints

- **A digest is a capture, not a new table.** This is a deliberate trade-off, not an oversight: it means zero changes to `list_stream`, `capture_search`, `queue_take`/`queue_peek` (already verified to only exclude on `queue_pos IS NOT NULL`, never on any content column), and the FTS sync triggers (already fire unconditionally on every `INSERT ON captures`). Do not build a second table or a UNION query.
- **The digest's "N captured, M unpromoted" counts include every non-digest capture created during the session's lifetime, regardless of who or what created it.** There is no reliable existing signal to distinguish "a human typed this" from "the agent wrote this back via `capture_add`" (both go through `Store::capture(&body, None)` with no source). Do not word the digest text as if it knows the answer (no "you captured..." phrasing) — say what's actually known.
- **`Store::capture()` and any other existing `INSERT INTO captures` statement must NOT need to change.** The new `kind` column ships with a SQL-level `DEFAULT 'capture'`; any insert that doesn't explicitly list `kind` gets the correct value automatically. Verify this holds for every capture-insertion code path, not just the one already confirmed (`Store::capture`).
- **No new digest-specific error variant, no new Tauri command, no UI/frontend work.** `Session.captures_during_session`/`unpromoted_at_end` and the digest capture itself flow through the *existing* `list_sessions`/`list_stream`/`capture_search` — nothing under `apps/desktop/src/**` is touched by this plan.
- Migration files go in `crates/magpie-core/migrations/000N_<name>.sql`, registered as a new tuple appended to `MIGRATIONS` in `crates/magpie-core/src/db.rs`. The next free number is `0007`.
- Match existing code style: doc comments explain *why*, not *what*.

---

### Task 1: `kind` discriminator on `captures`; session stat columns; model plumbing

**Files:**
- Create: `crates/magpie-core/migrations/0007_session_digests.sql`
- Modify: `crates/magpie-core/src/db.rs` (register migration)
- Modify: `crates/magpie-core/src/model.rs` (`Capture.kind`, `Capture::is_session_digest()`, `Session.captures_during_session`, `Session.unpromoted_at_end`)
- Modify: `crates/magpie-core/src/captures.rs` (`CAPTURE_COLUMNS`, `capture_from_row`, new test)
- Modify: `crates/magpie-core/src/sessions.rs` (`SESSION_COLUMNS`, `session_from_row`)
- Test: inline, in `captures.rs`

**Interfaces:**
- Produces: `Capture.kind: String` (`"capture"` or `"session_digest"`) and `Capture::is_session_digest(&self) -> bool` — consumed by Task 2's tests and any future UI.
- Produces: `Session.captures_during_session: Option<i64>`, `Session.unpromoted_at_end: Option<i64>` — both `None` until a session ends; consumed by Task 2.

- [ ] **Step 1: Grep for every `INSERT INTO captures` in the workspace**

Run: `grep -rn "INSERT INTO captures" --include="*.rs" crates/`

Confirm each call site either (a) doesn't list a column set at all (relies on `CAPTURE_COLUMNS`-style full-row construction, which won't exist for INSERTs), or (b) explicitly lists columns and would need `kind` added to that list if it does. As of this plan's writing, `Store::capture` (`crates/magpie-core/src/captures.rs`) is confirmed to use `INSERT INTO captures (body, created_at, source_id) VALUES (...)` — a partial column list that leaves `kind` to its `DEFAULT 'capture'`. If the grep finds any other `INSERT INTO captures` statement (e.g. for screenshot captures), apply the same reasoning: if it lists specific columns and doesn't include `kind`, it's already safe (the default applies); only a statement using `INSERT INTO captures VALUES (...)` positionally with *every* column listed would need updating, and none should exist in this codebase (`CAPTURE_COLUMNS`-driven code always uses named columns for `SELECT`, not positional `INSERT`). Note what you found in your task report; no code change is expected here.

- [ ] **Step 2: Write the migration**

Create `crates/magpie-core/migrations/0007_session_digests.sql`:

```sql
-- A session digest is a captures row, not a new table: `kind` distinguishes
-- a synthetic "here's what happened when this session ended" summary
-- (written by Store::end_session) from everything a human or agent
-- actually captured. This is what makes a digest "searchable like any
-- capture" for free -- list_stream, capture_search, and the FTS sync
-- triggers already operate over every captures row with no column
-- filtering, so nothing about those needs to change for a digest to show
-- up in the stream and in search results. It's excluded from Now the same
-- way an un-promoted capture already is: nothing ever calls promote() on
-- a digest, so its queue_pos stays NULL.
ALTER TABLE captures ADD COLUMN kind TEXT NOT NULL DEFAULT 'capture';

-- Set once, when the session ends (see Store::end_session) -- NULL for a
-- still-active session. Counts every non-digest capture created between
-- the session's started_at and its ended_at, regardless of who or what
-- created it (there's no reliable signal to attribute a capture to "the
-- human" vs. "the agent's capture_add" -- both go through the same
-- Store::capture(&body, None) path).
ALTER TABLE sessions ADD COLUMN captures_during_session INTEGER;
ALTER TABLE sessions ADD COLUMN unpromoted_at_end INTEGER;
```

- [ ] **Step 3: Register the migration**

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
    (
        "0006_handback",
        include_str!("../migrations/0006_handback.sql"),
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
    (
        "0006_handback",
        include_str!("../migrations/0006_handback.sql"),
    ),
    (
        "0007_session_digests",
        include_str!("../migrations/0007_session_digests.sql"),
    ),
];
```

- [ ] **Step 4: Run the existing migration test**

Run: `cargo test -p magpie-core migrates_cleanly_and_is_idempotent`
Expected: PASS.

- [ ] **Step 5: Add the model fields**

In `crates/magpie-core/src/model.rs`, change the `Capture` struct from:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capture {
    pub id: i64,
    pub body: String,
    pub created_at: String,
    pub done_at: Option<String>,
    pub failed_reason: Option<String>,
```

to:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capture {
    pub id: i64,
    /// `"capture"` for everything a human or agent captured; `"session_digest"`
    /// for a synthetic summary magpie writes when a session ends (see
    /// `Store::end_session`). Both live in the same stream and are both
    /// searchable -- a digest is a special kind of row, not a special table.
    pub kind: String,
    pub body: String,
    pub created_at: String,
    pub done_at: Option<String>,
    pub failed_reason: Option<String>,
```

Change `impl Capture` from:

```rust
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

to:

```rust
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

    pub fn is_session_digest(&self) -> bool {
        self.kind == "session_digest"
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
    pub handback_count: i64,
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
    pub captures_during_session: Option<i64>,
    pub unpromoted_at_end: Option<i64>,
}
```

- [ ] **Step 6: Write the failing test**

Add to `crates/magpie-core/src/captures.rs`'s existing `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn new_captures_default_to_kind_capture() {
        let store = Store::open_in_memory().unwrap();
        let c = store.capture("something", None).unwrap();
        assert_eq!(c.kind, "capture");
        assert!(!c.is_session_digest());
    }
```

- [ ] **Step 7: Run the test to verify it fails**

Run: `cargo test -p magpie-core new_captures_default_to_kind_capture`
Expected: FAIL to compile — `Capture` has no `kind` field yet.

- [ ] **Step 8: Update `CAPTURE_COLUMNS`/`capture_from_row` and `SESSION_COLUMNS`/`session_from_row`**

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

to:

```rust
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
    })
}

pub(crate) const CAPTURE_COLUMNS: &str =
    "id, kind, body, created_at, done_at, failed_reason, queue_pos, \
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
        handback_count: row.get("handback_count")?,
    })
}

const SESSION_COLUMNS: &str = "id, client, pid, project_id, branch, started_at, \
     last_active_at, ended_at, leased_count, completed_count, failed_count, handback_count";
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
        captures_during_session: row.get("captures_during_session")?,
        unpromoted_at_end: row.get("unpromoted_at_end")?,
    })
}

const SESSION_COLUMNS: &str = "id, client, pid, project_id, branch, started_at, \
     last_active_at, ended_at, leased_count, completed_count, failed_count, handback_count, \
     captures_during_session, unpromoted_at_end";
```

- [ ] **Step 9: Run the tests to verify they pass**

Run: `cargo test -p magpie-core`
Expected: PASS, all tests including every pre-existing one — this is an additive-columns change with a SQL default, not a behavior change to anything else.

- [ ] **Step 10: Commit**

```bash
git add crates/magpie-core/migrations/0007_session_digests.sql \
        crates/magpie-core/src/db.rs \
        crates/magpie-core/src/model.rs \
        crates/magpie-core/src/captures.rs \
        crates/magpie-core/src/sessions.rs
git commit -m "Add kind discriminator to captures and session digest stat columns"
```

---

### Task 2: `Store::end_session` writes a digest

**Files:**
- Modify: `crates/magpie-core/src/sessions.rs`

**Interfaces:**
- `Store::end_session`'s signature (`&self, id: &str) -> Result<()>`) is unchanged — both existing callers (`crates/magpie-mcp/src/lib.rs`'s `serve_stdio`, `apps/desktop/src-tauri/src/dead_pid_sweep.rs`'s `sweep`) need no modification and are not part of this task's file list.

- [ ] **Step 1: Write the failing tests**

Add to `crates/magpie-core/src/sessions.rs`'s existing `#[cfg(test)] mod tests` block:

```rust
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
        store
            .create_session("sess-1", 111, None, None)
            .unwrap();
        store.touch_session_active("sess-1", "claude-code").unwrap();
        store.end_session("sess-1").unwrap();

        let results = store.search("claude-code", 10).unwrap();
        assert!(
            results.iter().any(|c| c.is_session_digest()),
            "the digest body should mention the client name and be findable by it"
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

        assert!(store.queue_peek(Some(proj.id), None, 10).unwrap().is_empty());
        let identity = crate::lease::LeaseIdentity {
            session: "sess-2".to_string(),
            client: "someone-else".to_string(),
            pid: 222,
        };
        store.create_session("sess-2", 222, Some(proj.id), None).unwrap();
        assert!(store
            .queue_take(Some(proj.id), None, &identity)
            .unwrap()
            .is_none());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p magpie-core end_session_writes_a_digest_capture`
Expected: FAIL — no digest is written yet (the assertion `digests.len(), 1` fails against an empty stream, or the test fails to compile if `is_session_digest`/`kind` aren't wired — depends on Task 1 having landed first, which it must have).

- [ ] **Step 3: Implement digest writing in `end_session`**

In `crates/magpie-core/src/sessions.rs`, change:

```rust
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
```

to:

```rust
    /// Marks a session ended (graceful stdio-close, or the dead-pid sweep
    /// confirming its process is gone) and writes a digest capture
    /// summarizing what happened -- see `format_digest_body`. Idempotent in
    /// the sense that ending an already-ended session never errors (a dead-
    /// pid sweep and a graceful close could plausibly race on the same
    /// session), though calling this twice does write a second digest and
    /// re-run the counts against a later `now` -- acceptable since the
    /// realistic race is "ends once, from whichever path notices first."
    pub fn end_session(&self, id: &str) -> Result<()> {
        self.with_conn(|conn| {
            let session = get_session_tx(conn, id)?;
            let now = now_iso();

            let captures_during: i64 = conn.query_row(
                "SELECT COUNT(*) FROM captures
                 WHERE created_at >= ?1 AND created_at <= ?2 AND kind = 'capture'",
                params![session.started_at, now],
                |r| r.get(0),
            )?;
            let unpromoted: i64 = conn.query_row(
                "SELECT COUNT(*) FROM captures
                 WHERE created_at >= ?1 AND created_at <= ?2 AND kind = 'capture'
                   AND queue_pos IS NULL AND done_at IS NULL",
                params![session.started_at, now],
                |r| r.get(0),
            )?;

            conn.execute(
                "UPDATE sessions
                 SET ended_at = ?1, captures_during_session = ?2, unpromoted_at_end = ?3
                 WHERE id = ?4",
                params![now, captures_during, unpromoted, id],
            )?;

            let body = format_digest_body(&session, captures_during, unpromoted);
            conn.execute(
                "INSERT INTO captures (kind, body, created_at, project_id, branch)
                 VALUES ('session_digest', ?1, ?2, ?3, ?4)",
                params![body, now, session.project_id, session.branch],
            )?;

            Ok(())
        })
    }
```

Add a new private function after `end_session` (still inside `sessions.rs`, outside `impl Store`):

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p magpie-core`
Expected: PASS, all tests including every pre-existing one (in particular, `dead_pid_sweep_releases_stale_leases` and the Phase 2 session tests, since `end_session`'s behavior changed but its signature and error contract didn't).

- [ ] **Step 5: Commit**

```bash
git add crates/magpie-core/src/sessions.rs
git commit -m "Write a session digest capture when a session ends"
```

---

### Task 3: Update docs/design.md

**Files:**
- Modify: `docs/design.md`

**Interfaces:** None — documentation only, reflecting Tasks 1-2's finished behavior.

- [ ] **Step 1: Refresh the schema block**

In `docs/design.md`, change the `captures(...)` and `sessions(...)` lines (as they stand after the prior phase's edit) from:

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

to:

```
captures(id, kind DEFAULT 'capture',  -- 'session_digest' for a synthetic
                                       -- summary; same table, same stream,
                                       -- same search -- see "Session digests"
         body, created_at, done_at, failed_reason,
         queue_pos NULLABLE,      -- non-null ⇒ in Now; value is order
         project_id NULLABLE,     -- null ⇒ Inbox
         branch NULLABLE,         -- non-null ⇒ only matching sessions may take
         lease_session NULLABLE, lease_client, lease_pid, lease_at, lease_head_commit,
         handback_note, diff_stat, handback_at,  -- set by capture_handback; diff_stat
                                                  -- is git-computed, never agent-reported
         source_id, merged_into)
sessions(id, client, pid, project_id, branch, started_at,       -- one row per MCP
         last_active_at, ended_at,                              -- connection; no expiry --
         leased_count, completed_count, failed_count, handback_count,  -- liveness-ended
         captures_during_session, unpromoted_at_end)                   -- only; both set once,
                                                                        -- at end_session time
```

- [ ] **Step 2: Add a short subsection**

In `docs/design.md`, immediately after the paragraph about handback added by the prior phase (the one ending `"...a number the agent could be wrong about is worse than no number, so magpie computes what it can verify and says nothing when it can't."`), add:

```markdown

**Session digests are captures, not a separate feed.** When a session ends (`Store::end_session` --
called on graceful stdio-close and by the dead-pid sweep, so both recovery paths get this for
free), magpie writes one summary row into `captures` itself, with `kind = 'session_digest'`
instead of the default `'capture'`. This is the whole mechanism -- `list_stream`, `capture_search`,
and the FTS sync triggers all already operate over every row in `captures` with no column-specific
filtering, so a digest shows up in the stream in chronological order and is searchable exactly like
anything else, without a UNION query or a second index. It never appears in `Now`/`queue_take`/
`queue_peek` for the same structural reason an un-promoted capture doesn't: nothing ever calls
`promote()` on a digest, so its `queue_pos` stays `NULL`.
```

- [ ] **Step 3: Commit**

```bash
git add docs/design.md
git commit -m "Document session digests in docs/design.md"
```

---

## Self-Review Notes

- **Spec coverage:** Task 1 adds the `kind` discriminator and the two new session stat columns, with a grep-first step verifying no other insert path needs updating. Task 2 is the actual mechanism — computing the two stats and writing the digest, entirely inside `end_session`, so both existing callers (graceful close, dead-pid sweep) inherit the behavior with zero changes to either. Task 3 documents it.
- **Explicitly deferred, not silently dropped:** "who captured what" attribution (human vs. agent) is explicitly out of scope — Global Constraints state why (no reliable signal exists) and the digest's own wording avoids claiming it.
- **Type consistency check:** `Capture.kind`/`is_session_digest()` (Task 1) match what Task 2's tests assert (`c.is_session_digest()`). `Session.captures_during_session`/`unpromoted_at_end` (Task 1) are exactly the two columns `end_session` (Task 2) writes and what Task 2's tests read back via `store.get_session(...)`. `end_session`'s signature is unchanged end to end, which is what lets Task 2 avoid touching `crates/magpie-mcp/src/lib.rs` or `apps/desktop/src-tauri/src/dead_pid_sweep.rs` at all.
