use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;
use crate::toast::{hide_toast, show_toast, ToastPayload};

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
        fire_plain_toast(app, "Screenshot capture failed");
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
            fire_plain_toast(app, "Screenshot capture failed");
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
            fire_plain_toast(app, "Captured");
            capture.id
        }
        Err(e) => {
            eprintln!("magpie: screenshot capture insert failed: {e}");
            fire_plain_toast(app, "Screenshot capture failed");
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
