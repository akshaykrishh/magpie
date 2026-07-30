use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::Result;

/// Where a capture's text came from, when the backend can tell.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SourceInfo {
    pub app_name: Option<String>,
    pub bundle_id: Option<String>,
    pub window_title: Option<String>,
    pub url: Option<String>,
}

/// How this backend currently reads capturable text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMode {
    /// Reads whatever's already on the clipboard. Needs no OS permission;
    /// the user presses copy themselves, then the hotkey. This is the
    /// zero-permission default every platform ships with on first run.
    ClipboardOnly,
    /// Synthesizes a copy of the current selection before reading it --
    /// one keystroke instead of two. Needs a permission grant (e.g.
    /// Accessibility on macOS), offered only after the user has felt the
    /// two-keystroke friction. See docs/design.md "Capture and permissions".
    SynthesizedCopy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    pub mode: CaptureMode,
    /// Whether SynthesizedCopy exists on this platform at all, independent
    /// of whether it's currently active -- lets the UI decide whether to
    /// ever offer the upgrade prompt.
    pub synthesized_copy_available: bool,
    /// Whether `capture_screenshot_region` can do anything on this machine
    /// right now. Not just a platform check: on Linux this also depends on
    /// whether a portal implementing the Screenshot interface is actually
    /// running, which the UI has no other way to know in advance.
    pub screenshot_available: bool,
    /// Whether `ocr_image` can do anything on this machine right now.
    /// Screenshots can still be captured and kept without this -- OCR is
    /// what makes them searchable, not what makes them capturable.
    pub ocr_available: bool,
}

/// The result of a successful region capture: an image already written to
/// disk (under the directory `capture_screenshot_region` was asked to use)
/// plus enough metadata to populate a `blobs` row without re-opening the
/// file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScreenshotCapture {
    pub path: PathBuf,
    pub mime: String,
    pub width: u32,
    pub height: u32,
}

/// A source of capturable text, screenshots, and (where available)
/// provenance for either.
///
/// This is narrower than the trait sketched in docs/design.md: that sketch
/// also included `register_hotkey`, left out deliberately --
/// `tauri-plugin-global-shortcut` already is the cross-platform hotkey
/// abstraction (proven working in the M0 spike), so a second abstraction on
/// top of it would duplicate work for no benefit.
pub trait CaptureBackend: Send + Sync {
    fn capabilities(&self) -> Capabilities;

    /// Read whatever text is available to capture right now. Returns
    /// `Ok(None)` if there's nothing capturable (e.g. an empty clipboard),
    /// not an error -- "nothing to capture" is a normal outcome, not a
    /// failure.
    fn read_capture_text(&self) -> Result<Option<String>>;

    /// Provenance for the current capture, if this backend can determine
    /// it. `ClipboardOnly` backends generally can't tell which app the
    /// clipboard content came from and should return `Ok(None)`.
    fn front_app(&self) -> Result<Option<SourceInfo>>;

    /// Whether a `None` from `read_capture_text` means "a platform security
    /// feature is actively blocking synthetic input" rather than "nothing
    /// was selected". Default `false`; only overridden where such a feature
    /// exists (macOS's Secure Keyboard Entry, which silently drops every
    /// synthesized keystroke while active -- otherwise indistinguishable
    /// from an empty selection). Lets callers report "can't reach this app"
    /// instead of a misleading "nothing to capture" when that's not
    /// actually why it failed.
    fn secure_input_blocked(&self) -> bool {
        false
    }

    /// Let the user select a region (or window) of the screen and save it
    /// as an image under `dest_dir`. `Ok(None)` means the user cancelled
    /// the selection -- a normal outcome, not a failure, exactly like
    /// `read_capture_text`'s `Ok(None)`. Default: not supported on this
    /// backend.
    fn capture_screenshot_region(&self, dest_dir: &Path) -> Result<Option<ScreenshotCapture>> {
        let _ = dest_dir;
        Ok(None)
    }

    /// Recognize text in an image file, if this backend has a way to.
    /// `Ok(None)` covers both "OCR isn't available on this machine" and
    /// "the image contains no recognizable text" -- neither is an error.
    /// Default: not supported on this backend.
    fn ocr_image(&self, path: &Path) -> Result<Option<String>> {
        let _ = path;
        Ok(None)
    }
}
