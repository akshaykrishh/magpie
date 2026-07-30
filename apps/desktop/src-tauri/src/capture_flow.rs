use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;
use crate::toast::{hide_toast, show_toast};

const TOAST_VISIBLE_MS: u64 = 1800;

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

    let text = match state.backend.read_capture_text() {
        Ok(Some(t)) => t,
        Ok(None) => {
            let message = if state.backend.secure_input_blocked() {
                "Can't copy from this app (Secure Input) — copy manually, then retry"
            } else {
                "Nothing to capture"
            };
            fire_toast(app, message);
            return;
        }
        Err(e) => {
            eprintln!("magpie: capture read failed: {e}");
            fire_toast(app, "Capture failed");
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
        Ok(_capture) => {
            let _ = app.emit("capture:added", ());
            fire_toast(app, "Captured");
        }
        Err(e) => {
            eprintln!("magpie: capture insert failed: {e}");
            fire_toast(app, "Capture failed");
        }
    }
}

fn fire_toast(app: &AppHandle, message: &str) {
    let _ = app.emit_to("toast", "toast:show", message);
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
