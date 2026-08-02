use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;
use crate::toast::{hide_toast, show_toast, GuessedDestination, ToastPayload};

const TOAST_VISIBLE_MS: u64 = 1800;

/// How long the *first* capture's toast stays up, spelling out in full
/// what just happened ("Captured — nothing was asked of you") instead of
/// the usual pill -- see `first_capture_and_bump`. Only the design's
/// canonical spec covers capture #1 explicitly ("capture #1 spells it out
/// for 3.5s ... from #6 it is the pill"); it doesn't restate 1a's
/// intermediate #2-5 behavior, so this deliberately simplifies to a single
/// threshold -- #1 gets the long form, every capture after it gets the
/// pill. If a graduated #2-5 step is wanted later, this is where it'd
/// branch on the same counter.
const FIRST_CAPTURE_VISIBLE_MS: u64 = 3500;

const TOAST_CAPTURE_COUNT_KEY: &str = "toast_capture_count";
/// Counts only captures made while the backend is in `ClipboardOnly` mode
/// -- unlike `TOAST_CAPTURE_COUNT_KEY`, this is what the redesign's
/// permission-upgrade offer (`usePermissionOffer.ts`) gates on, since the
/// two-keystroke friction it's about to offer to fix only exists in that
/// mode. Screenshots never go through `on_capture_hotkey` at all, so this
/// is bumped there only.
const CLIPBOARD_ONLY_CAPTURE_COUNT_KEY: &str = "clipboard_only_capture_count";
pub(crate) const QUIET_UNTIL_KEY: &str = "quiet_until";

/// How long the capture hotkey can be held before a release stops counting
/// as "tap to confirm" the guessed project and instead becomes "hold to
/// aim" (see aim.rs). Matches the 250ms threshold from the canonical
/// capture-filing design.
const TAP_THRESHOLD: Duration = Duration::from_millis(250);

/// The whole hotkey-to-capture loop: read whatever the active backend can
/// see right now (clipboard, or a synthesized copy on macOS with
/// Accessibility granted), insert it with whatever provenance the backend
/// can provide, and show a toast reporting the outcome. Never takes focus
/// -- see toast.rs.
///
/// Freshness (not re-capturing stale or already-seen content) is the
/// backend's job, not this function's -- see magpie_capture::FreshnessTracker.
/// It needs to live there, seeded from whatever's on the clipboard at
/// construction time, or content left over from before the app even
/// started would get reported as a fresh capture the first time the hotkey
/// is pressed. A dedup check only in this layer, comparing solely against
/// the previous capture, can't catch that.
pub fn on_capture_hotkey(app: &AppHandle) {
    let state = app.state::<AppState>();

    // Unconditionally discard whatever guess was pending before this press,
    // regardless of what this press goes on to do. Without this, an earlier
    // successful capture's still-pending guess (its `Released` hasn't fired
    // yet) would survive a *new* `Pressed` that fails to produce a capture
    // (nothing to capture, Secure Input blocked, read/insert error) and
    // could then get committed by that earlier press's eventual `Released`
    // -- confirming a project assignment for a capture the user never just
    // tapped to confirm. The success arm below sets a fresh pending guess
    // right after this, so this is a no-op on the happy path.
    *state
        .pending_guess
        .lock()
        .expect("pending_guess mutex poisoned") = None;

    let text = match state.backend.read_capture_text() {
        Ok(Some(t)) => t,
        Ok(None) => {
            let message = if state.backend.secure_input_blocked() {
                "Can't copy from this app (Secure Input) — copy manually, then retry"
            } else {
                "Nothing to capture"
            };
            fire_nothing_toast(app, message);
            return;
        }
        Err(e) => {
            eprintln!("magpie: capture read failed: {e}");
            fire_nothing_toast(app, "Capture failed");
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
            let destination = guess_destination(&state.store);
            record_pending_guess(&state, capture.id, destination.as_ref());
            // Only worth watching for a hold if there was a guess to
            // possibly aim away from in the first place -- with no
            // destination, pending_guess never became `Some` above, so a
            // later hold has nothing to arm against anyway.
            if destination.is_some() {
                arm_aim_gesture_after_threshold(app, capture.id);
            }
            if state.backend.capabilities().mode == magpie_capture::CaptureMode::ClipboardOnly {
                bump_setting_counter(&state.store, CLIPBOARD_ONLY_CAPTURE_COUNT_KEY);
            }
            let first_capture = first_capture_and_bump(&state.store);
            fire_captured_toast(app, &state.store, capture.id, destination, first_capture);
        }
        Err(e) => {
            eprintln!("magpie: capture insert failed: {e}");
            fire_nothing_toast(app, "Capture failed");
        }
    }
}

