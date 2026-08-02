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
/// the manual "Check for updates" button and the background timer can't
/// both start a check while one is already in flight (`Checking`) or a
/// download is actively downloading -- the loser's `run_check` call
/// becomes a no-op instead of racing a second concurrent network request
/// (or a second concurrent download of the same version) and clobbering
/// the first one's result. Does not by itself prevent every lifecycle
/// race past this point; see `download_pending_update`'s own
/// version-match guard for the rest.
fn try_start_check(app: &AppHandle) -> bool {
    let state = app.state::<UpdaterState>();
    let mut status = state.status.lock().unwrap();
    if matches!(
        &*status,
        UpdateStatus::Checking | UpdateStatus::Downloading { .. }
    ) {
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

            // If the exact same version is already fully downloaded and
            // sitting in `pending`, don't discard those bytes and
            // re-download for no reason -- just re-affirm whichever status
            // (Ready or Skipped) already applies.
            let already_ready = {
                let state = app.state::<UpdaterState>();
                let pending = state.pending.lock().unwrap();
                matches!(
                    pending.as_ref(),
                    Some(PendingUpdate { update: u, bytes: Some(_) }) if u.version == version
                )
            };

            if already_ready {
                if skipped_version.as_deref() == Some(version.as_str()) {
                    set_status(app, UpdateStatus::Skipped { version });
                } else {
                    set_status(app, UpdateStatus::Ready { version, notes });
                }
                return;
            }

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
        let pending = state.pending.lock().unwrap();
        match pending.as_ref() {
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
            let store = &app.state::<crate::state::AppState>().store;

            // The pending update (or the skip setting) may have changed
            // while this download was in flight -- e.g. a newer check
            // superseded it, or the user clicked "Skip this version"
            // after the download had already started (it starts the
            // instant Available is set, before any click could beat it).
            // Only finalize if what's currently pending is still *this*
            // version; drop the bytes otherwise rather than overwriting a
            // different pending update's state.
            let mut pending_guard = state.pending.lock().unwrap();
            let still_current = matches!(
                pending_guard.as_ref(),
                Some(p) if p.update.version == version
            );
            if !still_current {
                return;
            }
            if let Some(pending) = pending_guard.as_mut() {
                pending.bytes = Some(bytes);
            }
            drop(pending_guard);

            let skipped_version = store.get_setting("update_skipped_version").ok().flatten();
            if skipped_version.as_deref() == Some(version.as_str()) {
                set_status(app, UpdateStatus::Skipped { version });
            } else {
                set_status(app, UpdateStatus::Ready { version, notes });
            }
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
        let status = state.status.lock().unwrap();
        matches!(
            &*status,
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
        let pending = state.pending.lock().unwrap();
        pending.as_ref().map(|p| p.update.clone())
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

    // Unsupported is a static, environment-determined fact (AppImage vs.
    // .deb) that can't change during the process's lifetime -- gate the
    // background loop from ever starting at all instead of letting it spin
    // forever re-detecting the same permanent state (see `run_check`'s own
    // `detect_unsupported` check, left in place as cheap defense-in-depth
    // for any future direct caller that skips `init`).
    if let Some(reason) = detect_unsupported(app) {
        set_status(app, UpdateStatus::Unsupported { reason });
        return;
    }

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
