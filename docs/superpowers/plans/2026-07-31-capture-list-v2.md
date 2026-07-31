# Capture List v2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Sections, deletion (soft-delete/Undo/Recently Deleted), Markdown rendering, a context-menu interaction model (Copy/Copy as List/Edit/Expand/Move to Project/Move to Section/Merge/Delete), keyboard-first navigation, and remappable global hotkeys to magpie's capture list, stream, Now, and Templates views.

**Architecture:** Six sequential phases, each independently shippable: (1) schema foundation for sections and soft-delete, (2) deletion end-to-end (core + purge sweep + Undo + Recently Deleted), (3) Markdown rendering, (4) the context-menu interaction model (the biggest phase — Copy/Copy as List/Edit/Expand/Move to Project/Move to Section/Merge/Delete, section rendering, and an extended `MergeToolbar`), (5) keyboard-first navigation built on Phase 4's action handlers, (6) a Settings window for the two remappable global hotkeys. Full rationale for every decision below lives in `docs/superpowers/specs/2026-07-31-capture-list-v2-design.md` — this plan does not re-derive it, only implements it.

**Tech Stack:** Rust (`magpie-core`: rusqlite/SQLite/FTS5; `apps/desktop/src-tauri`: Tauri v2, `tauri-plugin-global-shortcut`, new: `tauri-plugin-clipboard-manager`), TypeScript/React 19 (`apps/desktop/src`, Tailwind v4, `@dnd-kit`, new: `react-markdown`).

## Global Constraints

- Migration files go in `crates/magpie-core/migrations/000N_<name>.sql`, registered as a new tuple in `db.rs`'s `MIGRATIONS` array. Next free number: `0008`.
- **No delete/purge MCP tool, ever, for anything.** `crates/magpie-mcp` is not touched anywhere in this plan. `docs/design.md`'s "Agent trust" section documents the MCP surface as deliberately non-destructive against prompt injection — nothing in this plan weakens that.
- Sections are global (not project-scoped, no `project_id` on `sections`), single-membership (a capture/template has at most one `section_id`), and never have their own item-ordering column — items keep whatever ordering already governs their view (`created_at DESC` in the stream, `queue_pos` in Now); only `sections.position` (fractional, like `queue_pos`) needs ordering.
- Every id-based lookup on a capture/template must include `deleted_at IS NULL` (or explicitly look past it, e.g. restore) — a soft-deleted row produces the exact same `Error::CaptureNotFound`/`Error::TemplateNotFound` a genuinely-missing row already produces. No new "already deleted" error variant.
- No confirmation dialogs on delete, anywhere, single or batch — soft-delete + a real Undo replaces them.
- Markdown rendering uses `react-markdown` **without** the `rehype-raw` plugin — captured content is untrusted (can originate from arbitrary web pages), and enabling raw-HTML passthrough would be stored XSS in the Tauri webview.
- The frontend has **no automated test framework** (no vitest/jest in `apps/desktop/package.json`) — do not introduce one unilaterally. Frontend task verification is manual: run `pnpm tauri dev` from `apps/desktop` and confirm the behavior described in each step.
- Match existing code style: doc comments explain *why*, not *what* (see any function in `captures.rs`/`projects.rs` for tone). Rust tests are inline `#[cfg(test)] mod tests` blocks, as in every existing `magpie-core` file.
- Any background thread that later touches a Tauri window/panel must dispatch through `AppHandle::run_on_main_thread` (see `apps/desktop/src-tauri/src/toast.rs`'s history) — not relevant to the purge-sweep thread in this plan (it only touches the SQLite `Store`), but stated here since it's an easy trap to fall back into.

---

## Phase 1 — Data model foundation

### Task 1: Migration `0008` — sections, section membership, soft-delete columns

**Files:**
- Create: `crates/magpie-core/migrations/0008_sections_and_soft_delete.sql`
- Modify: `crates/magpie-core/src/db.rs` (register migration)
- Modify: `crates/magpie-core/src/model.rs` (new `Section` struct; `section_id`/`deleted_at` on `Capture` and `Template`)
- Modify: `crates/magpie-core/src/captures.rs` (`capture_from_row`, `CAPTURE_COLUMNS`)
- Modify: `crates/magpie-core/src/templates.rs` (`template_from_row`, `TEMPLATE_COLUMNS`)
- Modify: `crates/magpie-core/src/lib.rs` (re-export `Section`)
- Test: inline in `captures.rs` and `templates.rs`

**Interfaces:**
- Produces: `Section { id: i64, name: String, position: f64, created_at: String, deleted_at: Option<String> }`
- Produces: `Capture.section_id: Option<i64>`, `Capture.deleted_at: Option<String>`
- Produces: `Template.section_id: Option<i64>`, `Template.deleted_at: Option<String>`

- [ ] **Step 1: Write the migration**

Create `crates/magpie-core/migrations/0008_sections_and_soft_delete.sql`:

```sql
-- Sections: a lightweight, global, ordered, single-membership grouping for
-- captures and templates -- distinct from tags (many-to-many, unordered).
-- See docs/superpowers/specs/2026-07-31-capture-list-v2-design.md.
CREATE TABLE sections (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    position REAL NOT NULL,
    created_at TEXT NOT NULL,
    deleted_at TEXT
);

ALTER TABLE captures ADD COLUMN section_id INTEGER REFERENCES sections(id);
ALTER TABLE templates ADD COLUMN section_id INTEGER REFERENCES sections(id);

-- Soft-delete: hidden from every view/search immediately, purged after
-- ~30 days. See "Deletion" in the design spec.
ALTER TABLE captures ADD COLUMN deleted_at TEXT;
ALTER TABLE templates ADD COLUMN deleted_at TEXT;

CREATE INDEX captures_section_id_idx ON captures (section_id) WHERE section_id IS NOT NULL;
CREATE INDEX templates_section_id_idx ON templates (section_id) WHERE section_id IS NOT NULL;
CREATE INDEX captures_deleted_at_idx ON captures (deleted_at) WHERE deleted_at IS NOT NULL;
CREATE INDEX templates_deleted_at_idx ON templates (deleted_at) WHERE deleted_at IS NOT NULL;
CREATE INDEX sections_deleted_at_idx ON sections (deleted_at) WHERE deleted_at IS NOT NULL;
```

- [ ] **Step 2: Register the migration**

In `crates/magpie-core/src/db.rs`, add to the `MIGRATIONS` array (after the `0007_session_digests` entry):

```rust
    (
        "0008_sections_and_soft_delete",
        include_str!("../migrations/0008_sections_and_soft_delete.sql"),
    ),
```

- [ ] **Step 3: Add the `Section` struct and new fields to `Capture`/`Template`**

In `crates/magpie-core/src/model.rs`, add (near `Tag`):

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub id: i64,
    pub name: String,
    pub position: f64,
    pub created_at: String,
    pub deleted_at: Option<String>,
}
```

Add to the end of the `Capture` struct (after `merged_into`):

```rust
    pub section_id: Option<i64>,
    pub deleted_at: Option<String>,
```

Add to the end of the `Template` struct (after `pack_id`):

```rust
    pub section_id: Option<i64>,
    pub deleted_at: Option<String>,
```

- [ ] **Step 4: Update row-mapping and column lists**

In `crates/magpie-core/src/captures.rs`, change `CAPTURE_COLUMNS` (line 41-44) to end with `, section_id, deleted_at"` and add to `capture_from_row` (after `merged_into: row.get("merged_into")?,`):

```rust
        section_id: row.get("section_id")?,
        deleted_at: row.get("deleted_at")?,
```

In `crates/magpie-core/src/templates.rs`, change `TEMPLATE_COLUMNS` (line 11-12) to:

```rust
pub(crate) const TEMPLATE_COLUMNS: &str =
    "id, title, body, created_at, description, variables_json, pack_id, section_id, deleted_at";
```

and add to `template_from_row` (after `pack_id: row.get("pack_id")?,`):

```rust
        section_id: row.get("section_id")?,
        deleted_at: row.get("deleted_at")?,
```

- [ ] **Step 5: Re-export `Section`**

In `crates/magpie-core/src/lib.rs`, change:

```rust
pub use model::{AuditEntry, Blob, Capture, Pack, Project, Session, Source, Tag, Template};
```

to:

```rust
pub use model::{AuditEntry, Blob, Capture, Pack, Project, Section, Session, Source, Tag, Template};
```

- [ ] **Step 6: Write a regression test in each affected file**

In `crates/magpie-core/src/captures.rs`'s `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn new_captures_default_to_no_section_and_not_deleted() {
        let store = Store::open_in_memory().unwrap();
        let c = store.capture("hello", None).unwrap();
        assert_eq!(c.section_id, None);
        assert_eq!(c.deleted_at, None);
    }
```

In `crates/magpie-core/src/templates.rs`'s `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn new_templates_default_to_no_section_and_not_deleted() {
        let store = Store::open_in_memory().unwrap();
        let t = store.create_template("title", "body").unwrap();
        assert_eq!(t.section_id, None);
        assert_eq!(t.deleted_at, None);
    }
```

- [ ] **Step 7: Run the full test suite**

Run: `cargo test -p magpie-core`
Expected: all existing tests still pass, plus the two new ones.

- [ ] **Step 8: Commit**

```bash
git add crates/magpie-core/migrations/0008_sections_and_soft_delete.sql \
        crates/magpie-core/src/db.rs crates/magpie-core/src/model.rs \
        crates/magpie-core/src/captures.rs crates/magpie-core/src/templates.rs \
        crates/magpie-core/src/lib.rs