/// The screenshot counterpart to `on_capture_hotkey`: let the user select a
/// region, insert it immediately, then run OCR in the background. OCR runs
/// *after* the toast fires rather than blocking it -- Vision/Tesseract can
/// take a real fraction of a second, and the whole reason capture is a
/// toast rather than a panel is to never make the user wait on anything.
/// `capture:updated` (distinct from `capture:added`, which already fired by
/// the time OCR finishes) is what tells the UI the capture it already shows
/// just became searchable.
pub fn on_screenshot_hotkey(app: &AppHandle) {
    let state = app.state::<AppState>();

    let Some(dest_dir) = magpie_core::default_blobs_dir() else {
        eprintln!("magpie: could not determine a blobs directory for this platform");
        fire_nothing_toast(app, "Screenshot capture failed");
        return;
    };

    let shot = match state.backend.capture_screenshot_region(&dest_dir) {
        Ok(Some(shot)) => shot,
        // The user cancelled the selection -- the OS's own picker already
        // gave feedback for that, so this stays silent rather than firing
        // a second, redundant toast on top of it.
        Ok(None) => return,
        Err(e) => {
            eprintln!("magpie: screenshot capture failed: {e}");
            fire_nothing_toast(app, "Screenshot capture failed");
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

    let capture_id = match state.store.capture_screenshot(
        &shot.path.to_string_lossy(),
        &shot.mime,
        Some(shot.width as i64),
        Some(shot.height as i64),
        source,
    ) {
        Ok(capture) => {
            let _ = app.emit("capture:added", ());
            // No destination guess for screenshots -- unchanged from
            // before this redesign (see the module's doc comment on
            // `guess_destination`); this shares the Captured payload's
            // first-capture/quiet-mode handling, it just never computes a
            // destination to name.
            let first_capture = first_capture_and_bump(&state.store);
            fire_captured_toast(app, &state.store, capture.id, None, first_capture);
            capture.id
        }
        Err(e) => {
            eprintln!("magpie: screenshot capture insert failed: {e}");
            fire_nothing_toast(app, "Screenshot capture failed");
            return;
        }
    };

    let app = app.clone();
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let text = match state.backend.ocr_image(&shot.path) {
            Ok(Some(text)) => text,
            Ok(None) => return,
            Err(e) => {
                eprintln!("magpie: OCR failed: {e}");
                return;
            }
        };
        let blob = match state.store.get_blob_for_capture(capture_id) {
            Ok(Some(blob)) => blob,
            Ok(None) => {
                eprintln!("magpie: screenshot capture {capture_id} has no blob");
                return;
            }
            Err(e) => {
                eprintln!("magpie: failed to look up blob for capture {capture_id}: {e}");
                return;
            }
        };
        match state.store.set_blob_ocr_text(blob.id, &text) {
            Ok(_) => {
                let _ = app.emit("capture:updated", capture_id);
            }
            Err(e) => eprintln!("magpie: failed to save OCR text: {e}"),
        }
    });
}

fn fire_toast(app: &AppHandle, payload: ToastPayload, visible_ms: u64) {
    let _ = app.emit_to("toast", "toast:show", payload);
    show_toast(app);

    // AppKit window/panel calls must happen on the main thread -- see the
    // M0 postmortem in git history for the crash this avoids.
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(visible_ms));
        let for_main_thread = app.clone();
        let _ = app.run_on_main_thread(move || {
            hide_toast(&for_main_thread);
        });
    });
}

fn fire_nothing_toast(app: &AppHandle, message: &str) {
    fire_toast(
        app,
        ToastPayload::Nothing {
            message: message.to_string(),
        },
        TOAST_VISIBLE_MS,
    );
}

/// Fires a `Captured` toast unless quiet mode is active, in which case the
/// capture still happened (and tap-to-confirm still works -- the caller
/// already recorded the pending guess before calling this, if there was
/// one) but the ambient confirmation UI stays silent, per "Quiet for an
/// hour." Error/`Nothing` toasts never go through this -- see
/// `fire_nothing_toast`, which calls `fire_toast` directly, since a
/// failure the user needs to see shouldn't be swallowed just because quiet
/// mode is on.
fn fire_captured_toast(
    app: &AppHandle,
    store: &magpie_core::Store,
    capture_id: i64,
    destination: Option<GuessedDestination>,
    first_capture: bool,
) {
    if is_quiet_now(store) {
        return;
    }
    let visible_ms = if first_capture {
        FIRST_CAPTURE_VISIBLE_MS
    } else {
        TOAST_VISIBLE_MS
    };
    fire_toast(
        app,
        ToastPayload::Captured {
            capture_id,
            destination,
            first_capture,
        },
        visible_ms,
    );
}

/// Spawns the background timer that decides whether a press has become a
/// hold: if `TAP_THRESHOLD` after this press nothing has resolved
/// `pending_guess` yet (a quick release takes it first -- see
/// `on_capture_hotkey_released`), the hotkey is still down and `aim::arm`
/// takes over from here. A no-op if a *different* capture's pending guess
/// occupies the slot by then (this press's own guess was already taken),
/// which the capture_id match below guards against.
fn arm_aim_gesture_after_threshold(app: &AppHandle, capture_id: i64) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(TAP_THRESHOLD);
        let state = app.state::<AppState>();
        let still_held = state
            .pending_guess
            .lock()
            .expect("pending_guess mutex poisoned")
            .as_ref()
            .is_some_and(|p| p.capture_id == capture_id);
        if still_held {
            crate::aim::arm(&app, capture_id);
        }
    });
}

