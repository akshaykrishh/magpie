use std::sync::Mutex;

use arboard::Clipboard;

use crate::backend::{Capabilities, CaptureBackend, CaptureMode, SourceInfo};
use crate::error::Result;
use crate::freshness::FreshnessTracker;

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
///
/// `read_capture_text` only ever returns content that's new since the last
/// time it was checked (or since construction) -- see `FreshnessTracker`.
/// Without that, pressing the hotkey without having copied anything new
/// would silently re-capture whatever stale content happened to already be
/// on the clipboard.
pub struct ClipboardBackend {
    clipboard: Mutex<Clipboard>,
    freshness: FreshnessTracker,
}

impl ClipboardBackend {
    pub fn new() -> Result<Self> {
        let mut clipboard = Clipboard::new()?;
        let initial = clipboard.get_text().ok();
        Ok(Self {
            clipboard: Mutex::new(clipboard),
            freshness: FreshnessTracker::seeded_with(initial.as_deref()),
        })
    }
}

impl CaptureBackend for ClipboardBackend {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            mode: CaptureMode::ClipboardOnly,
            synthesized_copy_available: cfg!(target_os = "macos"),
            screenshot_available: false,
            ocr_available: false,
        }
    }

    fn read_capture_text(&self) -> Result<Option<String>> {
        let mut clipboard = self.clipboard.lock().expect("clipboard mutex poisoned");
        let text = match clipboard.get_text() {
            Ok(text) => Some(text),
            Err(arboard::Error::ContentNotAvailable) => None,
            Err(e) => return Err(e.into()),
        };
        Ok(self.freshness.check(text))
    }

    fn front_app(&self) -> Result<Option<SourceInfo>> {
        // The clipboard doesn't record which app copied into it -- provenance
        // beyond "unknown" needs a platform-specific frontmost-app read,
        // which is a native backend's job (see MacosBackend), not something
        // a clipboard read can ever provide.
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use super::*;

    // Every test in this crate that touches the real system clipboard or
    // any AppKit/Carbon API is `#[serial]`. Two `Clipboard` instances (or
    // AppKit calls generally) racing on the same shared OS-level state from
    // parallel test threads crashes the whole process, which is why this is
    // enforced with a crate-wide lock (see `serial_test`'s docs: its
    // default, argument-less `#[serial]` shares one implicit lock across the
    // *entire* crate, not just within one file) rather than left to each
    // test file remembering to opt in on its own -- a lock scoped to one
    // module silently stops protecting anything the moment a conflicting
    // test exists in another.
    #[test]
    #[serial]
    fn clipboard_backend_reads_reports_freshness_and_provenance() {
        let mut clipboard = Clipboard::new().unwrap();

        // Content already present *before* construction must be treated as
        // the baseline, not as a fresh capture -- this is the exact bug
        // found testing against Terminal.app: stale leftover clipboard
        // content getting reported as newly captured.
        clipboard.set_text("stale, pre-existing content").unwrap();
        let backend = ClipboardBackend::new().unwrap();
        assert_eq!(backend.read_capture_text().unwrap(), None);

        // Genuinely new content reads as a fresh capture.
        clipboard.set_text("magpie test payload").unwrap();
        assert_eq!(
            backend.read_capture_text().unwrap().as_deref(),
            Some("magpie test payload")
        );

        // Checking again without an intervening copy is not fresh again.
        assert_eq!(backend.read_capture_text().unwrap(), None);

        // An empty clipboard is nothing to capture, and doesn't disturb the
        // remembered baseline.
        clipboard.set_text("").unwrap();
        assert_eq!(backend.read_capture_text().unwrap(), None);
        clipboard.set_text("magpie test payload").unwrap();
        assert_eq!(
            backend.read_capture_text().unwrap(),
            None,
            "re-setting the same content the empty check didn't erase from memory must stay non-fresh"
        );

        // The clipboard never carries provenance -- that's a native
        // backend's job (MacosBackend).
        assert_eq!(backend.front_app().unwrap(), None);
    }
}