git commit -m "Add sections table and soft-delete columns (migration 0008)"
```

---

### Task 2: Section CRUD (create, rename, list, reorder)

**Files:**
- Create: `crates/magpie-core/src/sections.rs`
- Modify: `crates/magpie-core/src/lib.rs` (add `mod sections;`)
- Test: inline in `sections.rs`

**Interfaces:**
- Consumes: `Section` (Task 1), `Store` (`crate::Store`), `now_iso()` (`crate::db`)
- Produces: `Store::create_section(&self, name: &str) -> Result<Section>`
- Produces: `Store::rename_section(&self, id: i64, name: &str) -> Result<Section>`
- Produces: `Store::list_sections(&self) -> Result<Vec<Section>>` (active only, ordered by `position`)
- Produces: `Store::reorder_section(&self, id: i64, after_id: Option<i64>) -> Result<Section>`
- Produces: `Store::get_section(&self, id: i64) -> Result<Section>`

- [ ] **Step 1: Write failing tests**

Create `crates/magpie-core/src/sections.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;

    #[test]
    fn create_and_get_round_trip() {
        let store = Store::open_in_memory().unwrap();
        let s = store.create_section("Research").unwrap();
        assert_eq!(s.name, "Research");
        assert_eq!(store.get_section(s.id).unwrap(), s);
    }

    #[test]
    fn rename_updates_the_name_only() {
        let store = Store::open_in_memory().unwrap();
        let s = store.create_section("Research").unwrap();
        let renamed = store.rename_section(s.id, "Config formats").unwrap();
        assert_eq!(renamed.name, "Config formats");
        assert_eq!(renamed.id, s.id);
    }

    #[test]
    fn list_sections_excludes_deleted_and_orders_by_position() {
        let store = Store::open_in_memory().unwrap();
        let a = store.create_section("A").unwrap();
        let b = store.create_section("B").unwrap();
        let list = store.list_sections().unwrap();
        assert_eq!(list.iter().map(|s| s.id).collect::<Vec<_>>(), vec![a.id, b.id]);
    }

    #[test]
    fn reorder_moves_a_section_after_another() {
        let store = Store::open_in_memory().unwrap();
        let a = store.create_section("A").unwrap();
        let b = store.create_section("B").unwrap();
        let c = store.create_section("C").unwrap();
        // Move A to after C: order becomes B, C, A.
        store.reorder_section(a.id, Some(c.id)).unwrap();
        let list = store.list_sections().unwrap();
        assert_eq!(
            list.iter().map(|s| s.id).collect::<Vec<_>>(),
            vec![b.id, c.id, a.id]
        );
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p magpie-core sections::`
Expected: FAIL to compile — `create_section` etc. don't exist yet.

- [ ] **Step 3: Implement**

Add above the `#[cfg(test)]` block in `sections.rs`:

```rust
use rusqlite::{params, OptionalExtension};

use crate::db::now_iso;
use crate::error::{Error, Result};
use crate::model::Section;
use crate::Store;

fn section_from_row(row: &rusqlite::Row) -> rusqlite::Result<Section> {
    Ok(Section {
        id: row.get("id")?,
        name: row.get("name")?,
        position: row.get("position")?,
        created_at: row.get("created_at")?,
        deleted_at: row.get("deleted_at")?,
    })
}

const SECTION_COLUMNS: &str = "id, name, position, created_at, deleted_at";

impl Store {
    /// New sections land after everything else -- one past the current
    /// max `position`, or `0.0` for the very first section.
    pub fn create_section(&self, name: &str) -> Result<Section> {
        self.with_conn(|conn| {
            let max_pos: Option<f64> = conn.query_row(
                "SELECT MAX(position) FROM sections WHERE deleted_at IS NULL",
                [],
                |r| r.get(0),
            )?;
            let position = max_pos.map(|p| p + 1.0).unwrap_or(0.0);
            conn.execute(
                "INSERT INTO sections (name, position, created_at) VALUES (?1, ?2, ?3)",
                params![name, position, now_iso()],
            )?;
            let id = conn.last_insert_rowid();
            get_section_tx(conn, id)
        })
    }

    pub fn rename_section(&self, id: i64, name: &str) -> Result<Section> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE sections SET name = ?1 WHERE id = ?2 AND deleted_at IS NULL",
                params![name, id],
            )?;
            get_section_tx(conn, id)
        })
    }

    pub fn get_section(&self, id: i64) -> Result<Section> {
        self.with_conn(|conn| get_section_tx(conn, id))
    }

    pub fn list_sections(&self) -> Result<Vec<Section>> {
        self.with_conn(|conn| {
            let sql = format!(
                "SELECT {SECTION_COLUMNS} FROM sections WHERE deleted_at IS NULL ORDER BY position ASC"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], section_from_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Same fractional-index technique as `Store::reorder` for captures:
    /// the new position is the midpoint between whatever now precedes and
    /// follows the target slot, so reordering never touches any other row.
    pub fn reorder_section(&self, id: i64, after_id: Option<i64>) -> Result<Section> {
        self.with_conn(|conn| {
            let after_pos: Option<f64> = match after_id {
                Some(after_id) => Some(conn.query_row(
                    "SELECT position FROM sections WHERE id = ?1 AND deleted_at IS NULL",
                    params![after_id],
                    |r| r.get(0),
                )?),
                None => None,
            };
            let next_pos: Option<f64> = conn
                .query_row(
                    "SELECT MIN(position) FROM sections
                     WHERE deleted_at IS NULL AND id != ?1
                       AND (?2 IS NULL OR position > ?2)",
                    params![id, after_pos],
                    |r| r.get(0),
                )
                .optional()?
                .flatten();
            let new_pos = match (after_pos, next_pos) {
                (Some(a), Some(n)) => (a + n) / 2.0,
                (Some(a), None) => a + 1.0,
                (None, Some(n)) => n - 1.0,
                (None, None) => 0.0,
            };
            conn.execute(
                "UPDATE sections SET position = ?1 WHERE id = ?2",
                params![new_pos, id],
            )?;
            get_section_tx(conn, id)
        })
    }
}

fn get_section_tx(conn: &rusqlite::Connection, id: i64) -> Result<Section> {
    let sql = format!("SELECT {SECTION_COLUMNS} FROM sections WHERE id = ?1");
    conn.query_row(&sql, params![id], section_from_row)
        .optional()?
        .ok_or(Error::SectionNotFound(id))
}
```

Add the new error variant to `crates/magpie-core/src/error.rs` (after `TemplateNotFound`):

```rust
    #[error("section {0} not found")]
    SectionNotFound(i64),
```

Register the module in `crates/magpie-core/src/lib.rs` (alongside the other `mod` declarations):

```rust
mod sections;
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p magpie-core sections::`
Expected: PASS (4 tests)

- [ ] **Step 5: Commit**

```bash
git add crates/magpie-core/src/sections.rs crates/magpie-core/src/error.rs crates/magpie-core/src/lib.rs
git commit -m "Add Section CRUD (create/rename/list/reorder)"
```

---

### Task 3: Section assignment for captures and templates

**Files:**
- Modify: `crates/magpie-core/src/captures.rs`
- Modify: `crates/magpie-core/src/templates.rs`

**Interfaces:**
- Consumes: `Store::get_section` (Task 2), `Error::SectionNotFound` (Task 2)
- Produces: `Store::assign_capture_section(&self, id: i64, section_id: Option<i64>) -> Result<Capture>`
- Produces: `Store::assign_template_section(&self, id: i64, section_id: Option<i64>) -> Result<Template>`

- [ ] **Step 1: Write failing tests**

In `crates/magpie-core/src/captures.rs`'s test module:

```rust
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
```

In `crates/magpie-core/src/templates.rs`'s test module:

```rust
    #[test]
    fn assign_template_section_round_trips() {
        let store = Store::open_in_memory().unwrap();
        let t = store.create_template("title", "body").unwrap();
        let s = store.create_section("Prompts").unwrap();
        let assigned = store.assign_template_section(t.id, Some(s.id)).unwrap();
        assert_eq!(assigned.section_id, Some(s.id));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p magpie-core assign_`
Expected: FAIL to compile — methods don't exist yet.

- [ ] **Step 3: Implement**

In `crates/magpie-core/src/captures.rs`, add to `impl Store`:

```rust
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
```

In `crates/magpie-core/src/templates.rs`, add to `impl Store`:

```rust
    pub fn assign_template_section(&self, id: i64, section_id: Option<i64>) -> Result<Template> {
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
                "UPDATE templates SET section_id = ?1 WHERE id = ?2 AND deleted_at IS NULL",
                params![section_id, id],
            )?;
            get_template_tx(conn, id)
        })
    }
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p magpie-core`
Expected: PASS (all tests, including the 3 new ones)

- [ ] **Step 5: Commit**

```bash
git add crates/magpie-core/src/captures.rs crates/magpie-core/src/templates.rs
git commit -m "Add section assignment for captures and templates"
```

---

## Phase 2 — Deletion

### Task 4: Soft-delete/restore for captures, with merge cascade

**Files:**
- Modify: `crates/magpie-core/src/captures.rs` (soft-delete/restore, `list_stream`/`list_now` filters)
- Modify: `crates/magpie-core/src/search.rs` (filter)

**Interfaces:**
- Produces: `Store::soft_delete_capture(&self, id: i64) -> Result<Capture>`
- Produces: `Store::restore_capture(&self, id: i64) -> Result<Capture>`
- Produces: `Store::list_recently_deleted_captures(&self) -> Result<Vec<Capture>>`

- [ ] **Step 1: Write failing tests**

In `crates/magpie-core/src/captures.rs`'s test module:

```rust
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
        // restores their state consistently.
        let a_row = store.get_capture(a.id).unwrap();
        assert!(a_row.deleted_at.is_some());

        store.restore_capture(merged.id).unwrap();
        let a_row = store.get_capture(a.id).unwrap();
        assert_eq!(a_row.deleted_at, None);
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
```

This test needs `Store::get_capture` — check whether it already exists; if not, add it in this same step (a simple single-row fetch used only by tests and the cascade check above):

```rust
    pub fn get_capture(&self, id: i64) -> Result<Capture> {
        self.with_conn(|conn| {
            let sql = format!("SELECT {CAPTURE_COLUMNS} FROM captures WHERE id = ?1");
            conn.query_row(&sql, params![id], capture_from_row)
                .optional()?
                .ok_or(Error::CaptureNotFound(id))
        })
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p magpie-core soft_delete\|recently_deleted\|cascades_to_its_absorbed`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

Add to `impl Store` in `captures.rs`:

```rust
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
            let sql = format!("SELECT {CAPTURE_COLUMNS} FROM captures WHERE id = ?1");
            conn.query_row(&sql, params![id], capture_from_row)
                .optional()?
                .ok_or(Error::CaptureNotFound(id))
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
            let sql = format!("SELECT {CAPTURE_COLUMNS} FROM captures WHERE id = ?1");
            conn.query_row(&sql, params![id], capture_from_row)
                .optional()?
                .ok_or(Error::CaptureNotFound(id))
        })
    }

    pub fn list_recently_deleted_captures(&self) -> Result<Vec<Capture>> {
        self.with_conn(|conn| {
            let sql = format!(
                "SELECT {CAPTURE_COLUMNS} FROM captures
                 WHERE deleted_at IS NOT NULL ORDER BY deleted_at DESC"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], capture_from_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }
```

Now add the `deleted_at IS NULL` filter to the existing `list_stream` and `list_now` queries — find their `WHERE` clauses and add the condition. For `list_now` (around line 112, `WHERE project_id IS ?1 AND queue_pos IS NOT NULL AND done_at IS NULL`), change to:

```rust
                 WHERE project_id IS ?1 AND queue_pos IS NOT NULL AND done_at IS NULL
                   AND deleted_at IS NULL
```

For `list_stream`, find its `WHERE` clause (filters on `project_id`/all) and add `AND deleted_at IS NULL` to it the same way.

In `crates/magpie-core/src/search.rs`, change the `search` query's `WHERE` clause:

```rust
                 WHERE captures_fts MATCH ?1 AND c.merged_into IS NULL AND c.deleted_at IS NULL
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p magpie-core`
Expected: PASS (all tests)

- [ ] **Step 5: Commit**

```bash
git add crates/magpie-core/src/captures.rs crates/magpie-core/src/search.rs
git commit -m "Add soft-delete/restore for captures with merge cascade"
```

---

### Task 5: Soft-delete/restore for templates

**Files:**
- Modify: `crates/magpie-core/src/templates.rs`

**Interfaces:**
- Consumes: existing `delete_template` signature (replaced)
- Produces: `Store::delete_template(&self, id: i64) -> Result<()>` (now soft-delete, same signature)
- Produces: `Store::restore_template(&self, id: i64) -> Result<Template>`
- Produces: `Store::list_recently_deleted_templates(&self) -> Result<Vec<Template>>`

- [ ] **Step 1: Write failing tests**

In `crates/magpie-core/src/templates.rs`'s test module:

```rust
    #[test]
    fn delete_template_is_soft_and_restore_reverses_it() {
        let store = Store::open_in_memory().unwrap();
        let t = store.create_template("title", "body").unwrap();

        store.delete_template(t.id).unwrap();
        assert!(store.list_templates().unwrap().is_empty());

        let restored = store.restore_template(t.id).unwrap();
        assert_eq!(restored.deleted_at, None);
        assert_eq!(store.list_templates().unwrap().len(), 1);
    }

    #[test]
    fn list_recently_deleted_templates_returns_only_deleted_ones() {
        let store = Store::open_in_memory().unwrap();
        let t = store.create_template("title", "body").unwrap();
        store.delete_template(t.id).unwrap();
        let deleted = store.list_recently_deleted_templates().unwrap();
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].id, t.id);
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p magpie-core delete_template_is_soft\|list_recently_deleted_templates`
Expected: FAIL — `delete_template` currently hard-deletes (test asserting `list_templates` still shows it as absent would actually already pass by coincidence via hard-delete, but `restore_template` doesn't exist, so this fails to compile).

- [ ] **Step 3: Implement**

Replace the existing `delete_template` body in `templates.rs`:

```rust
    /// Soft-delete: hidden immediately, recoverable via `restore_template`
    /// or the Recently Deleted view until the purge sweep hard-deletes it.
    pub fn delete_template(&self, id: i64) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE templates SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
                params![now_iso(), id],
            )?;
            Ok(())
        })
    }

    pub fn restore_template(&self, id: i64) -> Result<Template> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE templates SET deleted_at = NULL WHERE id = ?1",
                params![id],
            )?;
            get_template_tx(conn, id)
        })
    }

    pub fn list_recently_deleted_templates(&self) -> Result<Vec<Template>> {
        self.with_conn(|conn| {
            let sql = format!(
                "SELECT {TEMPLATE_COLUMNS} FROM templates
                 WHERE deleted_at IS NOT NULL ORDER BY deleted_at DESC"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], template_from_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }
```

Add `AND deleted_at IS NULL` to `list_templates`'s existing `WHERE` clause (or add one if it has none yet — check the query at line 59-65 of the version read earlier).

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p magpie-core`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/magpie-core/src/templates.rs
git commit -m "Make template delete soft-delete-based, add restore"
```

---

### Task 6: Soft-delete/restore for sections, unassigning members

**Files:**
- Modify: `crates/magpie-core/src/sections.rs`

**Interfaces:**
- Produces: `Store::delete_section(&self, id: i64) -> Result<()>`
- Produces: `Store::restore_section(&self, id: i64) -> Result<Section>`
- Produces: `Store::list_recently_deleted_sections(&self) -> Result<Vec<Section>>`

- [ ] **Step 1: Write failing tests**

Add to `sections.rs`'s test module:

```rust
    #[test]
    fn delete_section_unassigns_members_without_deleting_them() {
        let store = crate::Store::open_in_memory().unwrap();
        let s = store.create_section("Research").unwrap();
        let c = store.capture("note", None).unwrap();
        store.assign_capture_section(c.id, Some(s.id)).unwrap();

        store.delete_section(s.id).unwrap();

        let c_after = store.get_capture(c.id).unwrap();
        assert_eq!(c_after.section_id, None);
        assert_eq!(c_after.deleted_at, None); // member itself isn't deleted
        assert!(store.list_sections().unwrap().is_empty());
    }

    #[test]
    fn restore_section_brings_it_back_but_does_not_reassign_former_members() {
        let store = crate::Store::open_in_memory().unwrap();
        let s = store.create_section("Research").unwrap();
        let c = store.capture("note", None).unwrap();
        store.assign_capture_section(c.id, Some(s.id)).unwrap();
        store.delete_section(s.id).unwrap();

        let restored = store.restore_section(s.id).unwrap();
        assert_eq!(restored.deleted_at, None);
        assert_eq!(store.list_sections().unwrap().len(), 1);

        let c_after = store.get_capture(c.id).unwrap();
        assert_eq!(c_after.section_id, None); // membership was discarded, not preserved
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p magpie-core delete_section\|restore_section`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

Add to `impl Store` in `sections.rs`:

```rust
    /// Unassigns members (`section_id = NULL`) but never touches their own
    /// `deleted_at` -- deleting an organizational header must never delete
    /// content.
    pub fn delete_section(&self, id: i64) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE sections SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
                params![now_iso(), id],
            )?;
            conn.execute(
                "UPDATE captures SET section_id = NULL WHERE section_id = ?1",
                params![id],
            )?;
            conn.execute(
                "UPDATE templates SET section_id = NULL WHERE section_id = ?1",
                params![id],
            )?;
            Ok(())
        })
    }

    pub fn restore_section(&self, id: i64) -> Result<Section> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE sections SET deleted_at = NULL WHERE id = ?1",
                params![id],
            )?;
            get_section_tx(conn, id)
        })
    }

    pub fn list_recently_deleted_sections(&self) -> Result<Vec<Section>> {
        self.with_conn(|conn| {
            let sql = format!(
                "SELECT {SECTION_COLUMNS} FROM sections
                 WHERE deleted_at IS NOT NULL ORDER BY deleted_at DESC"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], section_from_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p magpie-core`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/magpie-core/src/sections.rs
git commit -m "Add soft-delete/restore for sections, unassigning members"
```

---

### Task 7: Purge sweep (core)

**Files:**
- Modify: `crates/magpie-core/src/db.rs` (cutoff helper)
- Create: purge logic in `crates/magpie-core/src/captures.rs`, `templates.rs`, `sections.rs` (one function each) plus a combined entry point

**Interfaces:**
- Produces: `pub fn purge_cutoff(days: i64) -> String` in `db.rs`
- Produces: `Store::purge_expired_captures(&self, cutoff: &str) -> Result<usize>`
- Produces: `Store::purge_expired_templates(&self, cutoff: &str) -> Result<usize>`
- Produces: `Store::purge_expired_sections(&self, cutoff: &str) -> Result<usize>`
- Produces: `Store::purge_expired(&self, days: i64) -> Result<(usize, usize, usize)>` (captures, templates, sections purged)

- [ ] **Step 1: Write failing tests**

In `crates/magpie-core/src/captures.rs`'s test module:

```rust
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
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p magpie-core purge_expired`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

Add to `crates/magpie-core/src/db.rs` (near `now_iso`):

```rust
/// The RFC3339 cutoff for "older than `days` days ago" -- rows with
/// `deleted_at` before this are eligible for the purge sweep's hard delete.
pub fn purge_cutoff(days: i64) -> String {
    (OffsetDateTime::now_utc() - time::Duration::days(days))
        .format(&Rfc3339)
        .expect("RFC3339 formatting of a valid OffsetDateTime cannot fail")
}
```

Add to `impl Store` in `captures.rs` (relying on the existing blob/tags/FTS cascade already exercised by `deleting_a_screenshot_capture_removes_its_blob_and_search_entry`):

```rust
    pub fn purge_expired_captures(&self, cutoff: &str) -> Result<usize> {
        self.with_conn(|conn| {
            Ok(conn.execute(
                "DELETE FROM captures WHERE deleted_at IS NOT NULL AND deleted_at < ?1",
                params![cutoff],
            )?)
        })
    }
```

Add to `impl Store` in `templates.rs`:

```rust
    pub fn purge_expired_templates(&self, cutoff: &str) -> Result<usize> {
        self.with_conn(|conn| {
            Ok(conn.execute(
                "DELETE FROM templates WHERE deleted_at IS NOT NULL AND deleted_at < ?1",
                params![cutoff],
            )?)
        })
    }
```

Add to `impl Store` in `sections.rs`:

```rust
    pub fn purge_expired_sections(&self, cutoff: &str) -> Result<usize> {
        self.with_conn(|conn| {
            Ok(conn.execute(
                "DELETE FROM sections WHERE deleted_at IS NOT NULL AND deleted_at < ?1",
                params![cutoff],
            )?)
        })
    }

    /// The single entry point the desktop app's purge sweep calls -- see
    /// docs/superpowers/specs/2026-07-31-capture-list-v2-design.md's
    /// "Deletion" section for why this runs both at startup and on a
    /// recurring timer, unlike the dead-pid lease sweep.
    pub fn purge_expired(&self, days: i64) -> Result<(usize, usize, usize)> {
        let cutoff = crate::db::purge_cutoff(days);
        Ok((
            self.purge_expired_captures(&cutoff)?,
            self.purge_expired_templates(&cutoff)?,
            self.purge_expired_sections(&cutoff)?,
        ))
    }
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p magpie-core`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/magpie-core/src/db.rs crates/magpie-core/src/captures.rs \
        crates/magpie-core/src/templates.rs crates/magpie-core/src/sections.rs
git commit -m "Add purge sweep for expired soft-deleted rows"
```

---

### Task 8: Tauri commands for deletion and sections

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs` (register in `invoke_handler`)
- Modify: `apps/desktop/src/lib/api.ts` (JS wrappers)
- Modify: `apps/desktop/src/lib/types.ts` (add `Section` type)

**Interfaces:**
- Consumes: all `Store` methods from Tasks 2-7
- Produces: Tauri commands `delete_capture`, `restore_capture`, `list_recently_deleted_captures`, `delete_template` (already registered, unchanged signature), `restore_template`, `list_recently_deleted_templates`, `create_section`, `rename_section`, `list_sections`, `reorder_section`, `delete_section`, `restore_section`, `assign_capture_section`, `assign_template_section`
- Produces: `api.ts` functions of the same names (camelCase), `Section` TS type

- [ ] **Step 1: Add Tauri commands**

In `apps/desktop/src-tauri/src/commands.rs`, add:

```rust
#[tauri::command]
pub fn delete_capture(state: State<AppState>, id: i64) -> CmdResult<Capture> {
    map_err(state.store.soft_delete_capture(id))
}

#[tauri::command]
pub fn restore_capture(state: State<AppState>, id: i64) -> CmdResult<Capture> {
    map_err(state.store.restore_capture(id))
}

#[tauri::command]
pub fn list_recently_deleted_captures(state: State<AppState>) -> CmdResult<Vec<Capture>> {
    map_err(state.store.list_recently_deleted_captures())
}

#[tauri::command]
pub fn restore_template(state: State<AppState>, id: i64) -> CmdResult<Template> {
    map_err(state.store.restore_template(id))
}

#[tauri::command]
pub fn list_recently_deleted_templates(state: State<AppState>) -> CmdResult<Vec<Template>> {
    map_err(state.store.list_recently_deleted_templates())
}

#[tauri::command]
pub fn create_section(state: State<AppState>, name: String) -> CmdResult<Section> {
    map_err(state.store.create_section(&name))
}

#[tauri::command]
pub fn rename_section(state: State<AppState>, id: i64, name: String) -> CmdResult<Section> {
    map_err(state.store.rename_section(id, &name))
}

#[tauri::command]
pub fn list_sections(state: State<AppState>) -> CmdResult<Vec<Section>> {
    map_err(state.store.list_sections())
}

#[tauri::command]
pub fn reorder_section(
    state: State<AppState>,
    id: i64,
    after_id: Option<i64>,
) -> CmdResult<Section> {
    map_err(state.store.reorder_section(id, after_id))
}

#[tauri::command]
pub fn delete_section(state: State<AppState>, id: i64) -> CmdResult<()> {
    map_err(state.store.delete_section(id))
}

#[tauri::command]
pub fn restore_section(state: State<AppState>, id: i64) -> CmdResult<Section> {
    map_err(state.store.restore_section(id))
}

#[tauri::command]
pub fn assign_capture_section(
    state: State<AppState>,
    id: i64,
    section_id: Option<i64>,
) -> CmdResult<Capture> {
    map_err(state.store.assign_capture_section(id, section_id))
}

#[tauri::command]
pub fn assign_template_section(
    state: State<AppState>,
    id: i64,
    section_id: Option<i64>,
) -> CmdResult<Template> {
    map_err(state.store.assign_template_section(id, section_id))
}
```

Add `Section` to the `use magpie_core::{...}` import line at the top of `commands.rs`.

- [ ] **Step 2: Register in `invoke_handler`**

In `apps/desktop/src-tauri/src/lib.rs`, add to the `tauri::generate_handler![...]` list (after `commands::delete_template,`):

```rust
            commands::delete_capture,
            commands::restore_capture,
            commands::list_recently_deleted_captures,
            commands::restore_template,
            commands::list_recently_deleted_templates,
            commands::create_section,
            commands::rename_section,
            commands::list_sections,
            commands::reorder_section,
            commands::delete_section,
            commands::restore_section,
            commands::assign_capture_section,
            commands::assign_template_section,
```

- [ ] **Step 3: Add JS wrappers and the `Section` type**

In `apps/desktop/src/lib/types.ts`, add:

```typescript
export interface Section {
  id: number;
  name: string;
  position: number;
  created_at: string;
  deleted_at: string | null;
}
```

In `apps/desktop/src/lib/api.ts`, add (matching the existing `invoke<T>("command_name", { args })` style already used for e.g. `deleteTemplate`):

```typescript
  deleteCapture: (id: number) => invoke<Capture>("delete_capture", { id }),
  restoreCapture: (id: number) => invoke<Capture>("restore_capture", { id }),
  listRecentlyDeletedCaptures: () =>
    invoke<Capture[]>("list_recently_deleted_captures"),
  restoreTemplate: (id: number) => invoke<Template>("restore_template", { id }),
  listRecentlyDeletedTemplates: () =>
    invoke<Template[]>("list_recently_deleted_templates"),
  createSection: (name: string) => invoke<Section>("create_section", { name }),
  renameSection: (id: number, name: string) =>
    invoke<Section>("rename_section", { id, name }),
  listSections: () => invoke<Section[]>("list_sections"),
  reorderSection: (id: number, afterId: number | null) =>
    invoke<Section>("reorder_section", { id, afterId }),
  deleteSection: (id: number) => invoke<void>("delete_section", { id }),
  restoreSection: (id: number) => invoke<Section>("restore_section", { id }),
  assignCaptureSection: (id: number, sectionId: number | null) =>
    invoke<Capture>("assign_capture_section", { id, sectionId }),
  assignTemplateSection: (id: number, sectionId: number | null) =>
    invoke<Template>("assign_template_section", { id, sectionId }),
```

(Add `Section` to `api.ts`'s type imports from `./types`.)

- [ ] **Step 4: Verify it builds**

Run: `cd apps/desktop/src-tauri && cargo build` — Expected: builds cleanly.
Run: `cd apps/desktop && pnpm build` — Expected: `tsc` reports no type errors.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/commands.rs apps/desktop/src-tauri/src/lib.rs \
        apps/desktop/src/lib/api.ts apps/desktop/src/lib/types.ts
git commit -m "Expose deletion and section commands to the frontend"
```

---

### Task 9: Purge sweep wiring (startup + recurring)

**Files:**
- Create: `apps/desktop/src-tauri/src/purge_sweep.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `Store::purge_expired` (Task 7)
- Produces: `pub fn sweep(store: &Store)` in `purge_sweep.rs`

- [ ] **Step 1: Implement**

Create `apps/desktop/src-tauri/src/purge_sweep.rs`:

```rust
// Hard-deletes soft-deleted captures/templates/sections older than 30 days.
// Unlike dead_pid_sweep (startup-only, correct because leases don't expire
// so there's nothing to catch up on except a previous crash), this needs to
// run on a recurring timer too: a 30-day retention window depends on real
// wall-clock time passing, and this is a tray app that can plausibly stay
// open for weeks without a restart -- startup-only would mean purge never
// fires for a long-running session. See
// docs/superpowers/specs/2026-07-31-capture-list-v2-design.md "Deletion".

use magpie_core::Store;

const RETENTION_DAYS: i64 = 30;

pub fn sweep(store: &Store) {
    match store.purge_expired(RETENTION_DAYS) {
        Ok((captures, templates, sections)) => {
            if captures + templates + sections > 0 {
                eprintln!(
                    "magpie: purged {captures} capture(s), {templates} template(s), \
                     {sections} section(s) past the {RETENTION_DAYS}-day retention window"
                );
            }
        }
        Err(e) => eprintln!("magpie: purge sweep failed: {e}"),
    }
}
```

In `apps/desktop/src-tauri/src/lib.rs`, add `mod purge_sweep;` alongside the other `mod` declarations, and in `setup()`, after `dead_pid_sweep::sweep(&store);` and before `app.manage(...)`, capture what's needed, then after `app.manage(...)` spawn the recurring sweep:

```rust
            purge_sweep::sweep(&store);
```

(right next to the existing `dead_pid_sweep::sweep(&store);` line), and after `app.manage(state::AppState::new(store, backend));`, add:

```rust
            // Recurring purge: only touches the SQLite Store, never a
            // window/panel, so a plain background thread is safe here
            // (the AppKit main-thread rule from toast.rs's history only
            // applies to code that touches a Tauri window/panel).
            let app_handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(60 * 60 * 24));
                let state = app_handle.state::<state::AppState>();
                purge_sweep::sweep(&state.store);
            });
