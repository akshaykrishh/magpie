# Confidence-Aware Capture Filing (Phase 1 of the magpie Explorations canonical design) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the desktop app's ambient captures (hotkey copy, screenshot) a visible, honest "where is this going?" signal in the toast, with a near-zero-friction way to confirm or ignore it — without ever silently auto-filing a guess.

**Architecture:** magpie-core gains a `last_active_at` recency signal on `projects`, bumped whenever a capture is filed into one. The desktop app's capture flow reads the single most-recently-active project as a *proposed* destination and shows it in the toast — but never writes `project_id` for it. A quick tap of the same capture hotkey (release within 250ms, using the `ShortcutState::Released` event the global-shortcut plugin already emits) commits the guess via the existing `assign_project`/`assign_capture_project` path. Ignoring it, or holding past 250ms, leaves the capture in Inbox exactly as today. This is additive to, not a departure from, `docs/design.md`'s "nothing is ever auto-filed" principle — see that file's "Projects and multi-session" section for why filing is deliberately never automatic.

**Tech Stack:** Rust (magpie-core: rusqlite/SQLite; apps/desktop/src-tauri: Tauri v2, tauri-plugin-global-shortcut), TypeScript (apps/desktop/src, plain webview — the toast is not part of the React app).

## Global Constraints

- Filing a capture into a project must never happen without either (a) a deterministic, unambiguous signal (MCP's git-remote `detect()` — unchanged by this plan, already correct) or (b) an explicit user action (tap/confirm). A *guessed* destination that the user never acts on must leave `project_id` as `NULL` (Inbox) — copied verbatim from `docs/design.md`: *"Nothing is ever auto-filed, because auto-filing that's 85% right creates more work than none."*
- Migration files go in `crates/magpie-core/migrations/000N_<name>.sql`, registered as a new tuple appended to `MIGRATIONS` in `crates/magpie-core/src/db.rs`. The next free number is `0004`.
- Match existing code style: doc comments explain *why*, not *what* (see any existing function in `captures.rs`/`projects.rs` for the tone to match).
- This plan explicitly does **not** implement "hold to aim" (a keyboard-driven project picker for redirecting a guess to a different project). Holding past the 250ms tap threshold is a safe no-op in this plan — the capture simply stays in Inbox, identical to today's behavior. That picker is real, planned work, but is its own follow-up plan once this lands (it needs a new floating window, not just event-handler changes).
- This plan does **not** add a guess to screenshot captures (`on_screenshot_hotkey`) — screenshots already go through a distinct region-selection interaction; adding tap/hold semantics on top is out of scope here. Screenshot toasts keep today's plain "Captured" message.
- Desktop-originated captures currently *never* set `project_id` (confirmed: `Store::capture()` has no `project_id` parameter at all, and `capture_flow.rs` never calls `assign_project`). This plan is what first gives them a path to a project, without weakening MCP's existing git-certain path (`crates/magpie-mcp/src/project.rs`'s `detect()`), which is untouched.

---

### Task 1: Project recency tracking in magpie-core

**Files:**
- Create: `crates/magpie-core/migrations/0004_project_recency.sql`
- Modify: `crates/magpie-core/src/db.rs:19-26` (register the migration)
- Modify: `crates/magpie-core/src/model.rs:3-10` (`Project` struct)
- Modify: `crates/magpie-core/src/projects.rs` (row mapping, columns, touch helper, `get_or_create_project`, new `list_projects_by_recency`)
- Modify: `crates/magpie-core/src/captures.rs:216-224` (`assign_project` touches the newly-assigned project)
- Test: inline `#[cfg(test)]` modules in `projects.rs` and `captures.rs`

**Interfaces:**
- Produces: `Store::list_projects_by_recency(&self, limit: i64) -> Result<Vec<Project>>` — ordered most-recently-touched first, untouched projects last. Consumed by Task 2.
- Produces: `pub(crate) fn touch_project_active_tx(conn: &rusqlite::Connection, project_id: i64) -> rusqlite::Result<()>` in `projects.rs` — internal, called from both `projects.rs` and `captures.rs`.
- Produces: `Project.last_active_at: Option<String>` field.

