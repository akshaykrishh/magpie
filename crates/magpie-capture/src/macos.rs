use std::time::{Duration, Instant};

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString, NSWorkspace};
use objc2_foundation::NSString;

use crate::backend::{Capabilities, CaptureBackend, CaptureMode, SourceInfo};
use crate::error::{Error, Result};
use crate::freshness::FreshnessTracker;

/// kVK_ANSI_C -- the physical keycode for the "C" key, independent of
/// keyboard layout. Confirmed against Carbon's HIToolbox/Events.h; this
/// value is layout-independent so it's correct even on non-QWERTY layouts.
const KEYCODE_C: u16 = 0x08;

const COPY_POLL_TIMEOUT: Duration = Duration::from_millis(300);
const COPY_POLL_INTERVAL: Duration = Duration::from_millis(10);

// Carbon's `Boolean` is `unsigned char`, not guaranteed to be exactly the C99
// `_Bool` Rust's `bool` FFI mapping assumes -- declaring the return as `u8`
// and comparing `!= 0` avoids relying on that ABI assumption at all.
#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn IsSecureEventInputEnabled() -> u8;
}

/// Whether the frontmost app currently has Secure Keyboard Entry (or any
/// other secure-input state) active. macOS deliberately drops *all*
/// synthetic keyboard events system-wide while this is on -- it exists
/// specifically to stop keyloggers, which is exactly the category of thing
/// synthesize-copy looks like from the OS's point of view. Common in
/// terminal emulators (Terminal.app has it as a menu toggle; several others
/// enable it automatically around password prompts). When this is true, a
/// failed synthesize-copy is not "nothing was selected" -- it's "this app
/// cannot be reached this way at all" and should be reported differently.
pub fn is_secure_input_enabled() -> bool {
    unsafe { IsSecureEventInputEnabled() != 0 }
}

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
pub struct MacosBackend {
    freshness: FreshnessTracker,
}

impl MacosBackend {
    pub fn new() -> Self {
        let initial = read_pasteboard_string().ok().flatten();
        Self {
            freshness: FreshnessTracker::seeded_with(initial.as_deref()),
        }
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
        // Two independent questions, composed: (1) does the frontmost app
        // actually produce new content in response to a synthesized Cmd+C
        // (try_synthesize_copy, sentinel-verified -- definitively yes/no,
        // no ambiguity), and if not, (2) is there something on the
        // clipboard right now that the caller hasn't already been told
        // about (the freshness check, which also covers the case where the
        // user copied manually because synthesis can't reach this app --
        // e.g. terminal emulators, confirmed by hand against Terminal.app
        // and Ghostty, which never respond to a synthesized Cmd+C at all).
        let text = if Self::is_accessibility_trusted() {
            try_synthesize_copy()?.or(read_pasteboard_string()?)
        } else {
            read_pasteboard_string()?
        };
        Ok(self.freshness.check(text))
    }

    fn front_app(&self) -> Result<Option<SourceInfo>> {
        Ok(frontmost_app_info())
    }

