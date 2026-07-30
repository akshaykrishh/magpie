use crate::error::Result;

/// Where a capture's text came from, when the backend can tell.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceInfo {
    pub app_name: Option<String>,
    pub bundle_id: Option<String>,
    pub window_title: Option<String>,
    pub url: Option<String>,
}

/// How this backend currently reads capturable text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub mode: CaptureMode,
    /// Whether SynthesizedCopy exists on this platform at all, independent
    /// of whether it's currently active -- lets the UI decide whether to
    /// ever offer the upgrade prompt.
    pub synthesized_copy_available: bool,
}

/// A source of capturable text and (where available) provenance for it.
///
/// This is narrower than the trait sketched in docs/design.md: that sketch
/// also included `register_hotkey` and `screenshot_region`. Hotkey
/// registration is deliberately left out -- `tauri-plugin-global-shortcut`
/// already is the cross-platform hotkey abstraction (proven working in the
/// M0 spike), so a second abstraction on top of it would duplicate work for
/// no benefit. `screenshot_region` is M4 scope; adding it to the trait now
/// would just be an unimplemented!() on every M1 backend. Both can be added
/// here when the milestone that needs them arrives.
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
}