- [ ] **Step 1: Write the migration**

Create `crates/magpie-core/migrations/0004_project_recency.sql`:

```sql
-- Recency signal for projects, bumped whenever a capture is filed into one
-- (see Store::touch_project_active_tx). Powers "projects ordered by recency
-- of your own activity, not alphabetically" for the desktop app's
-- capture-filing guess (docs/design.md's dock already describes this
-- ordering for the focused-project list; this extends it to a queryable
-- column instead of being implicit in session state).
ALTER TABLE projects ADD COLUMN last_active_at TEXT;

CREATE INDEX projects_last_active_at_idx
    ON projects (last_active_at DESC) WHERE last_active_at IS NOT NULL;
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
];
```

- [ ] **Step 3: Run the existing migration test to verify it still passes with the new migration applied**

Run: `cargo test -p magpie-core migrates_cleanly_and_is_idempotent`
Expected: PASS (it asserts `PRAGMA user_version == MIGRATIONS.len()`, which now includes the new migration automatically).

- [ ] **Step 4: Add `last_active_at` to the `Project` struct**

In `crates/magpie-core/src/model.rs`, change:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub remote_url: Option<String>,
    pub common_git_dir: Option<String>,
    pub created_at: String,
}
```

to:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub remote_url: Option<String>,
    pub common_git_dir: Option<String>,
    pub created_at: String,
    pub last_active_at: Option<String>,
}
```

- [ ] **Step 5: Write the failing tests for recency tracking**

In `crates/magpie-core/src/projects.rs`, add to the existing `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn list_projects_by_recency_orders_touched_projects_first() {
        let store = Store::open_in_memory().unwrap();
        let a = store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();
        let b = store
            .get_or_create_project("b", Some("git@github.com:x/b.git"), None)
            .unwrap();
        // Touch b again, after a -- b should now rank first.
        store
            .get_or_create_project("b", Some("git@github.com:x/b.git"), None)
            .unwrap();

        let ranked = store.list_projects_by_recency(10).unwrap();
        assert_eq!(ranked[0].id, b.id);
        assert_eq!(ranked[1].id, a.id);
    }

    #[test]
    fn list_projects_by_recency_respects_limit() {
        let store = Store::open_in_memory().unwrap();
        store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();
        store
            .get_or_create_project("b", Some("git@github.com:x/b.git"), None)
            .unwrap();
        let ranked = store.list_projects_by_recency(1).unwrap();
        assert_eq!(ranked.len(), 1);
    }

    #[test]
    fn newly_created_project_has_a_recency_timestamp() {
        let store = Store::open_in_memory().unwrap();
        let p = store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();
        assert!(p.last_active_at.is_some());
    }
```

And to `crates/magpie-core/src/captures.rs`'s existing `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn assigning_a_capture_touches_its_project_recency() {
        let store = Store::open_in_memory().unwrap();
        let proj = store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();
        let capture = store.capture("something", None).unwrap();

        store.assign_project(capture.id, Some(proj.id)).unwrap();

        let refreshed = store.get_project(proj.id).unwrap();
        assert!(refreshed.last_active_at.is_some());
    }
```

- [ ] **Step 6: Run the new tests to verify they fail**

Run: `cargo test -p magpie-core list_projects_by_recency`
Expected: FAIL to compile — `list_projects_by_recency` doesn't exist yet, and `Project { ... }` construction sites are missing the new field.

- [ ] **Step 7: Implement the touch helper, wire it into `get_or_create_project`, and add `list_projects_by_recency`**

In `crates/magpie-core/src/projects.rs`, change the row mapper and column list:

```rust
fn project_from_row(row: &Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get("id")?,
        name: row.get("name")?,
        remote_url: row.get("remote_url")?,
        common_git_dir: row.get("common_git_dir")?,
        created_at: row.get("created_at")?,
        last_active_at: row.get("last_active_at")?,
    })
}

