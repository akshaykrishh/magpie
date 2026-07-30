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

#[cfg(target_os = "macos")]
pub fn make_backend() -> Box<dyn CaptureBackend> {
    Box::new(magpie_capture::MacosBackend::new())
}

#[cfg(not(target_os = "macos"))]
pub fn make_backend() -> Box<dyn CaptureBackend> {
    Box::new(magpie_capture::ClipboardBackend::new().expect("failed to initialize clipboard"))
}
