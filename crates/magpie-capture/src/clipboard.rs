use std::sync::Mutex;

use arboard::Clipboard;

use crate::backend::{CaptureBackend, CaptureMode, Capabilities, SourceInfo};
use crate::error::Result;

/// The universal, zero-permission capture path: read whatever is currently
/// on the system clipboard. The user copies (Cmd/Ctrl+C) themselves, then
/// hits the hotkey. Works identically on every OS and every Wayland
/// compositor, needs no Accessibility/Automation/portal grant, and is the
/// default every platform ships with -- see docs/design.md "Capture and
/// permissions" for why this comes before any native selection-read path,
/// not after it as a fallback.
///
/// This is deliberately *not* a background clipboard-history watcher: it
/// only reads on demand, when `read_capture_text` is called. Continuously
/// polling and recording every clipboard change would make this a generic
/// clipboard manager, which is a different, more invasive product than the
/// hotkey-gated capture tool this is meant to be.
pub struct ClipboardBackend {
    clipboard: Mutex<Clipboard>,
}

impl ClipboardBackend {
    pub fn new() -> Result<Self> {
        Ok(Self {
            clipboard: Mutex::new(Clipboard::new()?),
        })
    }
}

impl CaptureBackend for ClipboardBackend {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            mode: CaptureMode::ClipboardOnly,
            synthesized_copy_available: cfg!(target_os = "macos"),
        }
    }

    fn read_capture_text(&self) -> Result<Option<String>> {
        let mut clipboard = self.clipboard.lock().expect("clipboard mutex poisoned");
        match clipboard.get_text() {
            Ok(text) if text.is_empty() => Ok(None),
            Ok(text) => Ok(Some(text)),
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn front_app(&self) -> Result<Option<SourceInfo>> {
        // The clipboard doesn't record which app copied into it -- provenance
        // beyond "unknown" needs a platform-specific frontmost-app read,
        // which is a native backend's job (see the macOS backend in the
        // next milestone), not something a clipboard read can ever provide.
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These touch the real system clipboard, which crashes (SIGSEGV, not
    // just flakiness) when two `Clipboard` instances race on the same
    // NSPasteboard from different test threads -- reproduced while writing
    // this. Kept as one sequential test rather than several, so a plain
    // `cargo test` can't reintroduce the race by running them in parallel.
    #[test]
    fn clipboard_backend_reads_and_reports_provenance() {
        let mut clipboard = Clipboard::new().unwrap();

        clipboard.set_text("magpie test payload").unwrap();
        let backend = ClipboardBackend::new().unwrap();
        assert_eq!(
            backend.read_capture_text().unwrap().as_deref(),
            Some("magpie test payload")
        );

        clipboard.set_text("").unwrap();
        assert_eq!(backend.read_capture_text().unwrap(), None);

        // The clipboard never carries provenance -- that's a native
        // backend's job (macOS synthesize-copy, next milestone).
        assert_eq!(backend.front_app().unwrap(), None);
    }
}