const PROJECT_COLUMNS: &str =
    "id, name, remote_url, common_git_dir, created_at, last_active_at";

/// Bump a project's recency signal -- called whenever a capture is filed
/// into it, so `list_projects_by_recency` reflects "projects I've actually
/// touched lately", not alphabetical order. Not a public `Store` method:
/// it's an internal side effect of filing, never an action a caller takes
/// on its own.
pub(crate) fn touch_project_active_tx(
    conn: &rusqlite::Connection,
    project_id: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE projects SET last_active_at = ?1 WHERE id = ?2",
        params![now_iso(), project_id],
    )?;
    Ok(())
}
```

Replace `get_or_create_project`'s body so both the found-existing and newly-created paths touch recency before returning:

```rust
    pub fn get_or_create_project(
        &self,
        name: &str,
        remote_url: Option<&str>,
        common_git_dir: Option<&str>,
    ) -> Result<Project> {
        self.with_conn(|conn| {
            if let Some(remote_url) = remote_url {
                let sql = format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE remote_url = ?1");
                if let Some(p) = conn
                    .query_row(&sql, params![remote_url], project_from_row)
                    .optional()?
                {
                    touch_project_active_tx(conn, p.id)?;
                    let sql = format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id = ?1");
                    return Ok(conn.query_row(&sql, params![p.id], project_from_row)?);
                }
            } else if let Some(common_git_dir) = common_git_dir {
                let sql =
                    format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE common_git_dir = ?1");
                if let Some(p) = conn
                    .query_row(&sql, params![common_git_dir], project_from_row)
                    .optional()?
                {
                    touch_project_active_tx(conn, p.id)?;
                    let sql = format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id = ?1");
                    return Ok(conn.query_row(&sql, params![p.id], project_from_row)?);
                }
            }

            conn.execute(
                "INSERT INTO projects (name, remote_url, common_git_dir, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![name, remote_url, common_git_dir, now_iso()],
            )?;
            let id = conn.last_insert_rowid();
            touch_project_active_tx(conn, id)?;
            let sql = format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id = ?1");
            Ok(conn.query_row(&sql, params![id], project_from_row)?)
        })
    }
```

Add the new query method to the same `impl Store` block, after `list_projects`:

```rust
    /// Projects ordered by most-recently-touched first (via
    /// `touch_project_active_tx`), untouched projects last. This is the
    /// ranking the desktop app's capture-filing guess uses -- see
    /// docs/superpowers/plans/2026-07-31-confidence-aware-capture-filing.md.
    pub fn list_projects_by_recency(&self, limit: i64) -> Result<Vec<Project>> {
        self.with_conn(|conn| {
            let sql = format!(
                "SELECT {PROJECT_COLUMNS} FROM projects
                 ORDER BY last_active_at IS NULL, last_active_at DESC, id DESC
                 LIMIT ?1"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![limit], project_from_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }
```

In `crates/magpie-core/src/captures.rs`, change `assign_project`:

```rust
    pub fn assign_project(&self, id: i64, project_id: Option<i64>) -> Result<Capture> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE captures SET project_id = ?1 WHERE id = ?2",
                params![project_id, id],
            )?;
            if let Some(project_id) = project_id {
                crate::projects::touch_project_active_tx(conn, project_id)?;
            }
            get_capture_tx(conn, id)
        })
    }
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cargo test -p magpie-core`
Expected: PASS, including all pre-existing tests (this is an additive column with an internal-only new function; nothing existing changes shape besides the `Project` struct gaining a field, which is why Step 6 needed the field added first).

- [ ] **Step 9: Commit**

```bash
git add crates/magpie-core/migrations/0004_project_recency.sql \
        crates/magpie-core/src/db.rs \
        crates/magpie-core/src/model.rs \
        crates/magpie-core/src/projects.rs \
        crates/magpie-core/src/captures.rs
