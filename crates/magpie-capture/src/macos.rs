use std::time::{Duration, Instant};

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString, NSWorkspace};

use crate::backend::{CaptureBackend, Capabilities, CaptureMode, SourceInfo};
use crate::error::{Error, Result};

/// kVK_ANSI_C -- the physical keycode for the "C" key, independent of
/// keyboard layout. Confirmed against Carbon's HIToolbox/Events.h; this
/// value is layout-independent so it's correct even on non-QWERTY layouts.
const KEYCODE_C: u16 = 0x08;

const COPY_POLL_TIMEOUT: Duration = Duration::from_millis(300);
const COPY_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// macOS capture backend. Reads app-name/bundle-id provenance unconditionally
/// (no permission needed) and upgrades the actual capture path to a
/// synthesized Cmd+C once Accessibility is granted -- see docs/design.md
/// "Capture and permissions" for why this is opt-in and offered, never
/// requested up front.
///
/// Deliberately does not read the focused window title (the second
/// provenance tier in docs/design.md). That needs AXUIElementCopyAttributeValue,
/// which hands back an owned CFType through a raw out-parameter -- getting
/// the retain/release semantics right needs more verification than was safe
/// to do without a way to check the result against real Apple documentation
/// beyond a docs.rs summary, and the one safe wrapper crate available
/// (axuielement) pulls in a Swift toolchain as a build dependency just for
/// that one feature. Left as a follow-up rather than risking a memory-safety
/// bug or a disproportionate build dependency for a tier the design doc
/// itself calls secondary to app-name/bundle-id.
pub struct MacosBackend;

impl MacosBackend {
    pub fn new() -> Self {
        Self
    }

    /// Checks Accessibility permission without prompting. The prompting
    /// variant (AXIsProcessTrustedWithOptions with the prompt option) is
    /// intentionally not wired up here -- per docs/design.md, that dialog
    /// should only fire when the user explicitly opts into the upgrade
    /// (task: progressive permission flow), never on first run.
    pub fn is_accessibility_trusted() -> bool {
        unsafe { objc2_application_services::AXIsProcessTrusted() }
    }
}

impl Default for MacosBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureBackend for MacosBackend {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            mode: if Self::is_accessibility_trusted() {
                CaptureMode::SynthesizedCopy
            } else {
                CaptureMode::ClipboardOnly
            },
            synthesized_copy_available: true,
        }
    }

    fn read_capture_text(&self) -> Result<Option<String>> {
        if Self::is_accessibility_trusted() {
            synthesize_copy_and_read()
        } else {
            read_pasteboard_string()
        }
    }

    fn front_app(&self) -> Result<Option<SourceInfo>> {
        Ok(frontmost_app_info())
    }
}

fn frontmost_app_info() -> Option<SourceInfo> {
    let workspace = NSWorkspace::sharedWorkspace();
    let app = workspace.frontmostApplication()?;
    Some(SourceInfo {
        app_name: app.localizedName().map(|s| s.to_string()),
        bundle_id: app.bundleIdentifier().map(|s| s.to_string()),
        window_title: None,
        url: None,
    })
}

fn read_pasteboard_string() -> Result<Option<String>> {
    let pasteboard = NSPasteboard::generalPasteboard();
    Ok(unsafe { pasteboard.stringForType(NSPasteboardTypeString) }.map(|s| s.to_string()))
}

/// Post synthetic Cmd+C, wait for the pasteboard to actually change (rather
/// than a fixed sleep -- the target app needs a variable amount of time to
/// respond), read the result, then restore whatever was there before.
///
/// Known, accepted limitation (matches docs/design.md, not an oversight):
/// only plain text is preserved across the restore. If the clipboard held
/// something else (an image, a file reference) before the synthesized copy,
/// it's replaced with empty text rather than restored -- full multi-type
/// pasteboard preservation would mean enumerating and round-tripping every
/// pasteboard type, which the design doc explicitly calls a wart worth
/// accepting rather than solving.
fn synthesize_copy_and_read() -> Result<Option<String>> {
    let pasteboard = NSPasteboard::generalPasteboard();
    let previous_change_count = pasteboard.changeCount();
    let previous_text = unsafe { pasteboard.stringForType(NSPasteboardTypeString) };

    post_cmd_c()?;

    let deadline = Instant::now() + COPY_POLL_TIMEOUT;
    let mut changed = false;
    while Instant::now() < deadline {
        if pasteboard.changeCount() != previous_change_count {
            changed = true;
            break;
        }
        std::thread::sleep(COPY_POLL_INTERVAL);
    }

    let captured = if changed {
        unsafe { pasteboard.stringForType(NSPasteboardTypeString) }.map(|s| s.to_string())
    } else {
        // Nothing was selected, or the frontmost app doesn't support copy --
        // not an error, just nothing to capture.
        None
    };

    unsafe {
        pasteboard.clearContents();
        if let Some(prev) = &previous_text {
            pasteboard.setString_forType(prev, NSPasteboardTypeString);
        }
    }

    Ok(captured)
}

fn post_cmd_c() -> Result<()> {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| Error::EventSourceCreation)?;

    let key_down = CGEvent::new_keyboard_event(source.clone(), KEYCODE_C, true)
        .map_err(|_| Error::EventCreation)?;
    key_down.set_flags(CGEventFlags::CGEventFlagCommand);
    key_down.post(CGEventTapLocation::HID);

    let key_up = CGEvent::new_keyboard_event(source, KEYCODE_C, false)
        .map_err(|_| Error::EventCreation)?;
    key_up.set_flags(CGEventFlags::CGEventFlagCommand);
    key_up.post(CGEventTapLocation::HID);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmost_app_info_returns_something_plausible() {
        // Whatever app is running the test process should be frontmost (or
        // at least *an* app should be) -- this just proves the NSWorkspace
        // call path works end to end without crashing, not any specific app.
        let info = frontmost_app_info();
        assert!(info.is_some());
        assert!(info.unwrap().app_name.is_some());
    }

    #[test]
    fn accessibility_trust_check_does_not_crash() {
        // Can't assert a specific value -- depends on whether this test
        // binary has been granted Accessibility, which varies by machine.
        // The point is that the FFI call itself is sound.
        let _ = MacosBackend::is_accessibility_trusted();
    }

    #[test]
    fn capabilities_reflect_current_trust_state() {
        let backend = MacosBackend::new();
        let caps = backend.capabilities();
        assert!(caps.synthesized_copy_available);
        let expected_mode = if MacosBackend::is_accessibility_trusted() {
            CaptureMode::SynthesizedCopy
        } else {
            CaptureMode::ClipboardOnly
        };
        assert_eq!(caps.mode, expected_mode);
    }
}
