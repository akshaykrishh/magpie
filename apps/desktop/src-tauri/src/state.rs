use std::sync::atomic::AtomicU64;
use std::sync::Mutex;

use magpie_capture::CaptureBackend;
use magpie_core::Store;

use crate::aim::AimSession;

/// A capture whose destination project was guessed (not yet committed --
/// see docs/design.md "nothing is ever auto-filed") and is waiting to see
/// whether the capture hotkey is released quickly (tap = confirm) or held
/// past the tap threshold, in which case aim.rs's hold-to-aim gesture may
/// take over and pick a different project (see capture_flow.rs's
/// TAP_THRESHOLD and `arm_aim_gesture_after_threshold`). Cleared (taken)
/// the moment it's resolved, so a stray Released event with no matching
/// pending guess is always a safe no-op.
pub struct PendingGuess {
    pub capture_id: i64,
    pub project_id: i64,
}

pub struct AppState {
    pub store: Store,
    pub backend: Box<dyn CaptureBackend>,
    pub pending_guess: Mutex<Option<PendingGuess>>,
    /// The live hold-to-aim gesture, if the capture hotkey is currently
    /// being held past `capture_flow::TAP_THRESHOLD` with the panel shown
    /// -- see aim.rs. `None` whenever no aim gesture is in flight, which is
    /// almost always.
    pub aim_session: Mutex<Option<AimSession>>,
    /// Bumped every time a new aim gesture is armed -- lets the ~10s
    /// watchdog spawned for gesture N tell whether it's still gesture N's
    /// session that's live by the time it wakes, or a stale check against a
    /// session that already resolved and was replaced by a later press.
    /// See aim.rs's `arm`.
    pub aim_generation: AtomicU64,
    /// When this app process started, RFC3339 -- the synthesized S0
    /// "session" card's window start (see sessions_view.rs). Deliberately
    /// this process's own start time, not "since you sat down" or any
    /// other human-activity heuristic magpie has no way to know.
    pub app_started_at: String,
}

impl AppState {
    pub fn new(store: Store, backend: Box<dyn CaptureBackend>) -> Self {
        Self {
            store,
            backend,
            pending_guess: Mutex::new(None),
            aim_session: Mutex::new(None),
            aim_generation: AtomicU64::new(0),
            app_started_at: magpie_core::now_iso(),
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