git commit -m "Add project recency tracking to magpie-core"
```

---

### Task 2: Compute a filing guess and carry it in the toast payload

**Files:**
- Modify: `apps/desktop/src-tauri/src/toast.rs` (new `ToastPayload` type)
- Modify: `apps/desktop/src-tauri/src/capture_flow.rs` (compute the guess, rename plain-message call sites, new tests)

**Interfaces:**
- Consumes: `Store::list_projects_by_recency(limit: i64) -> Result<Vec<Project>>` from Task 1.
- Produces: `toast::ToastPayload` enum (`Plain { message }` / `Guess { capture_id, project_id, project_name }`), serialized over the existing `"toast:show"` Tauri event. Consumed by Task 3 (frontend) and Task 4 (tap-to-confirm, via the `capture_id`/`project_id` it carries).
- Produces: `fn guess_toast_payload(store: &magpie_core::Store, capture_id: i64) -> ToastPayload` (module-private to `capture_flow.rs`, unit-tested directly).

- [ ] **Step 1: Write the failing tests for the guess computation**

Add to `apps/desktop/src-tauri/src/capture_flow.rs` a new `#[cfg(test)] mod tests` block at the end of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guess_falls_back_to_plain_when_no_projects_exist() {
        let store = magpie_core::Store::open_in_memory().unwrap();
        let capture = store.capture("something", None).unwrap();

        let payload = guess_toast_payload(&store, capture.id);

        assert!(matches!(payload, ToastPayload::Plain { .. }));
    }

    #[test]
    fn guess_names_the_most_recently_active_project() {
        let store = magpie_core::Store::open_in_memory().unwrap();
        store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();
        let b = store
            .get_or_create_project("b", Some("git@github.com:x/b.git"), None)
            .unwrap();
        let capture = store.capture("something", None).unwrap();

        let payload = guess_toast_payload(&store, capture.id);

        match payload {
            ToastPayload::Guess {
                capture_id,
                project_id,
                project_name,
            } => {
                assert_eq!(capture_id, capture.id);
                assert_eq!(project_id, b.id);
                assert_eq!(project_name, "b");
            }
            other => panic!("expected a guess, got {other:?}"),
        }
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p magpie-desktop guess_`
Expected: FAIL to compile — `ToastPayload` and `guess_toast_payload` don't exist yet. (Confirm the actual package name of `apps/desktop/src-tauri` via its `Cargo.toml` `[package].name` if `magpie-desktop` doesn't match — it's whatever that file declares.)

- [ ] **Step 3: Add `ToastPayload` to `toast.rs`**

In `apps/desktop/src-tauri/src/toast.rs`, add near the top (after the existing module doc comment, before `use tauri::{AppHandle, Manager};`):

```rust
use serde::Serialize;

/// Everything the toast window can be told to show. `Plain` covers the
/// existing flat-message cases (capture ok/failed, nothing to capture,
/// secure input blocked). `Guess` is a proposed filing destination -- it is
/// never written to the capture's `project_id` just by being shown (see
/// docs/design.md "nothing is ever auto-filed"); committing it is a
/// separate, explicit action (see capture_flow.rs's tap-to-confirm).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToastPayload {
    Plain {
        message: String,
    },
    Guess {
        capture_id: i64,
        project_id: i64,
        project_name: String,
    },
}
```

- [ ] **Step 4: Compute the guess and thread it through `fire_toast`**

In `apps/desktop/src-tauri/src/capture_flow.rs`, change the import line:

```rust
use crate::toast::{hide_toast, show_toast};
```

to:

```rust
use crate::toast::{hide_toast, show_toast, ToastPayload};
```

Change `fire_toast` to take a `ToastPayload`, and add a `fire_plain_toast` convenience wrapper for the many call sites that only ever showed a flat message:

```rust
fn fire_toast(app: &AppHandle, payload: ToastPayload) {
    let _ = app.emit_to("toast", "toast:show", payload);
    show_toast(app);

    // AppKit window/panel calls must happen on the main thread -- see the
    // M0 postmortem in git history for the crash this avoids.
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(TOAST_VISIBLE_MS));
        let for_main_thread = app.clone();
        let _ = app.run_on_main_thread(move || {
            hide_toast(&for_main_thread);
        });
    });
}

