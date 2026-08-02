# Release Pipeline — Chunk 2: In-App Update UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the whole in-app update experience — background + manual checking, auto-download, user-initiated install+relaunch, tray surfacing, and a Settings → About panel — driven entirely from Rust, against a placeholder signing pubkey (no real key needed until Chunk 3). This is Chunk 2 of four independently-mergeable chunks — see the full design doc at `/Users/akshaykrishna/.claude-work/plans/lets-do-it-lets-shiny-rossum.md` for the four-chunk sequencing and reasoning. Chunk 1 (version single source of truth) is implemented and PR'd (#2) but not required to merge first — this chunk doesn't depend on any of its files except a harmless overlap in `tauri.conf.json` (Chunk 1 deletes a `version` key; this chunk adds an unrelated `plugins` key).

**Architecture:** `tauri_plugin_updater::UpdaterExt` gives `AppHandle::updater()` (verified directly against `tauri-plugin-updater` v2.10.1 source), which returns an `Updater` whose `check().await` returns `Result<Option<Update>>`. Everything is driven by five narrow `#[tauri::command]`s in a new `updater.rs` module — no `@tauri-apps/plugin-updater` JS package, no capability changes (both verified reasons below). A single `Mutex<UpdateStatus>` plus `Mutex<Option<PendingUpdate>>` in app-managed state tracks the whole lifecycle; every transition emits an `update:status` event (frontend) and rebuilds the tray menu (Rust), mirroring `toast.rs`'s `ToastPayload` `#[serde(tag = "kind")]` convention exactly. An update is auto-downloaded the moment it's detected (not gated behind a click — the tray's own "Downloading update… 42%" item has no way to trigger a download itself), but installing (extracting + `app.restart()`) only ever happens from an explicit "Install and relaunch" click.

**Tech Stack:** Rust (`tauri-plugin-updater` v2, `tauri::async_runtime`, `tokio::time::sleep`), React/TypeScript (`apps/desktop/src`).

## Global Constraints

- **No `@tauri-apps/plugin-updater` or `@tauri-apps/plugin-process` JS package, and no `capabilities/default.json` change.** Verified: `capabilities/default.json` grants `["core:default", "opener:default"]` across all six windows (`main`, `toast`, `dock`, `settings`, `aim`, `across`), including the three always-on-top floating panels — handing those three the ability to download and install software would violate least-privilege for no benefit. Commands this chunk adds are registered via the app's own `tauri::generate_handler!` (not the plugin's), which — verified directly against `tauri-plugin-updater::Builder::build()`'s source — needs no capability entry at all; only plugin-namespaced commands (`plugin:updater:*`) would. `AppHandle::restart()` already exists on Tauri core (verified: `tauri-2.11.5/src/app.rs:588`), so `plugin-process` buys nothing.
- **The placeholder pubkey (`"REPLACE_ME_BEFORE_FIRST_RELEASE"`) is safe.** Verified directly against `tauri-plugin-updater::Builder::build()`'s source: the pubkey string is stored into managed state at plugin-setup time with zero parsing or validation. It's only ever parsed inside `verify_signature()`, called from `Update::download()` — which never runs without a real signed release to check against. Chunk 1's own equivalent claim (Tauri's `Cargo.toml` version fallback) was verified the same way; this one is too, not assumed.
- **`getVersion()` comes from `@tauri-apps/api/app` (Tauri's built-in core API), not a custom `get_app_version` command.** A verified simplification over the original design sketch: `getVersion()` is part of Tauri's `core:app` permission set, whose `default` group (which includes `allow-version`) is already granted transitively through `capabilities/default.json`'s `"core:default"` entry (verified: `tauri-2.11.5/build.rs`'s `define_default_permission_set` composes `core:default` from every core plugin's own `:default` set, including `app:default`). No new command, no new round-trip.
- **`Update: Clone`, `Updater::check(&self) -> Result<Option<Update>>`, `Update::download`/`install`/`download_and_install`, `Config { endpoints: Vec<Url>, pubkey: String, .. }`** — every Rust API used below is copied from `tauri-plugin-updater` v2.10.1's actual source (fetched and read directly, not assumed from memory or docs).
- Match existing code style: doc comments explain *why*, not *what* (see `toast.rs`, `tray.rs`, `capture_flow.rs`'s `is_quiet_now` for the house style).
- New settings persisted via `Store::set_setting` directly (not through the `set_setting` IPC command) are excluded from `SETTING_KEYS` — same treatment as the existing `capture_hotkey`/`screenshot_hotkey` exclusion.

---

### Task 1: Dependency, placeholder config, bare plugin registration

**Files:**
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/tauri.conf.json`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

**Interfaces:**
- Produces: the `tauri_plugin_updater` plugin registered and configured, consumed by every later task via `use tauri_plugin_updater::UpdaterExt;`.

- [ ] **Step 1: Add dependencies**

In `apps/desktop/src-tauri/Cargo.toml`, change:

```toml
[dependencies]
tauri = { version = "2", features = ["tray-icon", "image-png"] }
tauri-plugin-opener = "2"
tauri-plugin-clipboard-manager = "2"
serde.workspace = true
serde_json.workspace = true
magpie-core.workspace = true
magpie-capture.workspace = true
base64 = "0.22"
```

to:

```toml
[dependencies]
tauri = { version = "2", features = ["tray-icon", "image-png"] }
tauri-plugin-opener = "2"
tauri-plugin-clipboard-manager = "2"
tauri-plugin-updater = "2"
serde.workspace = true
serde_json.workspace = true
magpie-core.workspace = true
magpie-capture.workspace = true
base64 = "0.22"
# Formats Update.date (an OffsetDateTime) as RFC3339 for the frontend --
# mirrors tauri-plugin-updater's own commands.rs, which does the same
# formatting for its (unused here) JS-facing check command.
time = { version = "0.3", features = ["formatting"] }
# Just the `time` feature: the background update-check loop needs an
# async sleep inside a tauri::async_runtime::spawn task (unlike the
# purge-sweep/tray-poll threads, which are synchronous std::thread loops).
tokio = { version = "1", features = ["time"] }
```

- [ ] **Step 2: Add the placeholder updater config**

In `apps/desktop/src-tauri/tauri.conf.json`, change:

```json
    "copyright": "Copyright the magpie contributors",
    "homepage": "https://github.com/akshaykrishh/magpie"
  }
}
```

to:

```json
    "copyright": "Copyright the magpie contributors",
    "homepage": "https://github.com/akshaykrishh/magpie"
  },
  "plugins": {
    "updater": {
      "endpoints": [
        "https://github.com/akshaykrishh/magpie/releases/latest/download/latest.json"
      ],
      "pubkey": "REPLACE_ME_BEFORE_FIRST_RELEASE"
    }
  }
}
```

`REPLACE_ME_BEFORE_FIRST_RELEASE` is a deliberate, obviously-fake placeholder — JSON has no comments, so its self-evident wording is the signal; the real key is generated and swapped in as Phase 0 of the master release-pipeline plan, only actually required by Chunk 3.

- [ ] **Step 3: Register the plugin (bare, no custom logic yet)**

In `apps/desktop/src-tauri/src/lib.rs`, change:

```rust
    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
