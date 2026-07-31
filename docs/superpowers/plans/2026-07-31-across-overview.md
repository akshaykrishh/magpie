# Cross-Project Overview (Backend) — Phase 5 of the magpie Explorations canonical design Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One read that answers "what needs a look, across every project" — the data behind the canonical design's "Across (⌘⌥K)" surface — without giving up per-project scoping and without inventing any new query logic.

**Architecture:** A single new `Store` method, `list_projects_overview`, composes three methods that already exist and are already tested (`list_now`, `list_sessions`, `list_projects_by_recency`) rather than writing new SQL. For each project (recency-ordered, matching the dock's own ordering) plus a pinned `Inbox` pseudo-entry first, it reports: how many items are in Now, how many of those are leased, how many are awaiting human review (handed back), and how many sessions are currently live. This is backend-only — no Tauri window, no React component — the one new Tauri command this plan adds is purely an IPC contract for a UI to consume later, matching the precedent set by `list_sessions` in an earlier phase.

**Tech Stack:** Rust (magpie-core; apps/desktop/src-tauri: one Tauri command).

## Global Constraints

- **No new SQL.** `list_projects_overview` must be implemented by calling `Store::list_now`, `Store::list_sessions`, and `Store::list_projects_by_recency` and aggregating their results in Rust — not by writing a new query against `captures`/`sessions`/`projects` directly. This keeps the overview permanently in sync with whatever those three methods already guarantee (e.g. handback exclusion from Now-adjacent counts, liveness-based session state) without needing to duplicate that logic.
- **Inbox is always included, even with zero projects.** Captures with no project are a real queue of their own (see `docs/design.md` "one stream, one working set") — the overview must never silently omit it.
- **No UI/frontend work.** The one Tauri command this plan adds (`list_projects_overview`) is an IPC contract only — nothing calls it from `apps/desktop/src/**` yet.
- Match existing code style: doc comments explain *why*, not *what*.

---

### Task 1: `ProjectOverview` and `Store::list_projects_overview`

**Files:**
- Modify: `crates/magpie-core/src/projects.rs` (new struct, new method, new tests)
- Modify: `crates/magpie-core/src/lib.rs` (export `ProjectOverview`)

**Interfaces:**
- Produces: `pub struct ProjectOverview { project_id: Option<i64>, project_name: String, now_count: i64, leased_count: i64, needs_review_count: i64, active_session_count: i64 }` — consumed by Task 2.
- Produces: `Store::list_projects_overview(&self) -> Result<Vec<ProjectOverview>>` — consumed by Task 2.

- [ ] **Step 1: Write the failing tests**

Add to `crates/magpie-core/src/projects.rs`'s existing `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn list_projects_overview_includes_inbox_even_with_no_projects() {
        let store = Store::open_in_memory().unwrap();
        let overview = store.list_projects_overview().unwrap();
        assert_eq!(overview.len(), 1);
        assert_eq!(overview[0].project_id, None);
        assert_eq!(overview[0].project_name, "Inbox");
        assert_eq!(overview[0].now_count, 0);
    }

    #[test]
    fn list_projects_overview_counts_now_leased_and_needs_review() {
        let store = Store::open_in_memory().unwrap();
        let proj = store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();
        store
            .create_session("sess-1", 111, Some(proj.id), None)
            .unwrap();
        let identity = crate::lease::LeaseIdentity {
            session: "sess-1".to_string(),
            client: "claude-code".to_string(),
            pid: 111,
        };

        // Leased and stays leased -- taken immediately while it's the only
        // item available, so ordering can't put the wrong item in its hands.
        let leased_item = store.capture("leased", None).unwrap();
        store.assign_project(leased_item.id, Some(proj.id)).unwrap();
        store.promote(leased_item.id).unwrap();
        store.queue_take(Some(proj.id), None, &identity).unwrap();

        // Leased, then handed back for review.
        let review_item = store.capture("review", None).unwrap();
        store.assign_project(review_item.id, Some(proj.id)).unwrap();
        store.promote(review_item.id).unwrap();
        store.queue_take(Some(proj.id), None, &identity).unwrap();
        store
            .capture_handback(review_item.id, "sess-1", "note", None)
            .unwrap();

        // Stays open, never leased -- promoted last so it's never the
        // candidate either of the two queue_take calls above would reach.
        let open_item = store.capture("open", None).unwrap();
        store.assign_project(open_item.id, Some(proj.id)).unwrap();
        store.promote(open_item.id).unwrap();

        let overview = store.list_projects_overview().unwrap();
        let proj_overview = overview
            .iter()
            .find(|o| o.project_id == Some(proj.id))
            .unwrap();
        assert_eq!(proj_overview.now_count, 3);
        assert_eq!(proj_overview.leased_count, 1);
        assert_eq!(proj_overview.needs_review_count, 1);
    }

    #[test]
    fn list_projects_overview_counts_only_active_sessions() {
        let store = Store::open_in_memory().unwrap();
        let proj = store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();
        store
            .create_session("sess-1", 111, Some(proj.id), None)
            .unwrap();
        store
            .create_session("sess-2", 222, Some(proj.id), None)
            .unwrap();
        store.end_session("sess-2").unwrap();

        let overview = store.list_projects_overview().unwrap();
        let proj_overview = overview
            .iter()
            .find(|o| o.project_id == Some(proj.id))
            .unwrap();
        assert_eq!(proj_overview.active_session_count, 1);
    }

    #[test]
    fn list_projects_overview_orders_projects_by_recency_after_inbox() {
        let store = Store::open_in_memory().unwrap();
        let a = store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();
        let b = store
            .get_or_create_project("b", Some("git@github.com:x/b.git"), None)
            .unwrap();
        // Re-touch a so it outranks b despite being created (and thus
        // having a lower id) first -- same fixture shape as
        // list_projects_by_recency_orders_touched_projects_first, above.
        store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();

        let overview = store.list_projects_overview().unwrap();
        assert_eq!(overview[0].project_id, None, "Inbox is always first");
        assert_eq!(overview[1].project_id, Some(a.id));
        assert_eq!(overview[2].project_id, Some(b.id));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p magpie-core list_projects_overview`
Expected: FAIL to compile — `ProjectOverview`/`list_projects_overview` don't exist yet.

- [ ] **Step 3: Add `ProjectOverview` and `list_projects_overview`**

In `crates/magpie-core/src/projects.rs`, add near the top (after the existing `use` block, before `fn project_from_row`):

```rust
use serde::Serialize;
```

Add, after `list_projects_by_recency` and before the closing `}` of `impl Store`:

```rust

    /// A per-project rollup for a cross-project view (the canonical
    /// design's "Across ⌘⌥K" surface -- see docs/design.md): how much is
    /// queued, how much of that is already claimed, how much is waiting on
    /// a human, and how many sessions are live right now. Composed
    /// entirely from `list_now`/`list_sessions`/`list_projects_by_recency`
    /// rather than new SQL, so it can never drift out of sync with what
    /// those already guarantee. Inbox is always first, even with zero
    /// projects -- captures with no project are a real queue of their own.
    pub fn list_projects_overview(&self) -> Result<Vec<ProjectOverview>> {
        let all_sessions = self.list_sessions(None)?;
        let active_sessions_for = |project_id: Option<i64>| {
            all_sessions
                .iter()
                .filter(|s| s.project_id == project_id && s.ended_at.is_none())
                .count() as i64
        };

        let mut overviews = Vec::new();

        let inbox_now = self.list_now(None)?;
        overviews.push(ProjectOverview {
            project_id: None,
            project_name: "Inbox".to_string(),
            now_count: inbox_now.len() as i64,
            leased_count: inbox_now.iter().filter(|c| c.is_leased()).count() as i64,
            needs_review_count: inbox_now.iter().filter(|c| c.needs_review()).count() as i64,
            active_session_count: active_sessions_for(None),
        });

        // A generous upper bound rather than a true "no limit" --
        // list_projects_by_recency always takes one; no real user has
        // anywhere near this many projects.
        for project in self.list_projects_by_recency(10_000)? {
            let now_items = self.list_now(Some(project.id))?;
            overviews.push(ProjectOverview {
                project_id: Some(project.id),
                project_name: project.name.clone(),
                now_count: now_items.len() as i64,
                leased_count: now_items.iter().filter(|c| c.is_leased()).count() as i64,
                needs_review_count: now_items.iter().filter(|c| c.needs_review()).count() as i64,
                active_session_count: active_sessions_for(Some(project.id)),
            });
        }

        Ok(overviews)
    }