fn record_pending_guess(
    state: &AppState,
    capture_id: i64,
    destination: Option<&GuessedDestination>,
) {
    let mut pending = state
        .pending_guess
        .lock()
        .expect("pending_guess mutex poisoned");
    *pending = destination.map(|d| crate::state::PendingGuess {
        capture_id,
        project_id: d.project_id,
    });
}

/// Resolves the gesture `on_capture_hotkey` started, regardless of how the
/// hold-vs-tap timing played out: `aim::resolve_if_active` returns the
/// project a live hold-to-aim gesture landed on, if there was one, and its
/// `None` covers both a quick "tap to save" release and a hold with fewer
/// than two destinations to aim at -- "hold does what tap does" per the
/// redesign plan's earned-surfaces table -- in which case the original
/// guess is confirmed as-is. Either way this is the one place that calls
/// `file_capture`, so a guess is never confirmed with `assign_project`'s
/// silence on *why* the project was set. No pending guess at all (nothing
/// was captured, or it wasn't a guess) leaves the capture exactly where
/// `on_capture_hotkey` put it.
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

    let project_id =
        crate::aim::resolve_if_active(app, pending.capture_id).unwrap_or(pending.project_id);

    if let Err(e) = state
        .store
        .file_capture(pending.capture_id, project_id, "guessed")
    {
        eprintln!("magpie: failed to confirm guessed project: {e}");
        return;
    }
    let _ = app.emit("capture:updated", pending.capture_id);
}

/// The desktop app's own guess at where a capture belongs: the single most
/// recently active project, if any exist yet. This is deliberately a weak,
/// visible-as-a-guess signal (see `GuessedDestination`'s doc comment) --
/// unlike MCP's `detect()` (crates/magpie-mcp/src/project.rs), which is
/// certain because it reads the actual git remote of the session's cwd,
/// this has no comparable certainty available: an ambient hotkey/screenshot
/// capture could have come from any app. `None` when no project exists yet
/// to guess -- a real state at the earliest data tiers, not an error.
fn guess_destination(store: &magpie_core::Store) -> Option<GuessedDestination> {
    match store.list_projects_by_recency(1) {
        Ok(projects) => projects.into_iter().next().map(|p| GuessedDestination {
            project_id: p.id,
            project_name: p.name,
        }),
        Err(e) => {
            eprintln!("magpie: project recency lookup failed: {e}");
            None
        }
    }
}

/// Reads `toast_capture_count`, bumps it, and reports whether this capture
/// was the first this installation has ever made (the setting was unset) --
/// see `FIRST_CAPTURE_VISIBLE_MS`. Called once per successful capture
/// (text or screenshot).
fn first_capture_and_bump(store: &magpie_core::Store) -> bool {
    bump_setting_counter(store, TOAST_CAPTURE_COUNT_KEY) == 0
}

/// Reads an integer setting, persists it incremented by one, and returns
/// the value from *before* this bump -- shared by every settings-backed
/// counter this module keeps (`toast_capture_count`,
/// `clipboard_only_capture_count`), so each one doesn't hand-roll its own
/// parse/increment/persist.
fn bump_setting_counter(store: &magpie_core::Store, key: &str) -> u64 {
    let count = store
        .get_setting(key)
        .ok()
        .flatten()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    if let Err(e) = store.set_setting(key, &(count + 1).to_string()) {
        eprintln!("magpie: failed to persist {key}: {e}");
    }
    count
}

/// Whether "Quiet for an hour" (or whatever duration set `quiet_until`) is
/// still in effect. RFC3339 UTC timestamps sort lexicographically in
/// chronological order (see `magpie_core::now_iso`'s doc comment), so
/// comparing as strings is correct and avoids pulling a datetime-parsing
/// dependency into this crate just for this one comparison.
pub(crate) fn is_quiet_now(store: &magpie_core::Store) -> bool {
    match store.get_setting(QUIET_UNTIL_KEY) {
        Ok(Some(until)) => until.as_str() > magpie_core::now_iso().as_str(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guess_is_none_when_no_projects_exist() {
        let store = magpie_core::Store::open_in_memory().unwrap();
        store.capture("something", None).unwrap();

        assert!(guess_destination(&store).is_none());
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
        store.capture("something", None).unwrap();

        let destination = guess_destination(&store);

        match destination {
            Some(GuessedDestination {
                project_id,
                project_name,
            }) => {
                assert_eq!(project_id, b.id);
                assert_eq!(project_name, "b");
            }
            other => panic!("expected a guess, got {other:?}"),
        }
    }
}