```

to:

```rust
    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        // No .pubkey(...)/.endpoints(...) overrides -- both come from
        // tauri.conf.json's plugins.updater block above.
        .plugin(tauri_plugin_updater::Builder::new().build())
```

- [ ] **Step 4: Verify**

Run: `cargo build --workspace`

Expected: succeeds (this only proves the plugin registers and the config parses — no update logic exists yet).

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/tauri.conf.json apps/desktop/src-tauri/src/lib.rs
git commit -m "Add tauri-plugin-updater with placeholder pubkey"
```

---

### Task 2: `updater.rs` — state, status enum, and the command surface

**Files:**
- Create: `apps/desktop/src-tauri/src/updater.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs` (`mod updater;`, call `updater::init`, register 5 commands)
- Modify: `apps/desktop/src-tauri/src/tray.rs` (one visibility change: `fn rebuild_tray_menu` → `pub(crate) fn rebuild_tray_menu`)
- Test: inline, in `updater.rs`

**Interfaces:**
- Consumes: `magpie_core::Store` (already used throughout this crate, e.g. `crate::state::AppState`), `magpie_core::now_iso()` / `magpie_core::iso_plus_hours(hours: i64)` (both already used in `tray.rs`'s `toggle_quiet`).
- Produces: `pub(crate) fn init(app: &AppHandle)` (called once from `lib.rs`'s `setup()`, manages `UpdaterState`). `pub enum UpdateStatus` (serde-tagged, consumed by Task 5's frontend and Task 4's tray). Five commands: `get_update_status`, `check_for_updates`, `install_update`, `skip_update_version`, `unskip_update_version` — consumed by Task 4 (tray) and Task 5 (Settings UI). `pub(crate) fn run_check(app: &AppHandle)` (async) and `pub(crate) fn check_is_due(store: &magpie_core::Store) -> bool` — consumed by Task 3's background loop.

- [ ] **Step 1: Write `check_is_due`'s test first (it will fail — the function doesn't exist yet)**

This is the one piece of genuinely trick-prone logic in this module (a throttle window's on/off boundary, and "never checked before" defaulting) — worth a real unit test using the same `Store::open_in_memory()` pattern `capture_flow.rs`'s existing tests already use, rather than only exercising it by hand through the real app.

Create `apps/desktop/src-tauri/src/updater.rs` with just this much for now:

```rust
// The in-app updater: background + manual checking, auto-download, and
// user-initiated install+relaunch -- driven entirely from Rust via
// tauri_plugin_updater::UpdaterExt. The background check has no window to
// run in, and capabilities/default.json's permission set spans all six
// windows including the three always-on-top floating panels (toast/aim/
// across), which have no business being able to download and install
// software -- see the release-pipeline design doc for the full reasoning.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use time::format_description::well_known::Rfc3339;

/// How long after launch the first background check fires.
const INITIAL_CHECK_DELAY: Duration = Duration::from_secs(60);
/// How often the background loop wakes to re-evaluate whether a check is
/// due and whether onboarding/auto-check just became true -- deliberately
/// much shorter than CHECK_INTERVAL, so completing onboarding or flipping
/// `update_auto_check` mid-session takes effect without a restart.
const POLL_INTERVAL: Duration = Duration::from_secs(5 * 60);
/// The actual throttle window between real checks.
const CHECK_INTERVAL_HOURS: i64 = 6;

type CmdResult<T> = Result<T, String>;

/// Whether a background check is due -- mirrors capture_flow.rs's
/// `is_quiet_now` exactly: RFC3339 UTC timestamps sort lexicographically
/// in chronological order (see `magpie_core::now_iso`'s doc comment), so
/// comparing as strings avoids any datetime-parsing dependency here.
/// `update_next_check_at` is a future timestamp computed and persisted by
/// `run_check` right after each real check (`iso_plus_hours(6)`), the same
/// "store the due time, not the last time" trick `quiet_until` already
/// uses -- this repo has no helper for "parse a stored past timestamp and
/// add hours to it," and doesn't need one.
pub(crate) fn check_is_due(store: &magpie_core::Store) -> bool {
    match store.get_setting("update_next_check_at") {
        Ok(Some(next)) => magpie_core::now_iso().as_str() >= next.as_str(),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn due_when_never_checked_before() {
        let store = magpie_core::Store::open_in_memory().unwrap();
        assert!(check_is_due(&store));
    }

    #[test]
    fn not_due_when_next_check_is_in_the_future() {
        let store = magpie_core::Store::open_in_memory().unwrap();
        store
            .set_setting("update_next_check_at", &magpie_core::iso_plus_hours(6))
            .unwrap();
        assert!(!check_is_due(&store));
    }

    #[test]
    fn due_when_next_check_is_in_the_past() {
        let store = magpie_core::Store::open_in_memory().unwrap();
        store
            .set_setting("update_next_check_at", &magpie_core::iso_plus_hours(-1))
            .unwrap();
        assert!(check_is_due(&store));
    }
}
```

- [ ] **Step 2: Wire the module in and run the tests — confirm they pass already (this step is a checkpoint, not a red/green pair: `check_is_due` is simple enough to write correctly in one pass, but the module must compile and be reachable first)**

In `apps/desktop/src-tauri/src/lib.rs`, change:

```rust
mod across;
mod aim;
mod capture_flow;
mod commands;
mod dead_pid_sweep;
mod panels;
mod purge_sweep;
mod sessions_view;
mod settings_commands;
mod state;
mod toast;
mod tray;
```

to:

```rust
mod across;
mod aim;
mod capture_flow;
mod commands;
mod dead_pid_sweep;
mod panels;
mod purge_sweep;
mod sessions_view;
mod settings_commands;
mod state;
mod toast;
mod tray;
mod updater;
```

Run: `cargo test --workspace -p desktop updater::`

Expected: `3 passed; 0 failed` (the three `check_is_due` tests). If any fail, the boundary logic (`>=` vs `>`, or the `_ =>` fallback) is wrong — fix `check_is_due` itself, don't adjust the tests to match broken behavior.

- [ ] **Step 3: Add the `UpdateStatus` enum, state, and helpers**

Append to `apps/desktop/src-tauri/src/updater.rs` (after `check_is_due`'s tests, i.e. insert this block right before the `#[cfg(test)]` module so the tests stay at the end of the file):

```rust
/// Every state the in-app updater can be in. Serialized to the frontend
/// (Settings -> About) and used by tray.rs to decide the "Update to X.Y.Z"
/// menu item. `#[serde(tag = "kind", rename_all = "snake_case")]` matches
/// toast.rs's `ToastPayload` convention exactly.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpdateStatus {
    Idle,
    Checking,
    UpToDate {
        checked_at: String,
    },
    /// Detected, not yet downloaded -- a background download starts the
    /// moment this status is set (see `download_pending_update`), so this
    /// is normally a brief transitional state, not one a user often sees.
    Available {
        version: String,
        notes: Option<String>,
        pub_date: Option<String>,
    },
    Downloading {
        downloaded: usize,
        total: Option<u64>,
    },
    /// Downloaded, verified, and installable right now -- this is the
    /// state whose action button says "Install and relaunch."
    Ready {
        version: String,
        notes: Option<String>,
    },
    Skipped {
        version: String,
    },
    /// The common Linux case, not an edge case: tauri-plugin-updater on
    /// Linux can only rewrite an AppImage in place, so a `.deb` install
    /// gets this permanent status instead of a tray item.
    Unsupported {
        reason: String,
    },
    Failed {
        message: String,
        checked_at: String,
    },
}

struct PendingUpdate {
    update: Update,
    /// `None` until `download_pending_update` finishes -- `install_update`
    /// refuses to run without this being populated.
    bytes: Option<Vec<u8>>,
}

pub(crate) struct UpdaterState {
    status: Mutex<UpdateStatus>,
    pending: Mutex<Option<PendingUpdate>>,
}

/// Sets the status, emits `update:status` for the frontend, and rebuilds
/// the tray menu -- the one place all three happen together, so no call
/// site can update state without also notifying both surfaces that read it.
fn set_status(app: &AppHandle, status: UpdateStatus) {
    *app.state::<UpdaterState>().status.lock().unwrap() = status.clone();
    let _ = app.emit("update:status", &status);
    crate::tray::rebuild_tray_menu(app);
}

/// Atomically checks-and-sets `Checking` under one lock acquisition, so
/// the manual "Check for updates" button and the background timer firing
/// at the same moment can't both start a check -- the loser's `run_check`
/// call becomes a no-op instead of racing a second concurrent network
/// request and clobbering the first one's result.
fn try_start_check(app: &AppHandle) -> bool {
    let state = app.state::<UpdaterState>();
    let mut status = state.status.lock().unwrap();
    if matches!(&*status, UpdateStatus::Checking) {
        return false;
    }
    *status = UpdateStatus::Checking;
    true
}

/// `tauri-plugin-updater` on Linux can only rewrite an AppImage in place --
/// `app.env().appimage` (verified against tauri-utils's `Env::default()`,
/// which sets this from the `APPIMAGE` env var AppImage's own runtime
/// exports) is `None` for a `.deb` install, a reliable and permanent signal
/// since a `.deb` never runs from an AppImage.
#[cfg(target_os = "linux")]
fn detect_unsupported(app: &AppHandle) -> Option<String> {
    if app.env().appimage.is_none() {
        Some("installed from a .deb -- update with your package manager".to_string())
    } else {
        None
    }
}

#[cfg(not(target_os = "linux"))]
fn detect_unsupported(_app: &AppHandle) -> Option<String> {
    None
}
```

- [ ] **Step 4: Add `run_check` and `download_pending_update`**

Append (still before the `#[cfg(test)]` module):

```rust
/// Shared by the manual "Check for updates" command and the background
/// timer -- the only difference between them is *when* this gets called,
/// never what it does once called.
pub(crate) async fn run_check(app: &AppHandle) {
    if !try_start_check(app) {
        return;
    }
    let _ = app.emit("update:status", &UpdateStatus::Checking);
    crate::tray::rebuild_tray_menu(app);

    if let Some(reason) = detect_unsupported(app) {
        set_status(app, UpdateStatus::Unsupported { reason });
        return;
    }

    let result = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(e) => Err(e),
    };
    let checked_at = magpie_core::now_iso();

    let state = app.state::<crate::state::AppState>();
    let store = &state.store;
    let _ = store.set_setting("update_last_checked_at", &checked_at);
    let _ = store.set_setting(
        "update_next_check_at",
        &magpie_core::iso_plus_hours(CHECK_INTERVAL_HOURS),
    );

    match result {
        Ok(Some(update)) => {
            let version = update.version.clone();
            let notes = update.body.clone();
            let pub_date = update.date.and_then(|d| d.format(&Rfc3339).ok());
            let skipped_version = store.get_setting("update_skipped_version").ok().flatten();

            *app.state::<UpdaterState>().pending.lock().unwrap() = Some(PendingUpdate {
                update: update.clone(),
                bytes: None,
            });

            if skipped_version.as_deref() == Some(version.as_str()) {
                set_status(app, UpdateStatus::Skipped { version });
            } else {
                set_status(
                    app,
                    UpdateStatus::Available {
                        version,
                        notes,
                        pub_date,
                    },
                );
                let app_for_download = app.clone();
                tauri::async_runtime::spawn(async move {
                    download_pending_update(&app_for_download).await;
                });
            }
        }
        Ok(None) => {
            *app.state::<UpdaterState>().pending.lock().unwrap() = None;
            set_status(app, UpdateStatus::UpToDate { checked_at });
        }
        Err(e) => {
            *app.state::<UpdaterState>().pending.lock().unwrap() = None;
            set_status(
                app,
                UpdateStatus::Failed {
                    message: e.to_string(),
                    checked_at,
                },
            );
        }
    }
}

/// Auto-downloads a just-detected update in the background -- not gated
/// behind a user click, since the tray's "Downloading update… 42%" item
/// has no way to trigger a download start itself. Reads the pending
/// `Update` back out of state (rather than taking one as a parameter) so
/// `unskip_update_version` can reuse this exact function.
async fn download_pending_update(app: &AppHandle) {
    let update = {
        let state = app.state::<UpdaterState>();
        match state.pending.lock().unwrap().as_ref() {
            Some(p) => p.update.clone(),
            None => return,
        }
    };
    let version = update.version.clone();

    let app_for_progress = app.clone();
    let mut downloaded_total: usize = 0;
    let result = update
        .download(
            move |chunk_length, total| {
                downloaded_total += chunk_length;
                set_status(
                    &app_for_progress,
                    UpdateStatus::Downloading {
                        downloaded: downloaded_total,
                        total,
                    },
                );
            },
            || {},
        )
        .await;

    match result {
        Ok(bytes) => {
            let notes = update.body.clone();
            let state = app.state::<UpdaterState>();
            if let Some(pending) = state.pending.lock().unwrap().as_mut() {
                pending.bytes = Some(bytes);
            }
            set_status(app, UpdateStatus::Ready { version, notes });
        }
        Err(e) => {
            // Falls back to Available, not Failed -- the update is still
            // there and installable via a retry (re-running the check,
            // manually or on the next background tick); a download hiccup
            // shouldn't read as a hard failure the way a check() network
            // error does.
            eprintln!("magpie: background download of update {version} failed: {e}");
            let notes = update.body.clone();
            let pub_date = update.date.and_then(|d| d.format(&Rfc3339).ok());
            set_status(
                app,
                UpdateStatus::Available {
                    version,
                    notes,
                    pub_date,
                },
            );
        }
    }
}
```

- [ ] **Step 5: Add the five commands and `init`**

Append (still before the `#[cfg(test)]` module):

```rust
#[tauri::command]
pub fn get_update_status(state: State<UpdaterState>) -> UpdateStatus {
    state.status.lock().unwrap().clone()
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> CmdResult<()> {
    run_check(&app).await;
    Ok(())
}

#[tauri::command]
pub fn install_update(app: AppHandle) -> CmdResult<()> {
    let (update, bytes) = {
        let state = app.state::<UpdaterState>();
        let pending = state.pending.lock().unwrap();
        match pending.as_ref() {
            Some(PendingUpdate {
                update,
                bytes: Some(bytes),
            }) => (update.clone(), bytes.clone()),
            _ => return Err("no downloaded update ready to install".to_string()),
        }
    };
    if let Err(e) = update.install(&bytes) {
        set_status(
            &app,
            UpdateStatus::Failed {
                message: e.to_string(),
                checked_at: magpie_core::now_iso(),
            },
        );
        return Err(e.to_string());
    }
    app.restart()
}

#[tauri::command]
pub fn skip_update_version(app: AppHandle, version: String) -> CmdResult<()> {
    let state = app.state::<crate::state::AppState>();
    let store = &state.store;
    store
        .set_setting("update_skipped_version", &version)
        .map_err(|e| e.to_string())?;

    let is_current_available = {
        let state = app.state::<UpdaterState>();
        matches!(
            &*state.status.lock().unwrap(),
            UpdateStatus::Available { version: v, .. } if *v == version
        )
    };
    if is_current_available {
        set_status(&app, UpdateStatus::Skipped { version });
    }
    Ok(())
}

#[tauri::command]
pub fn unskip_update_version(app: AppHandle) -> CmdResult<()> {
    let state = app.state::<crate::state::AppState>();
    let store = &state.store;
    store
        .set_setting("update_skipped_version", "")
        .map_err(|e| e.to_string())?;

    let update = {
        let state = app.state::<UpdaterState>();
        state
            .pending
            .lock()
            .unwrap()
            .as_ref()
            .map(|p| p.update.clone())
    };
    if let Some(update) = update {
        let version = update.version.clone();
        let notes = update.body.clone();
        let pub_date = update.date.and_then(|d| d.format(&Rfc3339).ok());
        set_status(
            &app,
            UpdateStatus::Available {
                version,
                notes,
                pub_date,
            },
        );
        let app_for_download = app.clone();
        tauri::async_runtime::spawn(async move {
            download_pending_update(&app_for_download).await;
        });
    }
    Ok(())
}

/// Manages `UpdaterState`. Called once from `lib.rs`'s `setup()`. Does NOT
/// start the background loop -- that's added in the next task, on top of
/// this same function.
pub(crate) fn init(app: &AppHandle) {
    app.manage(UpdaterState {
        status: Mutex::new(UpdateStatus::Idle),
        pending: Mutex::new(None),
    });
}
```

Note `install_update` ends with `app.restart()` (no semicolon) as the function's tail expression: `AppHandle::restart(&self) -> !` diverges and coerces to the function's declared `CmdResult<()>` return type, so this is valid and the function never actually returns on the success path — verified against `tauri-2.11.5/src/app.rs:588`.

- [ ] **Step 6: Wire `init` and the five commands into `lib.rs`**

In `apps/desktop/src-tauri/src/lib.rs`, change:

```rust
            settings_commands::get_hotkey_settings,
            settings_commands::set_hotkey,
            settings_commands::get_setting,
            settings_commands::set_setting,
            commands::select_across_project,
            commands::hide_across,
            commands::toggle_across,
        ])
```

to:

```rust
            settings_commands::get_hotkey_settings,
            settings_commands::set_hotkey,
            settings_commands::get_setting,
            settings_commands::set_setting,
            commands::select_across_project,
            commands::hide_across,
            commands::toggle_across,
            updater::get_update_status,
            updater::check_for_updates,
            updater::install_update,
            updater::skip_update_version,
            updater::unskip_update_version,
        ])
```

Then, in the same file, change:

```rust
            toast::init_toast_panel(app.handle());
            aim::init_aim_panel(app.handle());
            across::init_across_panel(app.handle());
            tray::init_tray(app.handle())?;

            Ok(())
```

to:

```rust
            toast::init_toast_panel(app.handle());
            aim::init_aim_panel(app.handle());
            across::init_across_panel(app.handle());
            tray::init_tray(app.handle())?;
            updater::init(app.handle());

            Ok(())
```

- [ ] **Step 7: Make `tray::rebuild_tray_menu` reachable from `updater.rs`**

In `apps/desktop/src-tauri/src/tray.rs`, change:

```rust
fn rebuild_tray_menu(app: &AppHandle) {
```

to:

```rust
pub(crate) fn rebuild_tray_menu(app: &AppHandle) {
```

- [ ] **Step 8: Verify**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace -p desktop updater::`

Expected: build and clippy both clean, `3 passed; 0 failed` for the `check_is_due` tests (unchanged from Step 2 — nothing since then touched that logic).

- [ ] **Step 9: Commit**

```bash
git add apps/desktop/src-tauri/src/updater.rs apps/desktop/src-tauri/src/lib.rs apps/desktop/src-tauri/src/tray.rs
git commit -m "Add updater.rs: status tracking, auto-download, install+relaunch"
```

---

### Task 3: Background polling loop + new settings

**Files:**
- Modify: `apps/desktop/src-tauri/src/updater.rs` (`init` gains the background loop)
- Modify: `apps/desktop/src-tauri/src/settings_commands.rs` (`SETTING_KEYS`)
- Modify: `apps/desktop/src/lib/types.ts` (`SettingKey`)

**Interfaces:**
- Produces: `"update_auto_check"` (`"on"`/`"off"`) and `"update_skipped_version"` settings, readable/writable by the frontend via the existing generic `get_setting`/`set_setting` commands — consumed by Task 5.
- Note: `"update_last_checked_at"` and `"update_next_check_at"` (written internally by Task 2's `run_check`) are deliberately **not** added here — same treatment as the existing `capture_hotkey`/`screenshot_hotkey` exclusion.

- [ ] **Step 1: Extend `init` with the background loop**

In `apps/desktop/src-tauri/src/updater.rs`, change:

```rust
/// Manages `UpdaterState`. Called once from `lib.rs`'s `setup()`. Does NOT
/// start the background loop -- that's added in the next task, on top of
/// this same function.
pub(crate) fn init(app: &AppHandle) {
    app.manage(UpdaterState {
        status: Mutex::new(UpdateStatus::Idle),
        pending: Mutex::new(None),
    });
}
```

to:

```rust
/// Manages `UpdaterState` and spawns the background check loop -- mirrors
/// tray.rs's `init_tray`, which spawns its own poll thread internally
/// rather than lib.rs orchestrating it. Uses `tauri::async_runtime::spawn`
/// (not `std::thread::spawn`, unlike the purge-sweep/tray-poll threads),
/// since `check()` is async.
pub(crate) fn init(app: &AppHandle) {
    app.manage(UpdaterState {
        status: Mutex::new(UpdateStatus::Idle),
        pending: Mutex::new(None),
    });

    let app_for_loop = app.clone();
    tauri::async_runtime::spawn(background_loop(app_for_loop));
}

/// Waits ~60s after launch, then re-checks roughly every 6 hours --
/// "roughly" because the throttle is driven by the persisted
/// `update_next_check_at` (see `check_is_due`), not a fixed in-process
/// timer, and because `onboarding_complete`/`update_auto_check` are
/// re-read on every tick rather than gated once at startup: completing
/// onboarding, or flipping the auto-check setting, mid-session takes
/// effect within one `POLL_INTERVAL`, not only after a restart.
async fn background_loop(app: AppHandle) {
    tokio::time::sleep(INITIAL_CHECK_DELAY).await;
    loop {
        let state = app.state::<crate::state::AppState>();
        let store = &state.store;
        let onboarding_complete = store
            .get_setting("onboarding_complete")
            .ok()
            .flatten()
            .as_deref()
            == Some("true");
        let auto_check_on = store
            .get_setting("update_auto_check")
            .ok()
            .flatten()
            .as_deref()
            != Some("off");
        if onboarding_complete && auto_check_on && check_is_due(store) {
            run_check(&app).await;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
```

- [ ] **Step 2: Add the two new frontend-facing settings**

In `apps/desktop/src-tauri/src/settings_commands.rs`, change:

```rust
/// Every UI preference key the frontend is allowed to read/write through
/// `get_setting`/`set_setting`. `Store::get_setting`/`set_setting` are a
/// generic KV pair over the `settings` table -- deliberately NOT exposed
/// to IPC as-is, the same way `set_hotkey` validates `kind` rather than
/// taking a raw column name. This allowlist is also the one place that
/// documents the whole preference surface: add a key here, and it exists.
///
/// `capture_hotkey`/`screenshot_hotkey` are intentionally excluded --
/// those are written only through the validated `set_hotkey` path above,
/// never through this generic one.
const SETTING_KEYS: &[&str] = &[
    "theme",
    "now_cap",
    "quiet_until",
    "toast_capture_count",
    "clipboard_only_capture_count",
    "permission_offer_dismissed",
    "onboarding_complete",
    "pinned_project_id",
];
```

to:

```rust
/// Every UI preference key the frontend is allowed to read/write through
/// `get_setting`/`set_setting`. `Store::get_setting`/`set_setting` are a
/// generic KV pair over the `settings` table -- deliberately NOT exposed
/// to IPC as-is, the same way `set_hotkey` validates `kind` rather than
/// taking a raw column name. This allowlist is also the one place that
/// documents the whole preference surface: add a key here, and it exists.
///
/// `capture_hotkey`/`screenshot_hotkey` are intentionally excluded --
/// those are written only through the validated `set_hotkey` path above,
/// never through this generic one. `update_last_checked_at` and
/// `update_next_check_at` (updater.rs) are excluded for the same reason:
/// written internally by Rust, surfaced to the frontend only via
/// `get_update_status`, never settable directly.
const SETTING_KEYS: &[&str] = &[
    "theme",
    "now_cap",
    "quiet_until",
    "toast_capture_count",
    "clipboard_only_capture_count",
    "permission_offer_dismissed",
    "onboarding_complete",
    "pinned_project_id",
    "update_auto_check",
    "update_skipped_version",
];
```

- [ ] **Step 3: Mirror the two new keys in the frontend's `SettingKey` union**

In `apps/desktop/src/lib/types.ts`, change:

```ts
// Mirrors `SETTING_KEYS` in src-tauri/src/settings_commands.rs -- the
// Rust command rejects any other key, so keep this list in sync with that
// one rather than typing `key: string` and letting a typo surface only at
// runtime.
export type SettingKey =
  | "theme"
  | "now_cap"
  | "quiet_until"
  | "toast_capture_count"
  | "clipboard_only_capture_count"
  | "permission_offer_dismissed"
  | "onboarding_complete"
  | "pinned_project_id";
```

to:

```ts
// Mirrors `SETTING_KEYS` in src-tauri/src/settings_commands.rs -- the
// Rust command rejects any other key, so keep this list in sync with that
// one rather than typing `key: string` and letting a typo surface only at
// runtime.
export type SettingKey =
  | "theme"
  | "now_cap"
  | "quiet_until"
  | "toast_capture_count"
  | "clipboard_only_capture_count"
  | "permission_offer_dismissed"
  | "onboarding_complete"
  | "pinned_project_id"
  | "update_auto_check"
  | "update_skipped_version";
```

- [ ] **Step 4: Verify**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets -- -D warnings && pnpm --dir apps/desktop exec tsc --noEmit`

Expected: all three clean. (The background loop isn't independently testable without a real Tauri app instance and a multi-hour wait — this repo has no Tauri-app test harness anywhere, verified during Chunk 1/3 planning research, so this matches existing convention rather than introducing new scope. `check_is_due`, the one piece of its logic that could regress silently, is already covered by Task 2's unit tests.)

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/updater.rs apps/desktop/src-tauri/src/settings_commands.rs apps/desktop/src/lib/types.ts
git commit -m "Add background update-check loop and its settings"
```

---

### Task 4: Tray integration

**Files:**
- Modify: `apps/desktop/src-tauri/src/tray.rs`

**Interfaces:**
- Consumes: `crate::updater::{UpdateStatus, current_status}` — see Step 1 below for why a small new accessor is added rather than reusing the `#[tauri::command]` directly.

- [ ] **Step 1: Add a non-command status accessor to `updater.rs`**

`get_update_status` (Task 2) is a `#[tauri::command]` taking `tauri::State<UpdaterState>` — callable from IPC, but awkward to call from plain Rust code like `tray.rs`'s `rebuild_tray_menu_for` (which already has `app: &AppHandle` in scope, not a `State` extractor). Add a second, non-command accessor next to it.

In `apps/desktop/src-tauri/src/updater.rs`, change:

```rust
#[tauri::command]
pub fn get_update_status(state: State<UpdaterState>) -> UpdateStatus {
    state.status.lock().unwrap().clone()
}
```

to:

```rust
#[tauri::command]
pub fn get_update_status(state: State<UpdaterState>) -> UpdateStatus {
    state.status.lock().unwrap().clone()
}

/// Same read, for call sites (tray.rs) that already have an `&AppHandle`
/// rather than a command-injected `State` -- both go through the same
/// `UpdaterState.status` lock, never a second source of truth.
pub(crate) fn current_status(app: &AppHandle) -> UpdateStatus {
    app.state::<UpdaterState>().status.lock().unwrap().clone()
}
```

- [ ] **Step 2: Add the tray menu item**

In `apps/desktop/src-tauri/src/tray.rs`, change:

```rust
fn rebuild_tray_menu_for(app: &AppHandle, tray: &TrayIcon) {
    let state = app.state::<AppState>();
    let store = &state.store;

    let mut items: Vec<Box<dyn tauri::menu::IsMenuItem<tauri::Wry>>> = Vec::new();

    // Project + branch header -- an explicit pin wins, otherwise the same
```

to:

```rust
fn rebuild_tray_menu_for(app: &AppHandle, tray: &TrayIcon) {
    let state = app.state::<AppState>();
    let store = &state.store;

    let mut items: Vec<Box<dyn tauri::menu::IsMenuItem<tauri::Wry>>> = Vec::new();

    // Earned, at the very top: absent for every state except an update
    // actually being ready or downloading -- a transient network failure
    // (Failed) has no business in a menu bar, it surfaces in Settings
    // only. Downloading is shown-but-disabled (no click target while a
    // download is in flight); Ready is the only clickable "go install"
    // entry point from the tray, and it just opens Settings -- see the
    // release-pipeline design doc's "never relaunch without the person
    // seeing what's in the update first."
    match crate::updater::current_status(app) {
        crate::updater::UpdateStatus::Ready { version, .. } => {
            if let Ok(item) = MenuItem::with_id(
                app,
                "update",
                format!("Update to {version}"),
                true,
                None::<&str>,
            ) {
                items.push(Box::new(item));
            }
        }
        crate::updater::UpdateStatus::Downloading { downloaded, total } => {
            let label = match total {
                Some(total) if total > 0 => {
                    let pct = (downloaded as f64 / total as f64 * 100.0).round() as u32;
                    format!("Downloading update… {pct}%")
                }
                _ => "Downloading update…".to_string(),
            };
            if let Ok(item) = MenuItem::with_id(app, "update", label, false, None::<&str>) {
                items.push(Box::new(item));
            }
        }
        _ => {}
    }

    // Project + branch header -- an explicit pin wins, otherwise the same
```

- [ ] **Step 3: Handle the menu click**

In `apps/desktop/src-tauri/src/tray.rs`, change:

```rust
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "toggle_dock" => toggle_dock_window(app),
            "settings" => show_settings_window(app),
            "now" | "unfiled" => show_main_window(app),
            "across" => crate::across::toggle(app),
            "quiet_toggle" => {
                toggle_quiet(app);
                rebuild_tray_menu(app);
            }
            _ => {}
        })
```

to:

```rust
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "toggle_dock" => toggle_dock_window(app),
            "settings" => show_settings_window(app),
            "now" | "unfiled" => show_main_window(app),
            "across" => crate::across::toggle(app),
            "update" => show_settings_window(app),
            "quiet_toggle" => {
                toggle_quiet(app);
                rebuild_tray_menu(app);
            }
            _ => {}
        })
```

- [ ] **Step 4: Listen for `update:status` and rebuild**

In `apps/desktop/src-tauri/src/tray.rs`, change:

```rust
    let now_handle = app.clone();
    app.listen("now:changed", move |_| rebuild_tray_menu(&now_handle));
    let capture_handle = app.clone();
    app.listen("capture:added", move |_| rebuild_tray_menu(&capture_handle));
```

to:

```rust
    let now_handle = app.clone();
    app.listen("now:changed", move |_| rebuild_tray_menu(&now_handle));
    let capture_handle = app.clone();
    app.listen("capture:added", move |_| rebuild_tray_menu(&capture_handle));
    let update_handle = app.clone();
    app.listen("update:status", move |_| rebuild_tray_menu(&update_handle));
```

- [ ] **Step 5: Verify**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets -- -D warnings`

Expected: both clean.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src-tauri/src/updater.rs apps/desktop/src-tauri/src/tray.rs
git commit -m "Surface update status in the tray menu"
```

---

### Task 5: Settings → About panel

**Files:**
- Modify: `apps/desktop/src/lib/types.ts` (`UpdateStatus` union)
- Modify: `apps/desktop/src/lib/api.ts` (5 new entries)
- Modify: `apps/desktop/src/SettingsApp.tsx` (About fieldset)
- Modify: `apps/desktop/src-tauri/tauri.conf.json` (settings window size)

**Interfaces:**
- Consumes: `UpdateStatus` (Task 2's Rust enum, mirrored here), the 5 commands from Task 2, `SettingKey` (Task 3).

- [ ] **Step 1: Add the `UpdateStatus` union to `types.ts`**

In `apps/desktop/src/lib/types.ts`, after the `SettingKey` type (the file's last export, from Task 3's edit), add:

```ts

// Mirrors updater.rs's `UpdateStatus` (`#[serde(tag = "kind", rename_all
// = "snake_case")]`) -- every field name is the Rust struct's own field
// name verbatim, since serde's `rename_all` only renames variant tags,
// not field names.
export type UpdateStatus =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "up_to_date"; checked_at: string }
  | {
      kind: "available";
      version: string;
      notes: string | null;
      pub_date: string | null;
    }
  | { kind: "downloading"; downloaded: number; total: number | null }
  | { kind: "ready"; version: string; notes: string | null }
  | { kind: "skipped"; version: string }
  | { kind: "unsupported"; reason: string }
  | { kind: "failed"; message: string; checked_at: string };
```

- [ ] **Step 2: Add the 5 commands to `api.ts`**

In `apps/desktop/src/lib/api.ts`, change:

```ts
import { invoke } from "@tauri-apps/api/core";
import type {
  AuditEntry,
  AuditEntryView,
  Blob,
  Capabilities,
  Capture,
  HotkeySettings,
  Project,
  ProjectFilter,
  ProjectOverview,
  Section,
  Session,
  SessionView,
  SettingKey,
  StreamRow,
  Tag,
  Template,
} from "./types";
```

to:

```ts
import { invoke } from "@tauri-apps/api/core";
import type {
  AuditEntry,
  AuditEntryView,
  Blob,
  Capabilities,
  Capture,
  HotkeySettings,
  Project,
  ProjectFilter,
  ProjectOverview,
  Section,
  Session,
  SessionView,
  SettingKey,
  StreamRow,
  Tag,
  Template,
  UpdateStatus,
} from "./types";
```

Then change:

```ts
  getHotkeySettings: () => invoke<HotkeySettings>("get_hotkey_settings"),

  getSetting: (key: SettingKey) => invoke<string | null>("get_setting", { key }),
  setSetting: (key: SettingKey, value: string) =>
    invoke<void>("set_setting", { key, value }),
};
```

to:

```ts
  getHotkeySettings: () => invoke<HotkeySettings>("get_hotkey_settings"),

  getSetting: (key: SettingKey) => invoke<string | null>("get_setting", { key }),
  setSetting: (key: SettingKey, value: string) =>
    invoke<void>("set_setting", { key, value }),

  getUpdateStatus: () => invoke<UpdateStatus>("get_update_status"),
  checkForUpdates: () => invoke<void>("check_for_updates"),
  installUpdate: () => invoke<void>("install_update"),
  skipUpdateVersion: (version: string) =>
    invoke<void>("skip_update_version", { version }),
  unskipUpdateVersion: () => invoke<void>("unskip_update_version"),
};
```

- [ ] **Step 3: Add the About fieldset**

In `apps/desktop/src/SettingsApp.tsx`, change the import block:

```tsx
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { api } from "./lib/api";
import { type ThemePreference, readStoredPreference, setThemePreference } from "./lib/theme";
```

to:

```tsx
import { getVersion } from "@tauri-apps/api/app";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import { api } from "./lib/api";
import { type ThemePreference, readStoredPreference, setThemePreference } from "./lib/theme";
import type { UpdateStatus } from "./lib/types";
```

Then, after the `hasModifier` function and before the `SettingsApp` function's doc comment, add two module-level helpers:

```tsx

/// One line describing the current update state -- kept as a plain
/// function (not JSX) since it's used both for the visible status text
/// and could be reused for a title/tooltip without re-deriving the switch.
function describeUpdateStatus(status: UpdateStatus): string {
  switch (status.kind) {
    case "idle":
      return "Not checked yet.";
    case "checking":
      return "Checking for updates…";
    case "up_to_date":
      return "Up to date.";
    case "available":
      return `Downloading ${status.version}…`;
    case "downloading": {
      if (status.total && status.total > 0) {
        const pct = Math.round((status.downloaded / status.total) * 100);
        return `Downloading update… ${pct}%`;
      }
      return "Downloading update…";
    }
    case "ready":
      return `${status.version} is ready to install.`;
    case "skipped":
      return `${status.version} is available (skipped).`;
    case "unsupported":
      return status.reason;
    case "failed":
      return `Update check failed: ${status.message}`;
  }
}
```

Note: `Idle` gets its own accurate label, not lumped in with `Checking` -- unlike a fleeting startup instant, `Idle` can persist indefinitely (e.g. `onboarding_complete` never gets set), and the "Check for updates" button below must still be available in that state, so the text can't claim a check is in progress when none is.

Now add the update-related state and effects. Change:

```tsx
  const [themePref, setThemePref] = useState<ThemePreference>(() => readStoredPreference());

  const loadSettings = () =>
```

to:

```tsx
  const [themePref, setThemePref] = useState<ThemePreference>(() => readStoredPreference());
  const [appVersion, setAppVersion] = useState("");
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus>({ kind: "idle" });
  const [autoCheck, setAutoCheck] = useState(true);
  const [updateBusy, setUpdateBusy] = useState(false);

  useEffect(() => {
    void getVersion().then(setAppVersion);
  }, []);

  useEffect(() => {
    void api.getUpdateStatus().then(setUpdateStatus);
    void api.getSetting("update_auto_check").then((v) => setAutoCheck(v !== "off"));

    const unlistenUpdate = listen<UpdateStatus>("update:status", (event) => {
      setUpdateStatus(event.payload);
    });
    return () => {
      unlistenUpdate.then((f) => f());
    };
  }, []);

  async function handleAutoCheckChange(checked: boolean) {
    setAutoCheck(checked);
    await api.setSetting("update_auto_check", checked ? "on" : "off");
  }

  async function handleCheckForUpdates() {
    setUpdateBusy(true);
    try {
      await api.checkForUpdates();
    } finally {
      setUpdateBusy(false);
    }
  }

  async function handleInstallUpdate() {
    setUpdateBusy(true);
    try {
      await api.installUpdate();
      // No `finally` resetting updateBusy on the success path -- a
      // successful install calls app.restart() on the Rust side, so this
      // window is about to be torn down anyway.
    } catch (e) {
      setUpdateBusy(false);
      setError(String(e));
    }
  }

  async function handleSkipUpdate(version: string) {
    await api.skipUpdateVersion(version);
  }

  async function handleUnskipUpdate() {
    await api.unskipUpdateVersion();
  }

  const loadSettings = () =>
```

Finally, add the fieldset itself. Change:

```tsx
      <label className="flex flex-col gap-1 text-body-sm text-fg">
        Screenshot hotkey
        <input
          className="rounded-xs border border-hairline bg-surface px-2 py-1 font-mono-label text-body-sm text-fg"
          value={screenshot}
          onChange={(e) => setScreenshot(e.target.value)}
          disabled={!loaded}
        />
        {!screenshotValid && loaded && (
          <span className="text-label text-danger">
            Must include at least one modifier (⌘/Ctrl/⌥/⇧).
          </span>
        )}
      </label>

      <div className="mt-auto flex items-center justify-end gap-3">
```

to:

```tsx
      <label className="flex flex-col gap-1 text-body-sm text-fg">
        Screenshot hotkey
        <input
          className="rounded-xs border border-hairline bg-surface px-2 py-1 font-mono-label text-body-sm text-fg"
          value={screenshot}
          onChange={(e) => setScreenshot(e.target.value)}
          disabled={!loaded}
        />
        {!screenshotValid && loaded && (
          <span className="text-label text-danger">
            Must include at least one modifier (⌘/Ctrl/⌥/⇧).
          </span>
        )}
      </label>

      <fieldset className="flex flex-col gap-2 text-body-sm text-fg">
        <legend className="font-mono-label text-label-sm tracking-label uppercase text-fg-faint">
          About
        </legend>
        <div className="flex items-center justify-between">
          <span>Version {appVersion}</span>
          {appVersion && (
            <button
              type="button"
              onClick={() =>
                void openUrl(
                  `https://github.com/akshaykrishh/magpie/releases/tag/v${appVersion}`,
                )
              }
              className="text-fg-muted underline hover:text-fg"
            >
              Release notes
            </button>
          )}
        </div>
        {updateStatus.kind !== "unsupported" && (
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={autoCheck}
              onChange={(e) => void handleAutoCheckChange(e.target.checked)}
            />
            Automatically check for updates
          </label>
        )}
        <div className="flex items-center justify-between gap-2">
          <span className="text-fg-muted">{describeUpdateStatus(updateStatus)}</span>
          {updateStatus.kind === "ready" && (
            <button
              type="button"
              disabled={updateBusy}
              onClick={() => void handleInstallUpdate()}
              className="rounded-xs bg-accent px-3 py-1.5 text-body-sm text-fg-on-accent disabled:cursor-not-allowed disabled:opacity-50"
            >
              Install and relaunch
            </button>
          )}
          {updateStatus.kind === "available" && (
            <button
              type="button"
              onClick={() => void handleSkipUpdate(updateStatus.version)}
              className="text-fg-muted underline hover:text-fg"
            >
              Skip this version
            </button>
          )}
          {updateStatus.kind === "skipped" && (
            <button
              type="button"
              onClick={() => void handleUnskipUpdate()}
              className="text-fg-muted underline hover:text-fg"
            >
              Show update
            </button>
          )}
          {(updateStatus.kind === "idle" ||
            updateStatus.kind === "up_to_date" ||
            updateStatus.kind === "failed") && (
            <button
              type="button"
              disabled={updateBusy}
              onClick={() => void handleCheckForUpdates()}
              className="text-fg-muted underline hover:text-fg disabled:cursor-not-allowed disabled:opacity-50"
            >
              Check for updates
            </button>
          )}
        </div>
      </fieldset>

      <div className="mt-auto flex items-center justify-end gap-3">
```

- [ ] **Step 4: Bump the settings window size**

The real current values (re-verified directly against the file, not assumed) are `width: 420, height: 320, minWidth: 360, minHeight: 280` — not the `320×280` the original design sketch assumed. The About fieldset adds roughly one more fieldset's worth of vertical space (comparable to Appearance's), so:

In `apps/desktop/src-tauri/tauri.conf.json`, change:

```json
      {
        "label": "settings",
        "title": "magpie — Settings",
        "url": "settings.html",
        "width": 420,
        "height": 320,
        "minWidth": 360,
        "minHeight": 280,
        "visible": false,
        "decorations": true,
        "resizable": true
      }
```

to:

```json
      {
        "label": "settings",
        "title": "magpie — Settings",
        "url": "settings.html",
        "width": 460,
        "height": 420,
        "minWidth": 400,
        "minHeight": 360,
        "visible": false,
        "decorations": true,
        "resizable": true
      }
```

Also make `<main>` scroll if content still overflows at the minimum size. In `apps/desktop/src/SettingsApp.tsx`, change:

```tsx
    <main className="flex h-screen flex-col gap-4 overflow-hidden bg-ground p-4">
```

to:

```tsx
    <main className="flex h-screen flex-col gap-4 overflow-y-auto bg-ground p-4">
```

- [ ] **Step 5: Verify**

Run: `pnpm --dir apps/desktop exec tsc --noEmit && cargo build --workspace`

Expected: both clean.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src/lib/types.ts apps/desktop/src/lib/api.ts apps/desktop/src/SettingsApp.tsx apps/desktop/src-tauri/tauri.conf.json
git commit -m "Add Settings -> About panel for in-app updates"
```

---

### Task 6: Chunk-level verification

**Files:** none (verification only).

- [ ] **Step 1: Full check**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && pnpm --dir apps/desktop exec tsc --noEmit`

Expected: all clean. (`cargo test --workspace` should be unaffected outside the 3 new `updater::` tests — if any *other* test's pass/fail status changed, that's a regression to investigate, not something to wave through.)

- [ ] **Step 2: Run the real app**

Run: `cd apps/desktop && pnpm tauri dev` (or a full `pnpm tauri build` + run the bundle, matching however Chunk 1's Task 5 verified a real build).

Manually confirm:
- Settings → About shows a real version number (matches `Cargo.toml`'s `[workspace.package] version`) and a working "Release notes" link.
- "Check for updates" completes and shows a result — against the placeholder endpoint this is expected to report `Up to date.` or `Update check failed: ...` (a network/parse error, since `REPLACE_ME_BEFORE_FIRST_RELEASE` doesn't resolve to a real signed release) — not expected to find a real update, just exercising the whole path end to end, matching Chunk 1's Task 5's "exercising the whole path, not expecting a real release" verification style.
- The "Automatically check for updates" checkbox persists across a Settings window close/reopen.
- No "update" tray item appears (nothing is ever `Ready`/`Downloading` against the placeholder endpoint) — confirms the tray item's absence-by-default is correct, not broken.

- [ ] **Step 3: Confirm CI is green**

Push the branch, open a PR, and confirm `lint-and-test`/`build` pass on both `ubuntu-latest` and `macos-latest` — matching Chunk 1's own definition of done. On Linux specifically, also confirm in the running app that a non-AppImage dev build reports `Unsupported` rather than crashing (the `APPIMAGE` env var won't be set in a normal `pnpm tauri dev` session, so this is exercised for free during Step 2 on Linux, not something that needs a separate AppImage build to check).

---

## Chunk Verification Summary

- `cargo test --workspace -p desktop updater::` — 3/3 passing (`check_is_due`'s throttle-boundary logic).
- `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm --dir apps/desktop exec tsc --noEmit` all clean after every task.
- Real app run: About panel shows correct version, "Check for updates" completes against the placeholder endpoint, auto-check setting persists, tray stays clean (no update item) when nothing is `Ready`/`Downloading`.
- No behavior in this chunk depends on the real signing key or any secret — matches the design doc's "built and run against a placeholder pubkey" scope for Chunk 2.
- CI green on both `ubuntu-latest` and `macos-latest`.