    fn secure_input_blocked(&self) -> bool {
        is_secure_input_enabled()
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

/// Post synthetic Cmd+C and determine, unambiguously, whether it produced
/// new content -- `Ok(Some(text))` if it definitely did, `Ok(None)` if it
/// definitely didn't. Always restores whatever was on the clipboard before
/// this ran, regardless of outcome.
///
/// Uses a unique sentinel value rather than watching NSPasteboard's
/// `changeCount`. `changeCount` is a shared global counter -- anything else
/// on the system that touches the clipboard while we're polling (another
/// app, a clipboard-history tool, macOS itself) can bump it without our
/// synthesized keystroke having done anything, which would misreport
/// failure as success. Comparing pasteboard content against a value we just
/// wrote and know can't legitimately already be there is unambiguous: if
/// the content differs from our sentinel afterward, something wrote real
/// content in response to the keystroke; if it's still our sentinel, the
/// synthesized copy produced nothing, full stop.
///
/// Confirmed by hand against a real Ghostty/Terminal.app session that this
/// path does return `Ok(None)` there (not an error, not a false positive):
/// the synthetic Cmd+C posts cleanly but these terminals never write to the
/// pasteboard in response to it, unlike Cocoa text views (Notes, Arc, ...),
/// which do. Terminal emulators typically implement their own keyboard
/// handling for PTY passthrough rather than going through the standard
/// Cocoa responder chain a synthetic key equivalent relies on.
///
/// Known, accepted limitation (matches docs/design.md, not an oversight):
/// only plain text is preserved across the restore. If the clipboard held
/// something else (an image, a file reference) before this ran, it's
/// replaced with empty text rather than restored -- full multi-type
/// pasteboard preservation would mean enumerating and round-tripping every
/// pasteboard type, which the design doc explicitly calls a wart worth
/// accepting rather than solving.
fn try_synthesize_copy() -> Result<Option<String>> {
    let pasteboard = NSPasteboard::generalPasteboard();
    let previous_text = unsafe { pasteboard.stringForType(NSPasteboardTypeString) };

    let sentinel = format!(
        "\u{200B}magpie-sentinel-{}-{}\u{200B}",
        std::process::id(),
        pasteboard.changeCount()
    );
    unsafe {
        pasteboard.clearContents();
        pasteboard.setString_forType(&NSString::from_str(&sentinel), NSPasteboardTypeString);
    }

    post_cmd_c()?;

    let deadline = Instant::now() + COPY_POLL_TIMEOUT;
    let mut result = None;
    while Instant::now() < deadline {
        let current = unsafe { pasteboard.stringForType(NSPasteboardTypeString) };
        if let Some(text) = &current {
            if text.to_string() != sentinel {
                result = Some(text.to_string());
                break;
            }
        }
        std::thread::sleep(COPY_POLL_INTERVAL);
    }

    unsafe {
        pasteboard.clearContents();
        if let Some(prev) = &previous_text {
            pasteboard.setString_forType(prev, NSPasteboardTypeString);
        }
    }

    Ok(result)
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
    use serial_test::serial;

    use super::*;

    // Every test here touches shared macOS/AppKit/Carbon state, and is
    // `#[serial]` for the same reason as clipboard.rs's tests -- see the
    // comment there. This crate's default `#[serial]` lock is shared across
    // every file, so these correctly serialize against clipboard.rs's test
    // too, not just against each other.

    #[test]
    #[serial]
    fn frontmost_app_info_returns_something_plausible() {
        // Whatever app is running the test process should be frontmost (or
        // at least *an* app should be) -- this just proves the NSWorkspace
        // call path works end to end without crashing, not any specific app.
        let info = frontmost_app_info();
        assert!(info.is_some());
        assert!(info.unwrap().app_name.is_some());
    }

    #[test]
    #[serial]
    fn secure_input_check_does_not_crash() {
        // Same reasoning as the accessibility check: can't assert a
        // specific value, since it depends on whatever app happens to be
        // frontmost and its secure-input state when the test runs. The
        // point is that the FFI declaration is sound and actually links.
        let _ = is_secure_input_enabled();
    }

    #[test]
    #[serial]
    fn accessibility_trust_check_does_not_crash() {
        // Can't assert a specific value -- depends on whether this test
        // binary has been granted Accessibility, which varies by machine.
        // The point is that the FFI call itself is sound.
        let _ = MacosBackend::is_accessibility_trusted();
    }

    #[test]
    #[serial]
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

    #[test]
    #[serial]
    fn stale_pasteboard_content_is_not_reported_as_a_fresh_capture() {
        // Reproduces the exact bug found testing against Terminal.app: seed
        // the pasteboard with content *before* the backend is constructed,
        // then confirm read_capture_text doesn't hand it back as new.
        let pasteboard = NSPasteboard::generalPasteboard();
        unsafe {
            pasteboard.clearContents();
            pasteboard.setString_forType(
                &NSString::from_str("stale content predating the backend"),
                NSPasteboardTypeString,
            );
        }

        let backend = MacosBackend::new();
        let result = backend.read_capture_text().unwrap();
        assert_ne!(
            result.as_deref(),
            Some("stale content predating the backend"),
            "content already on the pasteboard at construction must never be reported as freshly captured"
        );
    }
}