```

- [ ] **Step 2: Verify it builds and runs**

Run: `cd apps/desktop && pnpm tauri dev`
Expected: app launches with no panic; console shows no purge errors (nothing to purge yet, since Phase 2's delete UI isn't wired up until Task 10-11).

- [ ] **Step 3: Commit**

```bash
git add apps/desktop/src-tauri/src/purge_sweep.rs apps/desktop/src-tauri/src/lib.rs
git commit -m "Wire purge sweep at startup and on a 24h recurring timer"
```

---

### Task 10: Undo toast component, wired to template delete

**Files:**
- Create: `apps/desktop/src/components/UndoToast.tsx`
- Modify: `apps/desktop/src/components/TemplatesPanel.tsx`
- Modify: `apps/desktop/src/App.tsx` (host the toast, since it needs to float above everything)

**Interfaces:**
- Produces: `<UndoToast message={string} onUndo={() => void} onDismiss={() => void} />`
- Consumes: `api.deleteTemplate`, `api.restoreTemplate` (Task 8)

- [ ] **Step 1: Implement `UndoToast`**

Create `apps/desktop/src/components/UndoToast.tsx`. This is an ordinary in-window React component -- unrelated to the ambient capture-confirmation toast in `src-tauri/src/toast.rs`, which is a deliberately non-activating, click-through `NSPanel` that cannot host a clickable button. Delete happens with the main window already focused, so Undo needs to actually be clickable:

```tsx
import { useEffect } from "react";