fn fire_plain_toast(app: &AppHandle, message: &str) {
    fire_toast(
        app,
        ToastPayload::Plain {
            message: message.to_string(),
        },
    );
}

/// The desktop app's own guess at where a capture belongs: the single most
/// recently active project, if any exist yet. This is deliberately a weak,
/// visible-as-a-guess signal (see ToastPayload::Guess's doc comment) --
/// unlike MCP's `detect()` (crates/magpie-mcp/src/project.rs), which is
/// certain because it reads the actual git remote of the session's cwd,
/// this has no comparable certainty available: an ambient hotkey/screenshot
/// capture could have come from any app.
fn guess_toast_payload(store: &magpie_core::Store, capture_id: i64) -> ToastPayload {
    match store.list_projects_by_recency(1) {
        Ok(projects) => match projects.into_iter().next() {
            Some(p) => ToastPayload::Guess {
                capture_id,
                project_id: p.id,
                project_name: p.name,
            },
            None => ToastPayload::Plain {
                message: "Captured".to_string(),
            },
        },
        Err(e) => {
            eprintln!("magpie: project recency lookup failed: {e}");
            ToastPayload::Plain {
                message: "Captured".to_string(),
            }
        }
    }
}
```

Update every existing plain-message call site to use `fire_plain_toast` instead of `fire_toast`, and change the successful-hotkey-capture arm to compute and show a guess. In `on_capture_hotkey`:

```rust
pub fn on_capture_hotkey(app: &AppHandle) {
    let state = app.state::<AppState>();

    let text = match state.backend.read_capture_text() {
        Ok(Some(t)) => t,
        Ok(None) => {
            let message = if state.backend.secure_input_blocked() {
                "Can't copy from this app (Secure Input) — copy manually, then retry"
            } else {
                "Nothing to capture"
            };
            fire_plain_toast(app, message);
            return;
        }
        Err(e) => {
            eprintln!("magpie: capture read failed: {e}");
            fire_plain_toast(app, "Capture failed");
            return;
        }
    };

    let source = state
        .backend
        .front_app()
        .ok()
        .flatten()
        .map(|s| magpie_core::NewSource {
            app_name: s.app_name,
            bundle_id: s.bundle_id,
            window_title: s.window_title,
            url: s.url,
        });

    match state.store.capture(&text, source) {
        Ok(capture) => {
            let _ = app.emit("capture:added", ());
            let payload = guess_toast_payload(&state.store, capture.id);
            fire_toast(app, payload);
        }
        Err(e) => {
            eprintln!("magpie: capture insert failed: {e}");
            fire_plain_toast(app, "Capture failed");
        }
    }
}
```

In `on_screenshot_hotkey`, change every `fire_toast(app, "...")` call to `fire_plain_toast(app, "...")` (four call sites: the missing-blobs-dir error, the capture-region error, the success case, and the insert-error case) — screenshots keep today's plain "Captured" message, per this plan's Global Constraints.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p magpie-desktop guess_`
Expected: PASS.

- [ ] **Step 6: Run the full desktop crate's test suite and a release-mode type check to catch any remaining `fire_toast` call sites**

Run: `cargo check -p magpie-desktop && cargo test -p magpie-desktop`
Expected: both succeed with no leftover calls passing a bare `&str` to `fire_toast`.

- [ ] **Step 7: Commit**

```bash
git add apps/desktop/src-tauri/src/toast.rs apps/desktop/src-tauri/src/capture_flow.rs
git commit -m "Compute a filing guess and carry it in the toast payload"
```

---

### Task 3: Render the guess in the toast UI

**Files:**
- Modify: `apps/desktop/toast.html`
- Modify: `apps/desktop/src/toast.ts`