```

Add the struct definition after the `impl Store` block closes (before `#[cfg(test)]`):

```rust

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProjectOverview {
    pub project_id: Option<i64>,
    pub project_name: String,
    pub now_count: i64,
    pub leased_count: i64,
    pub needs_review_count: i64,
    pub active_session_count: i64,
}
```

- [ ] **Step 4: Export `ProjectOverview`**

In `crates/magpie-core/src/lib.rs`, change:

```rust
pub use captures::NewSource;
pub use db::{default_blobs_dir, default_db_path, now_iso, Store};
pub use error::{Error, Result};
pub use export::CaptureExport;
pub use lease::LeaseIdentity;
pub use model::{AuditEntry, Blob, Capture, Pack, Project, Session, Source, Tag, Template};
pub use packs::{ParsedPack, ParsedPrompt};
```

to:

```rust
pub use captures::NewSource;
pub use db::{default_blobs_dir, default_db_path, now_iso, Store};
pub use error::{Error, Result};
pub use export::CaptureExport;
pub use lease::LeaseIdentity;
pub use model::{AuditEntry, Blob, Capture, Pack, Project, Session, Source, Tag, Template};
pub use packs::{ParsedPack, ParsedPrompt};
pub use projects::ProjectOverview;
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p magpie-core`
Expected: PASS, all tests including every pre-existing one — this is a purely additive, composition-only method.

