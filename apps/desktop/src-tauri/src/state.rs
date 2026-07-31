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

#[cfg(target_os = "macos")]
pub fn make_backend() -> Box<dyn CaptureBackend> {
    Box::new(magpie_capture::MacosBackend::new())
}

#[cfg(target_os = "linux")]
pub fn make_backend() -> Box<dyn CaptureBackend> {
    Box::new(magpie_capture::LinuxBackend::new().expect("failed to initialize clipboard"))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn make_backend() -> Box<dyn CaptureBackend> {
    Box::new(magpie_capture::ClipboardBackend::new().expect("failed to initialize clipboard"))
}