**Interfaces:**
- Consumes: the `ToastPayload` JSON shape from Task 2 (`{"kind":"plain","message":"..."}` or `{"kind":"guess","capture_id":N,"project_id":N,"project_name":"..."}`), delivered over the existing `"toast:show"` Tauri event.

- [ ] **Step 1: Update the markup**

In `apps/desktop/toast.html`, change the `<style>` block to add rules for the new destination pill (append after the existing `#toast .check` rule):

```css
      #toast .arrow {
        opacity: 0.5;
        margin: 0 2px;
      }
      #toast .guess {
        border-bottom: 1px dashed rgba(255, 255, 255, 0.5);
      }
```

Change the body markup from:

```html
    <div id="toast">
      <span class="check">✓</span>
      <span id="msg">Captured</span>
    </div>
```

to:

```html
    <div id="toast">
      <span class="check">✓</span>
      <span id="msg">Captured</span>
      <span id="dest" hidden>
        <span class="arrow">→</span>
        <span id="dest-name" class="guess"></span>
      </span>
    </div>
```

- [ ] **Step 2: Update the frontend logic**

Replace the full contents of `apps/desktop/src/toast.ts` with:

```ts
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";

type ToastPayload =
  | { kind: "plain"; message: string }
  | { kind: "guess"; capture_id: number; project_id: number; project_name: string };

const msg = document.getElementById("msg")!;
const dest = document.getElementById("dest")!;
const destName = document.getElementById("dest-name")!;

listen<ToastPayload>("toast:show", (event) => {
  const payload = event.payload;
  if (payload.kind === "plain") {
    msg.textContent = payload.message;
    dest.hidden = true;
  } else {
    msg.textContent = "Captured";
    destName.textContent = payload.project_name;
    dest.hidden = false;
  }
});

// Sanity check for the M0 spike: prove this window never becomes the
// OS-level focused window. If it ever does, `focused` flips to true here.
getCurrentWindow().onFocusChanged(({ payload: focused }) => {
  if (focused) {
    console.error("[M0 spike] toast window took OS focus — non-activating setup failed");
    document.title = "FOCUS STOLEN";
  }
});
```

- [ ] **Step 3: Manual verification (no frontend test harness exists for this webview today)**