interface UndoToastProps {
  message: string;
  onUndo: () => void;
  onDismiss: () => void;
  /** How long before this toast auto-dismisses if Undo isn't clicked. */
  durationMs?: number;
}

export function UndoToast({
  message,
  onUndo,
  onDismiss,
  durationMs = 6000,
}: UndoToastProps) {
  useEffect(() => {
    const handle = setTimeout(onDismiss, durationMs);
    return () => clearTimeout(handle);
  }, [onDismiss, durationMs]);

  return (
    <div
      className="fixed bottom-4 left-1/2 z-50 flex -translate-x-1/2 items-center gap-3
                 rounded-lg bg-neutral-900 px-4 py-2.5 text-sm text-white shadow-lg
                 dark:bg-neutral-100 dark:text-neutral-900"
    >
      <span>{message}</span>
      <button
        type="button"
        onClick={onUndo}
        className="font-medium text-slate-teal-light underline hover:opacity-80 dark:text-slate-teal"
      >
        Undo
      </button>
    </div>
  );
}
```

- [ ] **Step 2: Add toast state to `App.tsx` and render it**

In `App.tsx`, add near the other `useState` calls:

```tsx
  const [undoToast, setUndoToast] = useState<{
    message: string;
    onUndo: () => void;
  } | null>(null);
```

Render it just before the closing `</main>`:

```tsx
      {undoToast && (
        <UndoToast
          message={undoToast.message}
          onUndo={() => {
            undoToast.onUndo();
            setUndoToast(null);
          }}
          onDismiss={() => setUndoToast(null)}
        />
      )}
```

Add the import: `import { UndoToast } from "./components/UndoToast";`

Expose a small setter prop or a context so `TemplatesPanel` can trigger it — simplest: lift a `showUndoToast` callback down as a prop, matching how `onInstantiated` is already passed to `TemplatesPanel` today:

```tsx
          {view === "templates" && (
            <TemplatesPanel
              onInstantiated={() => {
                refreshNow();
                emit(NOW_CHANGED_EVENT);
              }}
              onShowUndo={(message, onUndo) => setUndoToast({ message, onUndo })}
            />
          )}
```

- [ ] **Step 3: Rewire `TemplatesPanel`'s delete button**

In `TemplatesPanel.tsx`, change the `remove(id)` function (currently a direct hard `api.deleteTemplate(id)` call) to soft-delete + show the undo toast:

```tsx
  async function remove(id: number) {
    await api.deleteTemplate(id); // now soft-delete (Task 5) -- same call, new semantics
    setTemplates((prev) => prev.filter((t) => t.id !== id));
    onShowUndo("Template deleted.", async () => {
      await api.restoreTemplate(id);
      refresh(); // whatever the existing template-list refetch function is called
    });
  }
```

Add `onShowUndo: (message: string, onUndo: () => void) => void` to `TemplatesPanelProps`.

- [ ] **Step 4: Manually verify**

Run: `pnpm tauri dev` from `apps/desktop`. Create a template, delete it — confirm it disappears immediately and an Undo toast appears at the bottom of the window. Click Undo — confirm the template reappears. Delete again and let the toast time out (6s) without clicking — confirm the template stays gone.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/components/UndoToast.tsx apps/desktop/src/components/TemplatesPanel.tsx apps/desktop/src/App.tsx
git commit -m "Add Undo toast, wire template delete to soft-delete"
```

---

### Task 11: Recently Deleted view

**Files:**
- Create: `apps/desktop/src/components/RecentlyDeletedView.tsx`
- Modify: `apps/desktop/src/App.tsx` (new tab)

**Interfaces:**
- Consumes: `api.listRecentlyDeletedCaptures`, `api.listRecentlyDeletedTemplates`, `api.restoreCapture`, `api.restoreTemplate` (Task 8)

- [ ] **Step 1: Implement the view**

Create `apps/desktop/src/components/RecentlyDeletedView.tsx`:

```tsx
import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import type { Capture, Template } from "@/lib/types";

export function RecentlyDeletedView() {
  const [captures, setCaptures] = useState<Capture[]>([]);
  const [templates, setTemplates] = useState<Template[]>([]);

  function refresh() {
    api.listRecentlyDeletedCaptures().then(setCaptures).catch(console.error);
    api.listRecentlyDeletedTemplates().then(setTemplates).catch(console.error);
  }

  useEffect(refresh, []);

  async function restoreCapture(id: number) {
    await api.restoreCapture(id);
    refresh();
  }

  async function restoreTemplate(id: number) {
    await api.restoreTemplate(id);
    refresh();
  }

  if (captures.length === 0 && templates.length === 0) {
    return (
      <p className="px-3 py-6 text-center text-sm text-neutral-400 dark:text-neutral-600">
        Nothing recently deleted.
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-4 overflow-y-auto p-3">
      {captures.length > 0 && (
        <div>
          <h3 className="mb-2 text-xs font-semibold uppercase tracking-wide text-neutral-400">
            Captures
          </h3>
          <div className="flex flex-col gap-2">
            {captures.map((c) => (
              <div
                key={c.id}
                className="flex items-center justify-between rounded-lg border
                           border-neutral-200 bg-white px-3 py-2 text-sm dark:border-neutral-800
                           dark:bg-neutral-900"
              >
                <span className="truncate text-neutral-700 dark:text-neutral-300">
                  {c.body || "(screenshot)"}
                </span>
                <button
                  type="button"
                  onClick={() => restoreCapture(c.id)}
                  className="shrink-0 text-slate-teal hover:underline dark:text-slate-teal-light"
                >
                  Restore
                </button>
              </div>
            ))}
          </div>
        </div>
      )}
      {templates.length > 0 && (
        <div>
          <h3 className="mb-2 text-xs font-semibold uppercase tracking-wide text-neutral-400">
            Templates
          </h3>
          <div className="flex flex-col gap-2">
            {templates.map((t) => (
              <div
                key={t.id}
                className="flex items-center justify-between rounded-lg border
                           border-neutral-200 bg-white px-3 py-2 text-sm dark:border-neutral-800
                           dark:bg-neutral-900"
              >
                <span className="truncate text-neutral-700 dark:text-neutral-300">
                  {t.title}
                </span>
                <button
                  type="button"
                  onClick={() => restoreTemplate(t.id)}
                  className="shrink-0 text-slate-teal hover:underline dark:text-slate-teal-light"
                >
                  Restore
                </button>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
```