- [ ] **Step 6: Commit**

```bash
git add crates/magpie-core/src/projects.rs crates/magpie-core/src/lib.rs
git commit -m "Add list_projects_overview to magpie-core"
```

---

### Task 2: Expose `list_projects_overview` as a Tauri command

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs` (register the command)

**Interfaces:**
- Consumes: `Store::list_projects_overview` from Task 1.

- [ ] **Step 1: Add the command**

In `apps/desktop/src-tauri/src/commands.rs`, change the import line:

```rust
use magpie_core::{AuditEntry, Blob, Capture, Project, Session, Tag, Template};
```

to:

```rust
use magpie_core::{AuditEntry, Blob, Capture, Project, ProjectOverview, Session, Tag, Template};
```

Add, immediately after `list_sessions`:

```rust
#[tauri::command]
pub fn list_projects_overview(state: State<AppState>) -> CmdResult<Vec<ProjectOverview>> {
    map_err(state.store.list_projects_overview())
}
```

- [ ] **Step 2: Register the command**

In `apps/desktop/src-tauri/src/lib.rs`, change the last entry in the `tauri::generate_handler![...]` list from:

```rust
            commands::instantiate_template_with_values,
            commands::list_sessions,
        ])
```

to:

```rust
            commands::instantiate_template_with_values,
            commands::list_sessions,
            commands::list_projects_overview,
        ])
```

- [ ] **Step 3: Type-check**

Run: `cargo check -p desktop`
Expected: compiles cleanly.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src-tauri/src/commands.rs apps/desktop/src-tauri/src/lib.rs
git commit -m "Expose list_projects_overview as a Tauri command"
```

---

### Task 3: Update docs/design.md

**Files:**
- Modify: `docs/design.md`

**Interfaces:** None — documentation only, reflecting Tasks 1-2's finished behavior. No schema change in this plan, so the schema code block is unaffected.

- [ ] **Step 1: Add a short paragraph to "Projects and multi-session"**

In `docs/design.md`, the "Projects and multi-session" section currently ends with the paragraph added by an earlier phase:

```markdown
**Sessions persist past the connection.** What used to be an in-memory-only UUID (held by
`MagpieServer` for the life of one stdio connection) is now a `sessions` row: client name
(backfilled from MCP's `clientInfo` on the first tool call), pid, project/branch, and running
counts of items leased/completed/failed. It ends the same two ways leases already recover —
gracefully on stdio close, or via the dead-pid sweep — never on a timer, for the same
non-idempotent-consumer reason leases have no expiry. This is what a future dock/main-window UI
reads to show "who's doing what" instead of reconstructing it from raw lease columns.
```

Immediately after this paragraph (still inside the "Projects and multi-session" section, before the next `###` heading), add:

```markdown

**Across is the cross-project rollup, not a fourth queue.** The canonical design's "Across (⌘⌥K)"
surface needs one read that answers "what needs a look, across every project" without giving up
per-project scoping -- `Store::list_projects_overview` is that read. Inbox comes first (a queue of
its own, even without a project), then every project ordered by the same recency signal the dock
already uses, each annotated with how much is queued, how much is already claimed, how much is
waiting on a human, and how many sessions are live right now. It's composed entirely from
`list_now`/`list_sessions`/`list_projects_by_recency` -- no new table, no new query logic that
could drift out of sync with what those already guarantee.
```

- [ ] **Step 2: Commit**

```bash
git add docs/design.md
git commit -m "Document the cross-project overview in docs/design.md"
```

---

## Self-Review Notes

- **Spec coverage:** Task 1 is the entire mechanism — a composition-only method with no new SQL, exactly matching the plan's central design decision. Task 2 exposes it as a dormant IPC contract, matching the `list_sessions` precedent from an earlier phase. Task 3 documents it, introducing the "Across" concept in `docs/design.md` for the first time (confirmed via research that no such text existed there before this plan).
- **Explicitly deferred, not silently dropped:** all UI/frontend work is out of scope per Global Constraints — the Tauri command is unused by `apps/desktop/src/**` until a future phase.
- **Type consistency check:** `ProjectOverview`'s six fields (Task 1) are exactly what the Tauri command (Task 2) returns via `Vec<ProjectOverview>`, and exactly what Task 1's own tests assert on by field name (`project_id`, `now_count`, `leased_count`, `needs_review_count`, `active_session_count`). `list_projects_overview`'s only three dependencies (`list_now`, `list_sessions`, `list_projects_by_recency`) all already exist with the exact signatures used in this plan's code.