Run: `cd apps/desktop && pnpm tauri dev`
Steps:
1. With no projects yet in the database, press the capture hotkey (`Cmd+Shift+M` on macOS) over some copyable text. Expected: toast shows only "✓ Captured", no arrow/destination — same as before this plan.
2. Create a project (e.g. run `magpie-mcp` once from inside a git repo with a remote, or use the desktop app's existing project UI if present) so `list_projects_by_recency` has something to return.
3. Press the capture hotkey again. Expected: toast shows "✓ Captured → `<project name>`" with the project name underlined with a dashed border.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/toast.html apps/desktop/src/toast.ts
git commit -m "Render the filing guess in the toast UI"
```

---

### Task 4: Tap-to-confirm via the existing hotkey release event

**Files:**
- Modify: `apps/desktop/src-tauri/src/state.rs` (new `PendingGuess`, new `AppState` field)
- Modify: `apps/desktop/src-tauri/src/capture_flow.rs` (record the pending guess on Pressed, resolve it on Released)
- Modify: `apps/desktop/src-tauri/src/lib.rs` (route `ShortcutState::Released` for the capture hotkey)

**Interfaces:**
- Consumes: `ToastPayload` from Task 2 (to decide whether there's a guess to record).
- Consumes: `Store::assign_project` (already exists, `crates/magpie-core/src/captures.rs:216`).
- Produces: `capture_flow::on_capture_hotkey_released(app: &AppHandle)`, called from `lib.rs`'s shortcut handler.

- [ ] **Step 1: Add pending-guess state**

In `apps/desktop/src-tauri/src/state.rs`, change:

```rust
use magpie_capture::CaptureBackend;
use magpie_core::Store;

pub struct AppState {
    pub store: Store,
    pub backend: Box<dyn CaptureBackend>,
}

impl AppState {
    pub fn new(store: Store, backend: Box<dyn CaptureBackend>) -> Self {
        Self { store, backend }
    }
}
```

to:

```rust
use std::sync::Mutex;
use std::time::Instant;

use magpie_capture::CaptureBackend;
use magpie_core::Store;

/// A capture whose destination project was guessed (not yet committed --
/// see docs/design.md "nothing is ever auto-filed") and is waiting to see
/// whether the capture hotkey is released quickly (tap = confirm) or held
/// past the tap threshold (see capture_flow.rs's TAP_THRESHOLD). Cleared
/// (taken) the moment it's resolved, so a stray Released event with no
/// matching pending guess is always a safe no-op.
pub struct PendingGuess {
    pub capture_id: i64,
    pub project_id: i64,
    pub pressed_at: Instant,
}

pub struct AppState {
    pub store: Store,
    pub backend: Box<dyn CaptureBackend>,
    pub pending_guess: Mutex<Option<PendingGuess>>,
}

impl AppState {
    pub fn new(store: Store, backend: Box<dyn CaptureBackend>) -> Self {
        Self {
            store,
            backend,
            pending_guess: Mutex::new(None),
        }
    }
}
```

- [ ] **Step 2: Record the pending guess when one is shown, and add the release handler**

In `apps/desktop/src-tauri/src/capture_flow.rs`, add near the top (after the `TOAST_VISIBLE_MS` constant):

```rust
/// How long the capture hotkey can be held before a release stops counting
/// as "tap to confirm" the guessed project. Matches the 250ms threshold
/// from the canonical capture-filing design. Holding past this is reserved
/// for a future "hold to aim" picker (not implemented by this plan) -- for
/// now it's simply not a tap, so nothing happens and the capture stays in
/// Inbox.
const TAP_THRESHOLD: Duration = Duration::from_millis(250);
```

Change the success arm of `on_capture_hotkey` to record the pending guess:

```rust
    match state.store.capture(&text, source) {
        Ok(capture) => {
            let _ = app.emit("capture:added", ());
            let payload = guess_toast_payload(&state.store, capture.id);
            record_pending_guess(&state, &payload);
            fire_toast(app, payload);
        }
        Err(e) => {
            eprintln!("magpie: capture insert failed: {e}");
            fire_plain_toast(app, "Capture failed");
        }
    }
```

Add the helper and the release handler, near `fire_plain_toast`:

```rust
fn record_pending_guess(state: &AppState, payload: &ToastPayload) {
    let mut pending = state
        .pending_guess
        .lock()
        .expect("pending_guess mutex poisoned");
    *pending = match payload {
        ToastPayload::Guess {
            capture_id,
            project_id,
            ..
        } => Some(crate::state::PendingGuess {
            capture_id: *capture_id,
            project_id: *project_id,
            pressed_at: std::time::Instant::now(),
        }),
        ToastPayload::Plain { .. } => None,
    };
}

/// Resolves the gesture `on_capture_hotkey` started: a quick release (within
/// `TAP_THRESHOLD`) commits the pending guess by calling `assign_project` --
/// "tap to save" from the canonical capture-filing design. A longer hold, or
/// no pending guess at all (nothing was captured, or it wasn't a guess),
/// leaves the capture exactly where `on_capture_hotkey` put it.
pub fn on_capture_hotkey_released(app: &AppHandle) {
    let state = app.state::<AppState>();
    let pending = state
        .pending_guess
        .lock()
        .expect("pending_guess mutex poisoned")
        .take();

    let Some(pending) = pending else {
        return;
    };
    if pending.pressed_at.elapsed() >= TAP_THRESHOLD {
        return;
    }

    if let Err(e) = state
        .store
        .assign_project(pending.capture_id, Some(pending.project_id))
    {
        eprintln!("magpie: failed to confirm guessed project: {e}");
        return;
    }
    let _ = app.emit("capture:updated", pending.capture_id);
}
```

- [ ] **Step 3: Route the Released event in `lib.rs`**

In `apps/desktop/src-tauri/src/lib.rs`, change the shortcut handler from:

```rust
                .with_handler(move |app, shortcut, event| {
                    if event.state != ShortcutState::Pressed {
                        return;
                    }
                    if *shortcut == capture_shortcut {
                        capture_flow::on_capture_hotkey(app);
                    } else if *shortcut == screenshot_shortcut {
                        capture_flow::on_screenshot_hotkey(app);
                    }
                })
```

to:

```rust
                .with_handler(move |app, shortcut, event| {
                    if *shortcut == capture_shortcut {
                        if event.state == ShortcutState::Pressed {
                            capture_flow::on_capture_hotkey(app);
                        } else if event.state == ShortcutState::Released {
                            capture_flow::on_capture_hotkey_released(app);
                        }
                    } else if *shortcut == screenshot_shortcut
                        && event.state == ShortcutState::Pressed
                    {
                        capture_flow::on_screenshot_hotkey(app);
                    }
                })
```

- [ ] **Step 4: Type-check**

Run: `cargo check -p magpie-desktop` (substitute the real package name from `apps/desktop/src-tauri/Cargo.toml` if different)
Expected: compiles cleanly. `on_capture_hotkey_released` and `PendingGuess`/`state.pending_guess` are new but every call site that needs them was updated in this task.

Note: this task's new logic (`on_capture_hotkey_released`, `record_pending_guess`) is `AppHandle`-coupled glue, consistent with `on_capture_hotkey`/`on_screenshot_hotkey` themselves, neither of which has direct unit tests today (this codebase tests the underlying `Store` logic instead — see Task 2's tests for `guess_toast_payload`). Verify this task manually, per Step 5.

- [ ] **Step 5: Manual verification**

Run: `cd apps/desktop && pnpm tauri dev`
Steps:
1. With at least one project touched (so a guess is offered — see Task 3's manual verification for how to create one), press and quickly release the capture hotkey (a normal, non-held press). Expected: the toast shows the guess as before, and the capture's `project_id` is now set. Verify via `magpie list` (CLI) or by checking the main window shows it filed under that project instead of Inbox.
2. Press and hold the capture hotkey for over 250ms before releasing. Expected: the toast still shows the guess, but the capture's `project_id` stays `NULL` (Inbox) — confirm via the same method as step 1.
3. Trigger "Nothing to capture" (press the hotkey with no fresh clipboard content) — Expected: no crash on release; the plain toast behaves exactly as before this plan.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src-tauri/src/state.rs \
        apps/desktop/src-tauri/src/capture_flow.rs \
        apps/desktop/src-tauri/src/lib.rs
git commit -m "Confirm a guessed project on a quick hotkey tap"
```

---

## Self-Review Notes

- **Spec coverage:** "certain" filing (MCP's git `detect()`) is unchanged and already correct — no task needed. "Guessed" filing (desktop ambient captures) is Tasks 1–4: recency signal → guess computation → visible toast → tap-to-confirm, with default-to-Inbox-on-ignore-or-hold satisfied by never calling `assign_project` except in the Task 4 release handler. "No signal" (no projects exist yet) is handled by `guess_toast_payload`'s `None` branch in Task 2, falling back to the pre-existing plain "Captured" toast.
- **Explicitly deferred, not silently dropped:** "Hold to aim" (redirecting the guess to a different project via a numbered picker) is out of scope for this plan — flagged in Global Constraints and in Task 4's doc comments, not left as an unstated gap.
- **Type consistency check:** `ToastPayload` (Task 2, `toast.rs`) is consumed as-is by `toast.ts` (Task 3) via matching `kind` tags (`"plain"`/`"guess"`, from `#[serde(rename_all = "snake_case")]`), and its `capture_id`/`project_id` fields are what `PendingGuess` (Task 4, `state.rs`) copies out of `record_pending_guess`. `Store::list_projects_by_recency` (Task 1) is called with exactly the signature it's defined with in both `guess_toast_payload` (Task 2) and its own tests.