(`Delete Permanently` is intentionally left out of this first pass through the view — every row already has a working 30-day purge sweep from Task 9, and adding a permanent-delete button here is a small, obvious follow-up once the rest of the batch has shipped, not a blocker to this task's own deliverable.)

- [ ] **Step 2: Add the tab**

In `App.tsx`, extend the `View` type and `VIEWS` array:

```tsx
type View = "captures" | "templates" | "activity" | "recently_deleted";
const VIEWS: { id: View; label: string }[] = [
  { id: "captures", label: "Captures" },
  { id: "templates", label: "Templates" },
  { id: "activity", label: "Activity" },
  { id: "recently_deleted", label: "Recently Deleted" },
];
```

Add the render branch alongside the existing `{view === "activity" && <AuditView />}`:

```tsx
          {view === "recently_deleted" && <RecentlyDeletedView />}
```

Add the import: `import { RecentlyDeletedView } from "./components/RecentlyDeletedView";`

- [ ] **Step 3: Manually verify**

Run: `pnpm tauri dev`. Delete a template, switch to the "Recently Deleted" tab — confirm it's listed with a working Restore button, and that restoring it makes it reappear in Templates and disappear from Recently Deleted.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src/components/RecentlyDeletedView.tsx apps/desktop/src/App.tsx
git commit -m "Add Recently Deleted view"
```

---

## Phase 3 — Markdown rendering

### Task 12: `MarkdownBody` component

**Files:**
- Modify: `apps/desktop/package.json` (add `react-markdown`)
- Create: `apps/desktop/src/components/MarkdownBody.tsx`

**Interfaces:**
- Produces: `<MarkdownBody text={string} className?={string} />`

- [ ] **Step 1: Add the dependency**

```bash
cd apps/desktop && pnpm add react-markdown
```

- [ ] **Step 2: Implement**

Create `apps/desktop/src/components/MarkdownBody.tsx`:

```tsx
import ReactMarkdown from "react-markdown";
import { cn } from "@/lib/utils";

interface MarkdownBodyProps {
  text: string;
  className?: string;
}

/**
 * Renders capture/template bodies as Markdown. Deliberately does NOT use
 * the rehype-raw plugin: captured content routinely originates from
 * arbitrary clipboard content off untrusted web pages, and enabling raw
 * HTML passthrough would be stored XSS in this Tauri webview the moment
 * someone captures a snippet containing a crafted `<img onerror=...>`.
 * react-markdown is safe-by-default without that plugin -- embedded HTML
 * renders as inert text, not executed.
 */
export function MarkdownBody({ text, className }: MarkdownBodyProps) {
  return (
    <div
      className={cn(
        "prose prose-sm max-w-none whitespace-pre-wrap break-words leading-snug",
        "prose-p:my-1 prose-headings:my-1 prose-ul:my-1 prose-ol:my-1",
        "dark:prose-invert",
        className,
      )}
    >
      <ReactMarkdown>{text}</ReactMarkdown>
    </div>
  );
}
```

(If the `@tailwindcss/typography` plugin isn't already configured — check `apps/desktop` Tailwind config — either add it, or drop the `prose` classes and rely on the explicit `prose-p:my-1` etc. resets alone; confirm which by checking the project's `tailwind.config`/`vite.config.ts` for a `typography` plugin registration before assuming it's present.)

- [ ] **Step 3: Manually verify the security requirement**

Run: `pnpm tauri dev`. Capture (or type via `AddPromptInput`) a body containing `<img src=x onerror="alert(1)">` and view it in the stream once Task 13 wires this component in. Confirm no alert fires and the tag renders as inert visible text, not executed markup.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/package.json apps/desktop/pnpm-lock.yaml apps/desktop/src/components/MarkdownBody.tsx
git commit -m "Add MarkdownBody component (react-markdown, no rehype-raw)"
```

---

### Task 13: Wire `MarkdownBody` into `CaptureItem` and `TemplatesPanel`

**Files:**
- Modify: `apps/desktop/src/components/CaptureItem.tsx`
- Modify: `apps/desktop/src/components/TemplatesPanel.tsx`

**Interfaces:**
- Consumes: `MarkdownBody` (Task 12)

- [ ] **Step 1: Replace raw text rendering in `CaptureItem`**

In `CaptureItem.tsx`, replace both body-rendering branches (`{blob.ocr_text ?? "Reading text…"}` and the plain `<p>{capture.body}</p>`) with `MarkdownBody`:

```tsx
            <MarkdownBody text={blob.ocr_text ?? "Reading text…"} className="text-xs text-neutral-500 dark:text-neutral-400" />
```

and

```tsx
          <MarkdownBody text={capture.body} className="text-sm text-neutral-800 dark:text-neutral-200" />
```

Add the import: `import { MarkdownBody } from "./MarkdownBody";`

- [ ] **Step 2: Replace raw text rendering in `TemplatesPanel`**

Find wherever `TemplatesPanel.tsx` renders a template's `body` preview as plain text and swap in `<MarkdownBody text={t.body} />` the same way.

- [ ] **Step 3: Manually verify no regression**

Run: `pnpm tauri dev`. Confirm existing plain-text captures (no markdown syntax) render identically to before — no visual change for content that predates this feature. Capture something with `**bold**` or a `- list` and confirm it renders formatted.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src/components/CaptureItem.tsx apps/desktop/src/components/TemplatesPanel.tsx
git commit -m "Render capture and template bodies as Markdown"
```

---

## Phase 4 — Interaction model overhaul

### Task 14: Clipboard plugin + image copy for screenshots

**Files:**
- Modify: `apps/desktop/src-tauri/Cargo.toml` (add `tauri-plugin-clipboard-manager`)
- Modify: `apps/desktop/src-tauri/src/lib.rs` (register plugin)
- Modify: `apps/desktop/src-tauri/src/commands.rs` (new command)

**Interfaces:**
- Produces: Tauri command `copy_capture_image(state, capture_id: i64) -> CmdResult<()>`

- [ ] **Step 1: Add the dependency and register the plugin**

In `apps/desktop/src-tauri/Cargo.toml`, add under `[dependencies]`:

```toml
tauri-plugin-clipboard-manager = "2"
```

In `apps/desktop/src-tauri/src/lib.rs`, add `.plugin(tauri_plugin_clipboard_manager::init())` alongside the other `.plugin(...)` calls in `run()`.

- [ ] **Step 2: Implement the command**

In `commands.rs`, add (reads the screenshot's PNG bytes directly off disk via its existing `blobs.path`, matching how `get_blob_image_data_url` already locates the file, then writes real image bytes to the OS clipboard rather than any text derivative):

```rust
#[tauri::command]
pub fn copy_capture_image(state: State<AppState>, capture_id: i64) -> CmdResult<()> {
    let blob = map_err(state.store.get_blob_for_capture(capture_id))?
        .ok_or_else(|| format!("capture {capture_id} has no image blob"))?;
    let bytes = std::fs::read(&blob.path).map_err(|e| e.to_string())?;
    let image =
        tauri_plugin_clipboard_manager::models::Image::from_bytes(&bytes).map_err(|e| e.to_string())?;
    state
        .clipboard
        .write_image(&image)
        .map_err(|e| e.to_string())
}
```

(Check the exact `tauri-plugin-clipboard-manager` v2 Rust API surface once the dependency is added — `cargo doc -p tauri-plugin-clipboard-manager --open` or its docs.rs page — for the precise `Image` constructor and clipboard-handle-access pattern, since plugin APIs occasionally rename between minor versions; adjust the exact calls above to match what's actually exposed while preserving this task's intent: read the blob's PNG bytes from disk, write them to the OS clipboard as an image, not as text.)

- [ ] **Step 3: Register and verify**

Register `commands::copy_capture_image` in `lib.rs`'s `invoke_handler`. Add `api.copyCaptureImage: (captureId: number) => invoke<void>("copy_capture_image", { captureId })` to `api.ts`.

Run: `pnpm tauri dev`. Take a screenshot capture, trigger this command (temporarily via a throwaway button or the browser dev console's `window.__TAURI__.core.invoke`, since the real context-menu trigger lands in Task 18), then paste into e.g. Preview or Mail — confirm a real image pastes, not text.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/Cargo.lock \
        apps/desktop/src-tauri/src/lib.rs apps/desktop/src-tauri/src/commands.rs apps/desktop/src/lib/api.ts
git commit -m "Add clipboard-manager plugin, copy screenshot captures as real images"
```

---

### Task 15: Copy (text) and Copy as List (checklist)

**Files:**
- Modify: `crates/magpie-core/src/captures.rs` (`capture_display_text` helper)
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src/lib/api.ts`

**Interfaces:**
- Produces: `Store::capture_display_text(&self, id: i64) -> Result<String>` — returns `body`, or `blob.ocr_text` when `body` is empty, or `"[screenshot — OCR pending]"` when neither is available
- Produces: Tauri commands `copy_capture_text(state, id: i64) -> CmdResult<()>`, `copy_captures_as_checklist(state, ids: Vec<i64>) -> CmdResult<()>`

- [ ] **Step 1: Write failing tests**

In `captures.rs`'s test module:

```rust
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
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p magpie-core capture_display_text`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

Add to `impl Store` in `captures.rs`:

```rust
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
```

Add Tauri commands to `commands.rs`:

```rust
#[tauri::command]
pub fn copy_capture_text(state: State<AppState>, id: i64) -> CmdResult<()> {
    let text = map_err(state.store.capture_display_text(id))?;
    state.clipboard.write_text(text).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn copy_captures_as_checklist(state: State<AppState>, ids: Vec<i64>) -> CmdResult<()> {
    let mut out = String::new();
    for id in ids {
        let text = map_err(state.store.capture_display_text(id))?;
        out.push_str(&format!("- [ ] {text}\n"));
    }
    state.clipboard.write_text(out).map_err(|e| e.to_string())
}
```

(`state.clipboard` needs to exist on `AppState` — add a `clipboard: tauri_plugin_clipboard_manager::Clipboard` field if the plugin's Rust API requires holding a handle rather than accessing it statically; check the plugin's actual access pattern from Task 14 and reconcile — some Tauri v2 plugins expose their functionality via an `AppHandle` extension trait instead of managed state, in which case use that pattern consistently across `copy_capture_image`, `copy_capture_text`, and `copy_captures_as_checklist` instead of a `state.clipboard` field.)

- [ ] **Step 4: Run to verify pass, register commands**

Run: `cargo test -p magpie-core` — Expected: PASS.
Register both new commands in `lib.rs`'s `invoke_handler`, add `copyCaptureText`/`copyCapturesAsChecklist` to `api.ts`.

- [ ] **Step 5: Commit**

```bash
git add crates/magpie-core/src/captures.rs apps/desktop/src-tauri/src/commands.rs \
        apps/desktop/src-tauri/src/lib.rs apps/desktop/src/lib/api.ts
git commit -m "Add Copy (text) and Copy as List (Markdown checklist)"
```

---

### Task 16: Inline Edit

**Files:**
- Modify: `crates/magpie-core/src/captures.rs` (`update_capture_body`)
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Create: `apps/desktop/src/components/EditableBody.tsx`

**Interfaces:**
- Produces: `Store::update_capture_body(&self, id: i64, body: &str) -> Result<Capture>` (rejects `session_digest` rows)
- Produces: Tauri command `update_capture_body(state, id, body) -> CmdResult<Capture>`
- Produces: `<EditableBody capture={Capture} onSave={(body: string) => Promise<void>} />`

- [ ] **Step 1: Write failing tests**

In `captures.rs`'s test module:

```rust
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
        // Mirrors the existing promote_rejects_a_session_digest precedent --
        // digests are system-generated summaries, not user-authored content.
        let store = Store::open_in_memory().unwrap();
        let digest_id = store.end_session("some-session-id").unwrap(); // adjust to however session_digest rows are actually created in end_session's real signature
        let err = store.update_capture_body(digest_id, "edited").unwrap_err();
        assert!(matches!(err, Error::CannotEditDigest(_)));
    }
```

(Check `sessions.rs`'s real `end_session` signature/return type before finalizing this second test — it may return `()` or a `Session`, not directly a digest capture id; adapt the test to however a `session_digest`-kind capture is actually produced in the existing test suite, e.g. search for `session_digest` in `sessions.rs`'s or `captures.rs`'s existing tests for the established pattern, mirroring `promote_rejects_a_session_digest`'s own setup exactly.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p magpie-core update_capture_body`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

Add the new error variant to `error.rs`:

```rust
    #[error("capture {0} is a session digest and cannot be edited")]
    CannotEditDigest(i64),
```

Add to `impl Store` in `captures.rs` (follow whatever pattern `promote_rejects_a_session_digest`'s corresponding `promote` guard already uses for checking `kind == "session_digest"`):

```rust
    pub fn update_capture_body(&self, id: i64, body: &str) -> Result<Capture> {
        self.with_conn(|conn| {
            let capture = {
                let sql = format!("SELECT {CAPTURE_COLUMNS} FROM captures WHERE id = ?1");
                conn.query_row(&sql, params![id], capture_from_row)
                    .optional()?
                    .ok_or(Error::CaptureNotFound(id))?
            };
            if capture.is_session_digest() {
                return Err(Error::CannotEditDigest(id));
            }
            conn.execute(
                "UPDATE captures SET body = ?1 WHERE id = ?2 AND deleted_at IS NULL",
                params![body, id],
            )?;
            let sql = format!("SELECT {CAPTURE_COLUMNS} FROM captures WHERE id = ?1");
            conn.query_row(&sql, params![id], capture_from_row)
                .optional()?
                .ok_or(Error::CaptureNotFound(id))
        })
    }
```

Add the Tauri command to `commands.rs`:

```rust
#[tauri::command]
pub fn update_capture_body(state: State<AppState>, id: i64, body: String) -> CmdResult<Capture> {
    map_err(state.store.update_capture_body(id, &body))
}
```

- [ ] **Step 4: Implement `EditableBody`**

Create `apps/desktop/src/components/EditableBody.tsx`:

```tsx
import { useState } from "react";
import type { Capture } from "@/lib/types";
import { MarkdownBody } from "./MarkdownBody";

interface EditableBodyProps {
  capture: Capture;
  editing: boolean;
  onSave: (body: string) => Promise<void>;
  onCancel: () => void;
}

export function EditableBody({ capture, editing, onSave, onCancel }: EditableBodyProps) {
  const [draft, setDraft] = useState(capture.body);

  if (!editing) {
    return <MarkdownBody text={capture.body} className="text-sm text-neutral-800 dark:text-neutral-200" />;
  }

  return (
    <div className="flex flex-col gap-2">
      <textarea
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        autoFocus
        rows={4}
        className="w-full rounded-md border border-neutral-300 bg-white p-2 text-sm
                   dark:border-neutral-700 dark:bg-neutral-800"
      />
      <div className="flex gap-2">
        <button
          type="button"
          onClick={() => onSave(draft)}
          className="rounded-md bg-slate-teal px-2.5 py-1 text-xs text-white hover:opacity-90"
        >
          Save
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="rounded-md px-2.5 py-1 text-xs text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-800"
        >
          Cancel
        </button>
      </div>
    </div>
  );
}
```

Register the command in `lib.rs`'s `invoke_handler`, add `updateCaptureBody` to `api.ts`.

- [ ] **Step 5: Run to verify pass**

Run: `cargo test -p magpie-core` — Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/magpie-core/src/captures.rs crates/magpie-core/src/error.rs \
        apps/desktop/src-tauri/src/commands.rs apps/desktop/src-tauri/src/lib.rs \
        apps/desktop/src/lib/api.ts apps/desktop/src/components/EditableBody.tsx
git commit -m "Add inline Edit for captures (session digests excluded)"
```

---

### Task 17: Move to Project / Move to Section commands

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands.rs` (already has `assign_capture_project`; add template-side project assignment if it doesn't exist, and confirm `assign_capture_section`/`assign_template_section` from Task 8 are sufficient)

**Interfaces:**
- Consumes: `Store::assign_capture_section`/`assign_template_section` (Task 3), `Store::assign_project` (existing)

- [ ] **Step 1: Confirm existing coverage**

`assign_capture_project` already exists in `commands.rs`. Check whether templates need an equivalent "assign to project" — per the design spec, "Move to Project" only applies to captures (templates are a cross-project library, not project-scoped; the spec's data model section never gives `templates` a `project_id`). Confirm `templates` has no `project_id` column in the schema (it doesn't, per `migrations/0001_init.sql`'s `templates` definition) — if so, no new command is needed here beyond what Task 8 already added for sections. This task is a checkpoint, not new code, unless that check turns up a gap.

- [ ] **Step 2: Manually verify via the existing commands**

Run: `cargo build -p magpie-core -p desktop_lib` — Expected: builds cleanly with everything from Tasks 3 and 8 already in place.

- [ ] **Step 3: Commit**

Only if a gap was found and fixed; otherwise no commit — this task's purpose is confirming Task 3/8 already cover "Move to Project"/"Move to Section" fully before Task 20 wires them into the UI.

---

### Task 18: `ContextMenu` component replacing hover icons

**Files:**
- Create: `apps/desktop/src/components/ContextMenu.tsx`
- Modify: `apps/desktop/src/components/CaptureItem.tsx` (remove hover-icon row, add hover-reveal `•••` + right-click trigger)

**Interfaces:**
- Produces: `<ContextMenu items={ContextMenuItem[]} trigger={ReactNode} />` where `ContextMenuItem = { label: string; onClick: () => void; disabled?: boolean; submenu?: ContextMenuItem[] }`

- [ ] **Step 1: Implement `ContextMenu`**

Create `apps/desktop/src/components/ContextMenu.tsx` — a right-click-and-hover-button-triggered dropdown, built from scratch with plain positioned `<div>`s (no new dependency; check first whether `shadcn/ui`'s context-menu primitive is already vendored anywhere under `apps/desktop/src/components/ui/` from prior scaffolding — if so, use that instead of a hand-rolled version, since `docs/design.md`'s architecture table already lists `shadcn/ui` as this app's UI layer):

```tsx
import { MoreHorizontal } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";

export interface ContextMenuItem {
  label: string;
  onClick?: () => void;
  disabled?: boolean;
  submenu?: ContextMenuItem[];
  destructive?: boolean;
}

interface ContextMenuProps {
  items: ContextMenuItem[];
  /** Imperative open, so a parent's onContextMenu handler and the row's
      keyboard shortcut (Phase 5) can both trigger the same menu instance. */
  openRef?: React.MutableRefObject<((x: number, y: number) => void) | null>;
}

export function ContextMenu({ items, openRef }: ContextMenuProps) {
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (openRef) openRef.current = (x, y) => setPos({ x, y });
  }, [openRef]);

  useEffect(() => {
    if (!pos) return;
    function onClickAway(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setPos(null);
    }
    document.addEventListener("mousedown", onClickAway);
    return () => document.removeEventListener("mousedown", onClickAway);
  }, [pos]);

  if (!pos) return null;

  return (
    <div
      ref={ref}
      style={{ top: pos.y, left: pos.x }}
      className="fixed z-50 min-w-[180px] rounded-lg border border-neutral-200 bg-white
                 py-1 text-sm shadow-lg dark:border-neutral-700 dark:bg-neutral-900"
    >
      {items.map((item) => (
        <button
          key={item.label}
          type="button"
          disabled={item.disabled}
          onClick={() => {
            item.onClick?.();
            setPos(null);
          }}
          className={cn(
            "flex w-full items-center px-3 py-1.5 text-left hover:bg-neutral-100 dark:hover:bg-neutral-800",
            "disabled:cursor-not-allowed disabled:opacity-40",
            item.destructive && "text-red-600 dark:text-red-400",
          )}
        >
          {item.label}
        </button>
      ))}
    </div>
  );
}

/** Hover-reveal trigger button -- matches how the icons it replaces already
    behaved; a permanent per-row control at rest is visual noise a
    minimalist list shouldn't pay for. Right-click itself isn't gated by
    this button at all (the whole row is already the click target). */
export function ContextMenuTrigger({ onOpen }: { onOpen: (x: number, y: number) => void }) {
  return (
    <button
      type="button"
      title="More actions"
      onClick={(e) => {
        const rect = e.currentTarget.getBoundingClientRect();
        onOpen(rect.left, rect.bottom);
      }}
      className="rounded p-1.5 text-neutral-400 opacity-0 hover:bg-neutral-100
                 group-hover:opacity-100 dark:hover:bg-neutral-800"
    >
      <MoreHorizontal size={16} />
    </button>
  );
}
```

- [ ] **Step 2: Replace `CaptureItem`'s hover-icon row**

In `CaptureItem.tsx`, remove the entire `<div className="flex shrink-0 items-center gap-1 opacity-0 ...">` block (Done/Promote/Demote icon buttons) and replace it with:

```tsx
      <div
        onContextMenu={(e) => {
          e.preventDefault();
          menuOpenRef.current?.(e.clientX, e.clientY);
        }}
      >
        <ContextMenuTrigger onOpen={(x, y) => menuOpenRef.current?.(x, y)} />
        <ContextMenu items={buildContextMenuItems()} openRef={menuOpenRef} />
      </div>
```

with `const menuOpenRef = useRef<((x: number, y: number) => void) | null>(null);` declared at the top of the component, and `buildContextMenuItems()` deferred to Task 20 (this task's job is the mechanism; Task 20 fills in the actual action list). For now, stub `buildContextMenuItems` to return the pre-existing Done/Promote/Demote actions only, preserving current behavior 1:1 while the mechanism is proven out:

```tsx
  function buildContextMenuItems(): ContextMenuItem[] {
    const items: ContextMenuItem[] = [];
    if (onDone) items.push({ label: "Mark Done", onClick: () => onDone(capture.id) });
    if (onPromote) items.push({ label: "Promote to Now", onClick: () => onPromote(capture.id) });
    if (onDemote) items.push({ label: "Remove from Now", onClick: () => onDemote(capture.id) });
    return items;
  }
```

- [ ] **Step 3: Manually verify**

Run: `pnpm tauri dev`. Hover a capture row — confirm the `•••` button appears (hover-reveal, not always visible) and clicking it opens a menu with Mark Done/Promote/Remove from Now, functioning identically to the old hover-icon buttons. Right-click the same row — confirm the identical menu opens at the cursor position. Click elsewhere — confirm it closes.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src/components/ContextMenu.tsx apps/desktop/src/components/CaptureItem.tsx
git commit -m "Replace hover-icon row with a context menu (mechanism only)"
```

---

### Task 19: Section header rendering and in-place management

**Files:**
- Create: `apps/desktop/src/components/SectionHeader.tsx`
- Modify: `apps/desktop/src/App.tsx` (group `visibleStream` by section before rendering)
- Modify: `apps/desktop/src/components/NowList.tsx` (group `items` by section)

**Interfaces:**
- Produces: `<SectionHeader section={Section} onRename={(name)=>void} onDelete={()=>void} dragHandleProps={...} />`
- Consumes: `api.listSections`, `api.renameSection`, `api.reorderSection`, `api.deleteSection` (Task 8)

- [ ] **Step 1: Implement `SectionHeader`**

Create `apps/desktop/src/components/SectionHeader.tsx` (drag handle mirrors `NowList`'s existing `dnd-kit` `SortableCaptureItem` pattern, rename is an inline click-to-edit label, delete is a small trailing button — all in-place, no separate management screen, per the design spec):

```tsx
import { GripVertical, Trash2 } from "lucide-react";
import { useState } from "react";
import type { Section } from "@/lib/types";

interface SectionHeaderProps {
  section: Section;
  onRename: (name: string) => void;
  onDelete: () => void;
  dragHandleProps?: React.HTMLAttributes<HTMLButtonElement>;
}

export function SectionHeader({ section, onRename, onDelete, dragHandleProps }: SectionHeaderProps) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(section.name);

  return (
    <div className="group mt-3 flex items-center gap-1.5 first:mt-0">
      {dragHandleProps && (
        <button
          type="button"
          className="cursor-grab text-neutral-300 hover:text-neutral-500 dark:text-neutral-700"
          {...dragHandleProps}
        >
          <GripVertical size={14} />
        </button>
      )}
      {editing ? (
        <input
          autoFocus
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={() => {
            setEditing(false);
            if (draft.trim() && draft !== section.name) onRename(draft.trim());
          }}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          className="rounded border border-slate-teal bg-transparent px-1 text-xs font-semibold
                     uppercase tracking-wide text-neutral-500 outline-none dark:text-neutral-400"
        />
      ) : (
        <h3
          onClick={() => setEditing(true)}
          className="cursor-text text-xs font-semibold uppercase tracking-wide text-neutral-500 dark:text-neutral-400"
        >
          {section.name}
        </h3>
      )}
      <button
        type="button"
        onClick={onDelete}
        className="ml-auto rounded p-1 text-neutral-300 opacity-0 hover:bg-red-50 hover:text-red-500
                   group-hover:opacity-100 dark:hover:bg-red-950"
      >
        <Trash2 size={12} />
      </button>
    </div>
  );
}
```

- [ ] **Step 2: Group and render in `App.tsx`**

Add section state and a grouping helper near `visibleStream`:

```tsx
  const [sections, setSections] = useState<Section[]>([]);

  useEffect(() => {
    api.listSections().then(setSections).catch(console.error);
  }, []);

  function groupBySection<T extends { section_id: number | null }>(items: T[]) {
    const bySection = new Map<number, T[]>();
    const unsectioned: T[] = [];
    for (const item of items) {
      if (item.section_id === null) unsectioned.push(item);
      else bySection.set(item.section_id, [...(bySection.get(item.section_id) ?? []), item]);
    }
    return { bySection, unsectioned };
  }
```

Replace the plain `{visibleStream.map((capture) => <CaptureItem .../>)}` block with grouped rendering (sections in their own order above the plain list, no "Unsectioned" header at all — see design spec's "Sections — rendering & management"):

```tsx
                  <div className="flex flex-col gap-2">
                    {(() => {
                      const { bySection, unsectioned } = groupBySection(visibleStream);
                      return (
                        <>
                          {sections
                            .filter((s) => bySection.has(s.id))
                            .map((s) => (
                              <div key={s.id}>
                                <SectionHeader
                                  section={s}
                                  onRename={(name) => api.renameSection(s.id, name).then(() => api.listSections().then(setSections))}
                                  onDelete={() => api.deleteSection(s.id).then(() => { api.listSections().then(setSections); refreshStream(); })}
                                />
                                {bySection.get(s.id)!.map((capture) => (
                                  <CaptureItem key={capture.id} capture={capture} /* ...existing props... */ />
                                ))}
                              </div>
                            ))}
                          {unsectioned.map((capture) => (
                            <CaptureItem key={capture.id} capture={capture} /* ...existing props... */ />
                          ))}
                        </>
                      );
                    })()}
                  </div>
```

(Preserve every existing prop already passed to `CaptureItem` in the current `visibleStream.map` — `selected`, `onToggleSelect`, `onPromote` — this task only changes the grouping/wrapping, not the item's own props.)

- [ ] **Step 3: Apply the same grouping to `NowList`**

In `NowList.tsx`, apply the identical `groupBySection` treatment to `items`, rendering `SectionHeader` + `SortableCaptureItem`s per group, unsectioned items below with no header — items within a section keep the existing `queue_pos`-driven `dnd-kit` order (no new ordering dimension, per the design spec), so this is purely a rendering change, not a change to `handleDragEnd`'s logic.

- [ ] **Step 4: Manually verify**

Run: `pnpm tauri dev`. With no sections created yet, confirm the stream and Now list look identical to before this task (no visual change — the "if you never touch it, it's a plain list" requirement). Create a section via `api.createSection` in the dev console, assign a capture to it via `api.assignCaptureSection`, refresh — confirm the header appears above the plain list, rename it inline, drag it, delete it (confirm the capture reappears unsectioned, not deleted).

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/components/SectionHeader.tsx apps/desktop/src/App.tsx apps/desktop/src/components/NowList.tsx
git commit -m "Render section headers with in-place rename/reorder/delete"
```

---

### Task 20: Wire the full action set into the context menu

**Files:**
- Modify: `apps/desktop/src/components/CaptureItem.tsx` (`buildContextMenuItems`, stubbed in Task 18)

**Interfaces:**
- Consumes: everything from Tasks 14-17 and 19 (`copyCaptureImage`, `copyCaptureText`, `copyCapturesAsChecklist`, `updateCaptureBody`, `assignCaptureSection`, `assignCaptureProject`, `deleteCapture`, `mergeCaptures`)

- [ ] **Step 1: Replace the stubbed `buildContextMenuItems`**

Expand the Task 18 stub in `CaptureItem.tsx` into the full action set from the spec — Mark Done/Reopen, Promote/Remove from Now, Copy, Copy as List, Edit, Edit in New Window, Expand, Merge Notes, Move to Project (submenu), Move to Section (submenu), Delete:

```tsx
  function buildContextMenuItems(): ContextMenuItem[] {
    const isScreenshot = capture.body === "";
    return [
      capture.done_at
        ? { label: "Reopen", onClick: () => onReopen?.(capture.id) }
        : { label: "Mark Done", onClick: () => onDone?.(capture.id) },
      capture.queue_pos === null
        ? { label: "Promote to Now", onClick: () => onPromote?.(capture.id) }
        : { label: "Remove from Now", onClick: () => onDemote?.(capture.id) },
      {
        label: "Copy",
        onClick: () => (isScreenshot ? api.copyCaptureImage(capture.id) : api.copyCaptureText(capture.id)),
      },
      { label: "Copy as List", onClick: () => api.copyCapturesAsChecklist([capture.id]) },
      { label: "Edit", onClick: () => setEditing(true), disabled: capture.kind === "session_digest" },
      { label: "Edit in New Window", onClick: () => openEditWindow(capture.id), disabled: capture.kind === "session_digest" },
      { label: "Expand", onClick: () => setExpanded(true) },
      { label: "Merge Notes", onClick: () => onMerge?.(capture.id), disabled: !onMerge },
      {
        label: "Move to Project",
        submenu: [
          { label: "Inbox", onClick: () => api.assignCaptureProject(capture.id, null) },
          ...projects.map((p) => ({ label: p.name, onClick: () => api.assignCaptureProject(capture.id, p.id) })),
        ],
      },
      {
        label: "Move to Section",
        submenu: [
          { label: "None", onClick: () => api.assignCaptureSection(capture.id, null) },
          ...sections.map((s) => ({ label: s.name, onClick: () => api.assignCaptureSection(capture.id, s.id) })),
          { label: "New section…", onClick: () => createAndAssignSection() },
        ],
      },
      { label: "Delete", onClick: () => onDelete?.(capture.id), destructive: true },
    ];
  }
```

This needs new props threaded down to `CaptureItem` (`onReopen`, `onMerge`, `onDelete`, `projects: Project[]`, `sections: Section[]`) and local state (`editing`, `expanded`) plus helper functions (`openEditWindow`, `createAndAssignSection`) — wire `editing`/`onSave` into `EditableBody` (Task 16) in place of the current always-`MarkdownBody` render from Task 13, and thread `onDone`/`onDelete`/etc. from `App.tsx` down through the same prop-drilling pattern already used for `onPromote`/`onToggleSelect` today.

`createAndAssignSection` prompts for a name (a simple `window.prompt`-free inline text input reusing the same pattern as `SectionHeader`'s rename input, or a minimal native `prompt()` call as the simplest correct first pass) then calls `api.createSection(name).then((s) => api.assignCaptureSection(capture.id, s.id))`.

`openEditWindow` creates a new Tauri webview window (`new WebviewWindow(...)` from `@tauri-apps/api/webviewWindow`) pointed at a small new route/component that mounts just `EditableBody` for the given capture id, reusing `updateCaptureBody` on save.

- [ ] **Step 2: Manually verify every action**

Run: `pnpm tauri dev`. For a plain text capture: Mark Done, Copy, Copy as List (paste and confirm `- [ ] ...`), Edit (change the text, save, confirm it persists and re-renders as Markdown), Expand, Move to Project, Move to Section (create a new one via the submenu), Delete (confirm Undo toast appears and Undo works). For a screenshot capture: confirm Copy pastes a real image, Copy as List uses OCR text, Edit is disabled.

- [ ] **Step 3: Commit**

```bash
git add apps/desktop/src/components/CaptureItem.tsx apps/desktop/src/App.tsx
git commit -m "Wire the full context-menu action set"
```

---

### Task 21: Extend `MergeToolbar` into the full batch action bar

**Files:**
- Modify: `apps/desktop/src/components/MergeToolbar.tsx`
- Modify: `apps/desktop/src/App.tsx` (pass the new handlers)

**Interfaces:**
- Consumes: `api.copyCapturesAsChecklist`, `api.assignCaptureProject`, `api.assignCaptureSection`, `api.deleteCapture`, `api.mergeCaptures` (all existing/Task 8/15)

- [ ] **Step 1: Extend the toolbar**

In `MergeToolbar.tsx`, add props and buttons for the rest of the batch action set (Copy as List, Move to Project, Move to Section, Delete), alongside the existing Merge button — same conditional-render-only-when-`count > 0` shape as today, just more buttons in the same bar:

```tsx
interface MergeToolbarProps {
  count: number;
  onMerge: () => void;
  onCopyAsList: () => void;
  onMoveToProject: (projectId: number | null) => void;
  onMoveToSection: (sectionId: number | null) => void;
  onDelete: () => void;
  onClear: () => void;
  projects: { id: number; name: string }[];
  sections: { id: number; name: string }[];
}
```

Add the corresponding buttons (Copy as List, a "Move to..." dropdown pair, Delete) using the same button styling already established for Merge/Clear in this component — each simply calls its handler prop, no new interaction pattern introduced.

- [ ] **Step 2: Wire handlers in `App.tsx`**

```tsx
  async function handleCopyAsList() {
    await api.copyCapturesAsChecklist(Array.from(selected));
  }

  async function handleBatchDelete() {
    const ids = Array.from(selected);
    await Promise.all(ids.map((id) => api.deleteCapture(id)));
    setSelected(new Set());
    refreshStream();
    setUndoToast({
      message: `${ids.length} capture(s) deleted.`,
      onUndo: async () => {
        await Promise.all(ids.map((id) => api.restoreCapture(id)));
        refreshStream();
      },
    });
  }
```

Pass these, plus `onMoveToProject`/`onMoveToSection` (each mapping over `selected` calling the existing single-item assignment commands), into `<MergeToolbar>`.

- [ ] **Step 3: Manually verify**

Run: `pnpm tauri dev`. Select 3 captures — confirm the toolbar shows Merge, Copy as List, Move to Project, Move to Section, and Delete. Exercise each: Copy as List pastes all 3 as a checklist; Delete removes all 3 with a single Undo toast that restores all 3 together.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src/components/MergeToolbar.tsx apps/desktop/src/App.tsx
git commit -m "Extend MergeToolbar into the full batch action bar"
```

---

### Task 22: Expand / detail view

**Files:**
- Create: `apps/desktop/src/components/ExpandedCaptureModal.tsx`
- Modify: `apps/desktop/src/components/CaptureItem.tsx` (render when `expanded` is true, from Task 20)

**Interfaces:**
- Consumes: `MarkdownBody` (Task 12), `EditableBody` (Task 16)

- [ ] **Step 1: Implement**

Create `apps/desktop/src/components/ExpandedCaptureModal.tsx` — a simple centered overlay (no new dependency needed) showing the full body via `MarkdownBody`, with an Edit toggle reusing `EditableBody`:

```tsx
import { useState } from "react";
import type { Capture } from "@/lib/types";
import { EditableBody } from "./EditableBody";

interface ExpandedCaptureModalProps {
  capture: Capture;
  onClose: () => void;
  onSave: (body: string) => Promise<void>;
}

export function ExpandedCaptureModal({ capture, onClose, onSave }: ExpandedCaptureModalProps) {
  const [editing, setEditing] = useState(false);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/30"
      onClick={onClose}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className="max-h-[80vh] w-[32rem] max-w-[90vw] overflow-y-auto rounded-lg
                   bg-white p-4 shadow-xl dark:bg-neutral-900"
      >
        <EditableBody
          capture={capture}
          editing={editing}
          onSave={async (body) => {
            await onSave(body);
            setEditing(false);
          }}
          onCancel={() => setEditing(false)}
        />
        {!editing && capture.kind !== "session_digest" && (
          <button
            type="button"
            onClick={() => setEditing(true)}
            className="mt-3 text-xs text-slate-teal hover:underline dark:text-slate-teal-light"
          >
            Edit
          </button>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Wire into `CaptureItem`**

Render `{expanded && <ExpandedCaptureModal capture={capture} onClose={() => setExpanded(false)} onSave={(body) => api.updateCaptureBody(capture.id, body).then(refresh)} />}` alongside the existing component tree, using the `expanded` state introduced in Task 20.

- [ ] **Step 3: Manually verify**

Run: `pnpm tauri dev`. Click Expand on a long capture — confirm a modal shows the full rendered Markdown body, Edit works from inside it, clicking outside or a close action dismisses it.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src/components/ExpandedCaptureModal.tsx apps/desktop/src/components/CaptureItem.tsx
git commit -m "Add Expand/detail view for captures"
```

---

## Phase 5 — Keyboard-first navigation

### Task 23: Real-DOM-focus list containers and cursor state

**Files:**
- Modify: `apps/desktop/src/App.tsx` (stream list container)
- Modify: `apps/desktop/src/components/NowList.tsx` (Now list container)
- Create: `apps/desktop/src/lib/useListCursor.ts`

**Interfaces:**
- Produces: `useListCursor<T extends { id: number }>(items: T[]) -> { cursorId: number | null; setCursorId; onKeyDown: (e: KeyboardEvent) => void }`

- [ ] **Step 1: Implement the cursor hook**

Create `apps/desktop/src/lib/useListCursor.ts` — arrow keys move a highlight cursor independent of any checkbox selection, per the Gmail/Superhuman model in the design spec:

```typescript
import { useState } from "react";

export function useListCursor<T extends { id: number }>(items: T[]) {
  const [cursorId, setCursorId] = useState<number | null>(null);

  function onKeyDown(e: React.KeyboardEvent) {
    if (items.length === 0) return;
    const index = items.findIndex((i) => i.id === cursorId);
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setCursorId(items[Math.min(index + 1, items.length - 1)]?.id ?? items[0].id);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setCursorId(items[Math.max(index - 1, 0)]?.id ?? items[0].id);
    }
  }

  return { cursorId, setCursorId, onKeyDown };
}
```

- [ ] **Step 2: Wire real DOM focus into the stream container in `App.tsx`**

Change the stream's scroll container:

```tsx
              <div
                tabIndex={0}
                onKeyDown={streamCursor.onKeyDown}
                onClick={() => {
                  /* clicking the background, not just a row, gives this list focus */
                }}
                className="flex-1 overflow-y-auto focus:outline-none focus-within:ring-1 focus-within:ring-slate-teal/30"
              >
```

with `const streamCursor = useListCursor(visibleStream);` declared alongside the other stream state, and pass `selected={capture.id === streamCursor.cursorId}`-equivalent styling into each `CaptureItem` (a new `cursor` prop distinct from the existing checkbox `selected` prop — do not conflate the two, per the design spec's explicit "cursor position is not the selection" requirement).

- [ ] **Step 3: Apply the same treatment to `NowList`**

Add `tabIndex={0}` and the same `useListCursor(items)` wiring to `NowList`'s root container.

- [ ] **Step 4: Manually verify**

Run: `pnpm tauri dev`. Click into the stream, press Down/Up — confirm a highlight moves without touching any checkboxes. Tab — confirm focus moves to the Now sidebar and arrow keys now move its cursor instead. Click into the search box — confirm arrow keys type/move the caret in the input as normal, not the list cursor (native input behavior, unaffected by the list's own keydown handler since it's scoped to the list container, not global).

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/lib/useListCursor.ts apps/desktop/src/App.tsx apps/desktop/src/components/NowList.tsx
git commit -m "Add real-DOM-focus list containers with an independent cursor"
```

---

### Task 24: Space/Enter and single-key action shortcuts

**Files:**
- Modify: `apps/desktop/src/lib/useListCursor.ts` (extend `onKeyDown`)
- Modify: `apps/desktop/src/App.tsx`

**Interfaces:**
- Consumes: `useListCursor` (Task 23), all context-menu action handlers (Task 20-21)

- [ ] **Step 1: Extend the hook with an actions parameter**

Change `useListCursor`'s signature to accept an actions map and guard against text-input focus:

```typescript
interface ListCursorActions<T> {
  onToggleSelect?: (id: number) => void;
  onExpand?: (id: number) => void;
  onAction?: (key: string, id: number) => void;
}

export function useListCursor<T extends { id: number }>(
  items: T[],
  actions: ListCursorActions<T> = {},
) {
  const [cursorId, setCursorId] = useState<number | null>(null);

  function onKeyDown(e: React.KeyboardEvent) {
    // Critical scoping rule: never fire while any text input has focus --
    // otherwise typing "d" while composing a capture would fire "mark done"
    // instead of typing a letter. See the design spec's Keyboard-first
    // navigation section.
    const active = document.activeElement;
    const isTextInput =
      active instanceof HTMLInputElement ||
      active instanceof HTMLTextAreaElement ||
      (active instanceof HTMLElement && active.isContentEditable);
    if (isTextInput) return;

    if (items.length === 0) return;
    const index = items.findIndex((i) => i.id === cursorId);

    if (e.key === "ArrowDown") {
      e.preventDefault();
      setCursorId(items[Math.min(index + 1, items.length - 1)]?.id ?? items[0].id);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setCursorId(items[Math.max(index - 1, 0)]?.id ?? items[0].id);
    } else if (e.key === " " && cursorId !== null) {
      e.preventDefault();
      actions.onToggleSelect?.(cursorId);
    } else if (e.key === "Enter" && cursorId !== null) {
      e.preventDefault();
      actions.onExpand?.(cursorId);
    } else if (cursorId !== null) {
      actions.onAction?.(e.key, cursorId);
    }
  }

  return { cursorId, setCursorId, onKeyDown };
}
```

- [ ] **Step 2: Wire single-key actions in `App.tsx`**

```tsx
  const streamCursor = useListCursor(visibleStream, {
    onToggleSelect: toggleSelect,
    onExpand: (id) => setExpandedId(id),
    onAction: (key, id) => {
      const ids = selected.size > 0 ? Array.from(selected) : [id];
      if (key === "d") ids.forEach((i) => handleDone(i));
      else if (key === "c") api.copyCaptureText(id);
      else if (key === "C") api.copyCapturesAsChecklist(ids);
      else if (key === "e") setEditingId(id);
      else if (key === "M") ids.length >= 2 && api.mergeCaptures(ids);
      else if (key === "Backspace" || key === "Delete") ids.forEach((i) => api.deleteCapture(i));
    },
  });
```

(`selected.size > 0 ? Array.from(selected) : [id]` implements the spec's rule: an action key acts on the checked selection if non-empty, otherwise the cursor row — the same rule the context menu itself uses.)

- [ ] **Step 3: Manually verify**

Run: `pnpm tauri dev`. Click into the stream, arrow down to a capture, press `d` — confirm it's marked done. Select 2 captures via Space, press `C` — confirm both copy as a checklist. Click into the search box and type the letter "d" — confirm it types normally and does not mark anything done.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src/lib/useListCursor.ts apps/desktop/src/App.tsx
git commit -m "Add Space/Enter and single-key action shortcuts to list navigation"
```

---

### Task 25: Keyboard shortcut opens the context menu

**Files:**
- Modify: `apps/desktop/src/components/CaptureItem.tsx`
- Modify: `apps/desktop/src/App.tsx`

**Interfaces:**
- Consumes: `ContextMenu`'s `openRef` mechanism (Task 18)

- [ ] **Step 1: Expose a per-row menu-open ref keyed by capture id**

In `App.tsx`, maintain `const menuRefs = useRef(new Map<number, (x: number, y: number) => void>());` and pass a registration callback into each `CaptureItem` so it can register its own `openRef` setter; add a case to `useListCursor`'s `onAction` for a dedicated key (e.g. the `Menu`/context-menu key, or `m` if unused elsewhere — reconcile against Task 24's existing single-key bindings for collisions) that calls `menuRefs.current.get(cursorId)?.(centerOfCursorRow.x, centerOfCursorRow.y)` — computing the cursor row's screen position via `document.getElementById(`capture-${cursorId}`)?.getBoundingClientRect()`, requiring each `CaptureItem` root to carry `id={`capture-${capture.id}`}`.

- [ ] **Step 2: Manually verify**

Run: `pnpm tauri dev`. Arrow down to a row, press the dedicated shortcut — confirm the identical context menu opens near that row, with the same items right-click produces.

- [ ] **Step 3: Commit**

```bash
git add apps/desktop/src/components/CaptureItem.tsx apps/desktop/src/App.tsx
git commit -m "Add keyboard shortcut parity for opening the context menu"
```

---

## Phase 6 — Custom Shortcuts

### Task 26: `settings` table and Store methods

**Files:**
- Create: `crates/magpie-core/migrations/0009_settings.sql`
- Modify: `crates/magpie-core/src/db.rs`
- Create: `crates/magpie-core/src/settings.rs`
- Modify: `crates/magpie-core/src/lib.rs`

**Interfaces:**
- Produces: `Store::get_setting(&self, key: &str) -> Result<Option<String>>`
- Produces: `Store::set_setting(&self, key: &str, value: &str) -> Result<()>`

- [ ] **Step 1: Write the migration**

Create `crates/magpie-core/migrations/0009_settings.sql`:

```sql
-- Simple key/value settings storage -- first user: the two remappable
-- global hotkeys (capture, screenshot). See
-- docs/superpowers/specs/2026-07-31-capture-list-v2-design.md's
-- "Custom Shortcuts".
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

Register it in `db.rs`'s `MIGRATIONS` array as `0009_settings`.

- [ ] **Step 2: Write failing tests**

Create `crates/magpie-core/src/settings.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;

    #[test]
    fn get_setting_returns_none_when_unset() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.get_setting("capture_hotkey").unwrap(), None);
    }

    #[test]
    fn set_then_get_round_trips_and_overwrites() {
        let store = Store::open_in_memory().unwrap();
        store.set_setting("capture_hotkey", "CommandOrControl+Shift+M").unwrap();
        assert_eq!(
            store.get_setting("capture_hotkey").unwrap(),
            Some("CommandOrControl+Shift+M".to_string())
        );
        store.set_setting("capture_hotkey", "CommandOrControl+Shift+K").unwrap();
        assert_eq!(
            store.get_setting("capture_hotkey").unwrap(),
            Some("CommandOrControl+Shift+K".to_string())
        );
    }
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p magpie-core settings::`
Expected: FAIL to compile.

- [ ] **Step 4: Implement**

Add above the test module in `settings.rs`:

```rust
use rusqlite::{params, OptionalExtension};

use crate::error::Result;
use crate::Store;

impl Store {
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        self.with_conn(|conn| {
            Ok(conn
                .query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get(0))
                .optional()?)
        })
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
            Ok(())
        })
    }
}
```

Register `mod settings;` in `crates/magpie-core/src/lib.rs`.

- [ ] **Step 5: Run to verify pass**

Run: `cargo test -p magpie-core` — Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/magpie-core/migrations/0009_settings.sql crates/magpie-core/src/db.rs \
        crates/magpie-core/src/settings.rs crates/magpie-core/src/lib.rs
git commit -m "Add a key/value settings table for remappable hotkeys"
```

---

### Task 27: Settings window UI

**Files:**
- Modify: `apps/desktop/src-tauri/tauri.conf.json` (declare a new `settings` window)
- Create: `apps/desktop/src-tauri/src/settings_commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Create: `apps/desktop/src/SettingsApp.tsx`
- Modify: `apps/desktop/src/main.tsx` or add `apps/desktop/src/settings-main.tsx` (matching how `dock-main.tsx` is a separate entry point for the existing pinned-dock window)

**Interfaces:**
- Produces: Tauri commands `get_hotkey_settings(state) -> CmdResult<{capture: String, screenshot: String}>`, validated in Task 28

- [ ] **Step 1: Declare the window**

In `apps/desktop/src-tauri/tauri.conf.json`, add a `settings` window entry alongside the existing `main`/`dock`/`toast` windows (check the file's exact current shape first and match its conventions for `label`, `url`, `width`/`height`, `visible: false` initially).

- [ ] **Step 2: Add a settings-only Tauri command file**

Create `apps/desktop/src-tauri/src/settings_commands.rs`:

```rust
use tauri::State;

use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

#[tauri::command]
pub fn get_hotkey_settings(state: State<AppState>) -> CmdResult<serde_json::Value> {
    let capture = state
        .store
        .get_setting("capture_hotkey")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| crate::HOTKEY.to_string());
    let screenshot = state
        .store
        .get_setting("screenshot_hotkey")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| crate::SCREENSHOT_HOTKEY.to_string());
    Ok(serde_json::json!({ "capture": capture, "screenshot": screenshot }))
}
```

(`crate::HOTKEY`/`crate::SCREENSHOT_HOTKEY` need to become `pub(crate) const` rather than private `const` in `lib.rs` for this to compile — make that visibility change as part of this step.)

Register `mod settings_commands;` and `commands::get_hotkey_settings` (via `settings_commands::get_hotkey_settings`) in `lib.rs`.

- [ ] **Step 3: Build the Settings UI**

Create `apps/desktop/src/SettingsApp.tsx` — a minimal form with two text inputs (capture hotkey, screenshot hotkey) pre-filled from `get_hotkey_settings`, each validated client-side to require at least one modifier (`⌘`/`Ctrl`/`⌥`/`⇧`) before allowing Save — the actual save/rebind logic lands in Task 28.

- [ ] **Step 4: Manually verify the window opens**

Wire a "Settings…" menu item into the existing tray menu (`tray.rs`) that opens/shows the new window. Run: `pnpm tauri dev`, click it — confirm the window opens showing the current hotkeys.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/tauri.conf.json apps/desktop/src-tauri/src/settings_commands.rs \
        apps/desktop/src-tauri/src/lib.rs apps/desktop/src-tauri/src/tray.rs \
        apps/desktop/src/SettingsApp.tsx
git commit -m "Add a Settings window for hotkey configuration"
```

---

### Task 28: Runtime rebind, validation, and failure surfacing

**Files:**
- Modify: `apps/desktop/src-tauri/src/settings_commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs` (hold shortcuts in re-registerable state)
- Modify: `apps/desktop/src/SettingsApp.tsx`

**Interfaces:**
- Produces: Tauri command `set_hotkey(state, kind: "capture" | "screenshot", combo: String) -> CmdResult<()>` — errors if `combo` lacks a modifier, or if OS registration fails

- [ ] **Step 1: Implement the rebind command**

Add to `settings_commands.rs`:

```rust
#[tauri::command]
pub fn set_hotkey(
    app: tauri::AppHandle,
    state: State<AppState>,
    kind: String,
    combo: String,
) -> CmdResult<()> {
    let has_modifier = ["Command", "Control", "Alt", "Shift", "Cmd", "Ctrl", "Option"]
        .iter()
        .any(|m| combo.contains(m));
    if !has_modifier {
        return Err(format!(
            "\"{combo}\" has no modifier key -- binding a bare key would break normal typing"
        ));
    }

    let new_shortcut: tauri_plugin_global_shortcut::Shortcut =
        combo.parse().map_err(|e| format!("invalid shortcut syntax: {e}"))?;

    let setting_key = match kind.as_str() {
        "capture" => "capture_hotkey",
        "screenshot" => "screenshot_hotkey",
        other => return Err(format!("unknown hotkey kind: {other}")),
    };
    let previous = state
        .store
        .get_setting(setting_key)
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| if kind == "capture" { crate::HOTKEY.to_string() } else { crate::SCREENSHOT_HOTKEY.to_string() });

    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let gs = app.global_shortcut();
    let old_shortcut: tauri_plugin_global_shortcut::Shortcut =
        previous.parse().expect("previously-stored shortcut must itself be valid");
    let _ = gs.unregister(old_shortcut);

    // Registration can fail if another app already owns this combination --
    // that's a real, expected failure mode, not an edge case to ignore. On
    // failure, restore the old binding so the app doesn't silently end up
    // with no hotkey registered at all, and report the failure honestly
    // rather than claiming success.
    if let Err(e) = gs.register(new_shortcut) {
        let _ = gs.register(old_shortcut);
        return Err(format!("could not register \"{combo}\": {e} (still using \"{previous}\")"));
    }

    state
        .store
        .set_setting(setting_key, &combo)
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 2: Read stored hotkeys at startup instead of the hardcoded consts**

In `lib.rs`'s `setup()`, after opening the `Store` and before building the `tauri_plugin_global_shortcut::Builder`, read `store.get_setting("capture_hotkey")`/`"screenshot_hotkey"`, falling back to `HOTKEY`/`SCREENSHOT_HOTKEY` when unset, and use those resolved strings (not the raw consts) when parsing `Shortcut`s and calling `.with_shortcuts([...])`. This requires restructuring `run()` slightly so the `Store` is opened before the shortcut plugin is registered, rather than after (currently the plugin closure runs before `setup()` opens the store) — move the `Store::open(...)` call earlier, before the `.plugin({ ... global_shortcut ... })` block, and share it into `setup()` via a captured variable rather than re-opening it, since a `Store` should only be opened once per process.

- [ ] **Step 3: Wire the Settings UI's Save action**

In `SettingsApp.tsx`, call `invoke("set_hotkey", { kind: "capture", combo })` on Save; on error, display the returned message inline (e.g. "could not register... still using..."), matching the design spec's requirement that a failed rebind must surface as a visible failure, not a false success. On success, show a brief confirmation.

- [ ] **Step 4: Manually verify both the happy path and the failure path**

Run: `pnpm tauri dev`. Rebind the capture hotkey to an unused combo (e.g. `CommandOrControl+Shift+K`) — confirm it takes effect immediately without restarting the app (press the new combo, confirm a capture toast fires; press the old combo, confirm nothing happens). Attempt to rebind to a combo already owned by another running app (e.g. `CommandOrControl+Space` if Spotlight has it) — confirm the Settings UI shows a visible error and the old binding keeps working. Attempt to rebind to a bare unmodified key (e.g. `A`) — confirm the modifier-required validation rejects it before even attempting registration.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/settings_commands.rs apps/desktop/src-tauri/src/lib.rs apps/desktop/src/SettingsApp.tsx
git commit -m "Add runtime hotkey rebinding with validation and failure surfacing"
```

---

## Self-Review Notes

- **Spec coverage:** Sections (Tasks 1-3, 19), Deletion (Tasks 4-11), Markdown rendering (Tasks 12-13), Copy/Copy as List (Tasks 14-15), Edit (Task 16), Context menu + Move to Project/Section + Merge/Delete (Tasks 17-22), keyboard-first navigation (Tasks 23-25), Custom Shortcuts (Tasks 26-28) — every top-level section of the design spec maps to at least one task above.
- **No CLI/MCP changes anywhere in this plan** — matches the spec's explicit "no delete/purge MCP tool, ever" constraint; `crates/magpie-cli` and `crates/magpie-mcp` are never listed as Modify/Create targets in any task.
- **Cross-window sync events** (mentioned in the spec) are intentionally left to each task's own judgment rather than a separate task — Tasks 10, 19, 20, and 21 all involve mutations visible from both the main window and the pinned dock; when implementing each, follow the existing `now:changed`/`capture:updated` broadcast pattern from `apps/desktop/src/lib/events.ts` for whichever of that task's mutations the dock also displays (Now-list membership, section assignment, deletion of a Now item).
- **Type consistency check:** `Section` is defined once (Task 1: `model.rs`) and reused verbatim (Tasks 2, 3, 6, 8, 19) — no renamed variant introduced later. `capture_display_text` (Task 15) and `update_capture_body` (Task 16) are each defined once and consumed by exactly the commands that need them (Tasks 15/20 and 16/20 respectively).

## Backlog (not scheduled in this plan)

### Task 29 (future): Real "Edit in New Window" second-window support

Task 20 shipped "Edit in New Window" as a stub (opens the same in-row Expand modal in edit mode) rather than a genuine separate Tauri window, since real second-window infrastructure is a distinct, non-trivial piece of work. Revisited mid-execution: captures have no inherent length limit (not just short snippets — a full AI response, a large code block, a long article can all be captured), so a real separate window is worth building — the same reason Notion/Bear/Apple Notes offer "open in new window" for long documents; the Expand modal alone doesn't give the same room, isn't resizable independently of the main app, and can't be dragged to a second monitor. Decided to scope this as its own follow-up task rather than grow Task 20 further, since it needs:
- A new route/entry point (mirroring how `dock-main.tsx`/`DockApp.tsx` is a separate mounted window today)
- A new Tauri window declared in `tauri.conf.json`
- Capability grants for that window
- A `get_capture(id)` Tauri command (the new window needs to fetch the capture standalone, not receive it via props)
- Close/focus semantics consistent with this app's existing multi-window patterns (main, dock, toast)

Not scheduled with a number yet — pick this up as a fresh brainstorming→plan cycle when ready, rather than bolting it onto the current 28-task sequence.
