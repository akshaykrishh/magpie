use std::path::Path;
use std::ptr::NonNull;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use objc2::{AnyThread, ClassType};
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString, NSWorkspace};
use objc2_application_services::{AXError, AXUIElement};
use objc2_core_foundation::{CFRetained, CFString, CFType};
use objc2_foundation::{NSArray, NSData, NSDictionary, NSString};
use objc2_vision::{
    VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel,
};

use crate::backend::{Capabilities, CaptureBackend, CaptureMode, ScreenshotCapture, SourceInfo};
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
/// Also reads the focused window's title once Accessibility is granted --
/// this is what disambiguates browser tabs, which are otherwise invisible
/// to app-level provenance: NSWorkspace only reports "Chrome" whether the
/// tab is chatgpt.com, claude.ai, or a GitHub issue, since a browser is one
/// application as far as the OS is concerned. Reading the window title needs
/// no permission beyond the Accessibility grant already required for
/// one-key capture, and no extra user action -- unlike a browser extension
/// or bookmarklet, both considered and rejected: an extension is a second
/// thing to install and maintain across two independently-versioned browser
/// stores, and a bookmarklet is still an extra click every time. A title
/// alone won't give the exact URL (ChatGPT/Claude/GitHub/Stack Overflow all
/// set informative tab titles, which covers "which page was this from" --
/// the actual ask -- without it), and getting the URL itself would need
/// either that same extension or a scary per-browser AppleScript automation
/// prompt; not worth either cost for what the title already answers.
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
            screenshot_available: true,
            ocr_available: true,
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

    fn capture_screenshot_region(&self, dest_dir: &Path) -> Result<Option<ScreenshotCapture>> {
        capture_screenshot_region(dest_dir)
    }

    fn ocr_image(&self, path: &Path) -> Result<Option<String>> {
        ocr_image(path)
    }
}

fn frontmost_app_info() -> Option<SourceInfo> {
    let workspace = NSWorkspace::sharedWorkspace();
    let app = workspace.frontmostApplication()?;
    let pid = app.processIdentifier();
    Some(SourceInfo {
        app_name: app.localizedName().map(|s| s.to_string()),
        bundle_id: app.bundleIdentifier().map(|s| s.to_string()),
        window_title: MacosBackend::is_accessibility_trusted()
            .then(|| focused_window_title(pid))
            .flatten(),
        url: None,
    })
}

/// Focused-window → title, via Accessibility. Bounded and narrow on
/// purpose: one attribute chain (app -> focused window -> title), a 1s
/// messaging timeout so an unresponsive app can't hang capture, and a
/// checked downcast (`CFRetained::downcast`, which verifies the CF type ID
/// before casting) rather than an unchecked pointer cast -- this is not
/// general AX tree traversal, and deliberately doesn't grow into it.
/// Absence at any step (no focused window, no title, app doesn't support
/// AX) degrades to `None`, never an error -- provenance is opportunistic.
fn focused_window_title(pid: libc::pid_t) -> Option<String> {
    let app_element = unsafe { AXUIElement::new_application(pid) };
    unsafe { app_element.set_messaging_timeout(1.0) };

    let window_element = copy_ax_attribute::<AXUIElement>(&app_element, "AXFocusedWindow")?;
    let title = copy_ax_attribute::<CFString>(&window_element, "AXTitle")?;
    Some(title.to_string())
}

/// `AXUIElementCopyAttributeValue`, wrapped: hands back an owned `CFType`
/// through a raw out-parameter, which this takes ownership of correctly
/// (`CFRetained::from_raw`, matching the Copy/Create-rule +1 the function
/// name promises) and then safely downcasts to `T` -- `None` if the
/// attribute is absent/unsupported or isn't actually a `T`, never a crash
/// from an unexpected type.
fn copy_ax_attribute<T: objc2_core_foundation::ConcreteType>(
    element: &AXUIElement,
    attribute: &str,
) -> Option<CFRetained<T>> {
    let attribute = CFString::from_str(attribute);
    let mut value: *const CFType = std::ptr::null();
    let err = unsafe { element.copy_attribute_value(&attribute, NonNull::from(&mut value)) };
    if err != AXError::Success {
        return None;
    }
    let value = NonNull::new(value as *mut CFType)?;
    let retained: CFRetained<CFType> = unsafe { CFRetained::from_raw(value) };
    retained.downcast::<T>().ok()
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

/// Interactive region/window capture via macOS's own `screencapture -i` --
/// the same drag-to-select UI behind Cmd+Shift+4, with window highlighting
/// and marquee selection for free. Reimplementing that selection UI would
/// duplicate a solved, Apple-maintained problem for no benefit; shelling
/// out to it is the same choice this crate already makes for reading
/// capturable text (system tools over reimplementing OS-level UI).
fn capture_screenshot_region(dest_dir: &Path) -> Result<Option<ScreenshotCapture>> {
    std::fs::create_dir_all(dest_dir)?;
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let path = dest_dir.join(format!("shot-{millis}.png"));

    let status = std::process::Command::new("screencapture")
        .arg("-i")
        .arg(&path)
        .status()?;

    // `-i` writes nothing at all when the user cancels the selection
    // (Escape) -- that, not the exit status, is the reliable cancellation
    // signal. screencapture's exit code on cancel isn't documented as part
    // of any stable contract, so this doesn't depend on it either way.
    let file_is_empty_or_missing = std::fs::metadata(&path).map(|m| m.len() == 0).unwrap_or(true);
    if file_is_empty_or_missing || !status.success() {
        let _ = std::fs::remove_file(&path);
        return Ok(None);
    }

    let (width, height) = crate::png::png_dimensions(&path).unwrap_or((0, 0));
    Ok(Some(ScreenshotCapture {
        path,
        mime: "image/png".to_string(),
        width,
        height,
    }))
}

/// Text recognition via the Vision framework -- free, built into every
/// macOS install, no model download or third-party dependency. Runs
/// synchronously: `performRequests` blocks until Vision has finished, which
/// is the right shape here since this is already called from a background
/// thread after the capture itself has completed (see capture_flow.rs).
fn ocr_image(path: &Path) -> Result<Option<String>> {
    let bytes = std::fs::read(path)?;
    let data = NSData::with_bytes(&bytes);
    let options: objc2::rc::Retained<NSDictionary<NSString, objc2::runtime::AnyObject>> =
        NSDictionary::from_slices::<NSString>(&[], &[]);

    let handler =
        VNImageRequestHandler::initWithData_options(VNImageRequestHandler::alloc(), &data, &options);

    let request = VNRecognizeTextRequest::new();
    request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
    request.setUsesLanguageCorrection(true);

    let request_ref: &VNRequest = request.as_super().as_super();
    let requests = NSArray::from_slice(&[request_ref]);

    handler
        .performRequests_error(&requests)
        .map_err(|e| Error::Ocr(e.to_string()))?;

    let Some(observations) = request.results() else {
        return Ok(None);
    };

    let mut fragments = Vec::with_capacity(observations.count());
    for observation in observations.iter() {
        let candidates = observation.topCandidates(1);
        let Some(top) = candidates.iter().next() else {
            continue;
        };
        let rect = unsafe { observation.boundingBox() };
        fragments.push(TextFragment {
            y_center: rect.origin.y + rect.size.height / 2.0,
            height: rect.size.height,
            x_center: rect.origin.x + rect.size.width / 2.0,
            text: top.string().to_string(),
        });
    }

    Ok(assemble_text(fragments))
}

/// One recognized piece of text plus enough of its `boundingBox` (Vision's
/// normalized [0,1] coordinates, origin at the image's bottom-left, per
/// Apple's docs) to know where it sits relative to everything else.
struct TextFragment {
    y_center: f64,
    height: f64,
    x_center: f64,
    text: String,
}

/// Vision emits one `VNRecognizedTextObservation` per text *region* it
/// detects, not necessarily one per visual line -- a row containing a large
/// horizontal gap (spaced-out terminal or table output, a status bar) is
/// liable to come back as several separate observations for what a reader
/// would call a single line. Joining every observation with a newline would
/// render that gap as artificial line breaks instead of a single line.
/// Grouping by vertical center within half a text-height of each other,
/// then ordering left-to-right within each such group, reconstructs actual
/// reading order instead of Vision's arbitrary per-region enumeration
/// order.
fn assemble_text(mut fragments: Vec<TextFragment>) -> Option<String> {
    if fragments.is_empty() {
        return None;
    }

    // Reading order: top-to-bottom (Vision's y grows upward, so descending
    // y is top-to-bottom), then left-to-right within whatever line a
    // fragment ends up grouped into below.
    fragments.sort_by(|a, b| {
        b.y_center
            .partial_cmp(&a.y_center)
            .unwrap()
            .then(a.x_center.partial_cmp(&b.x_center).unwrap())
    });

    let mut lines: Vec<(f64, Vec<TextFragment>)> = Vec::new();
    for fragment in fragments {
        match lines.last_mut() {
            Some((anchor_y, words)) if (*anchor_y - fragment.y_center).abs() < fragment.height / 2.0 => {
                words.push(fragment);
            }
            _ => {
                let y = fragment.y_center;
                lines.push((y, vec![fragment]));
            }
        }
    }

    let joined = lines
        .into_iter()
        .map(|(_, mut words)| {
            words.sort_by(|a, b| a.x_center.partial_cmp(&b.x_center).unwrap());
            words.into_iter().map(|w| w.text).collect::<Vec<_>>().join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n");

    Some(joined)
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

    fn fragment(y_center: f64, height: f64, x_center: f64, text: &str) -> TextFragment {
        TextFragment { y_center, height, x_center, text: text.to_string() }
    }

    #[test]
    fn assemble_text_joins_same_line_fragments_with_spaces_not_newlines() {
        // A single visual line containing a wide gap (spaced-out
        // terminal/table columns, a status bar) commonly comes back from
        // Vision as several observations at nearly the same vertical
        // center rather than one -- these should read back as one line.
        let fragments = vec![
            fragment(0.50, 0.04, 0.10, "left"),
            fragment(0.505, 0.04, 0.60, "right"),
            fragment(0.495, 0.04, 0.35, "middle"),
        ];

        assert_eq!(
            assemble_text(fragments).as_deref(),
            Some("left middle right")
        );
    }

    #[test]
    fn assemble_text_keeps_distinct_rows_on_separate_lines() {
        let fragments = vec![
            fragment(0.80, 0.05, 0.10, "first"),
            fragment(0.20, 0.05, 0.10, "third"),
            fragment(0.50, 0.05, 0.10, "second"),
        ];

        assert_eq!(
            assemble_text(fragments).as_deref(),
            Some("first\nsecond\nthird")
        );
    }

    #[test]
    fn assemble_text_of_no_fragments_is_none() {
        assert_eq!(assemble_text(Vec::new()), None);
    }

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
    fn window_title_is_populated_when_accessibility_is_trusted() {
        // Can't assert exact title content -- it's whatever's frontmost on
        // whatever machine runs this test. What's being verified is that
        // the AXUIElement chain (app -> focused window -> title) actually
        // returns *something* when Accessibility is granted, confirmed by
        // hand against real running apps (WhatsApp, Terminal) before this
        // test was written, not assumed from the code alone.
        if !MacosBackend::is_accessibility_trusted() {
            return;
        }
        let info = frontmost_app_info().expect("some app is always frontmost");
        assert!(
            info.window_title.is_some(),
            "expected a window title with Accessibility trusted, got none"
        );
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
    fn ocr_runs_against_a_real_screen_capture_without_crashing() {
        // Verified by hand (cargo run -p magpie-capture --example probe --
        // ocr) that this recognizes real, accurate text from a live
        // screen. What's asserted here automatically is narrower --
        // whatever's on screen when CI or a dev machine runs this test is
        // not something to assert exact content against -- but confirms
        // the whole Vision FFI path (NSData -> VNImageRequestHandler ->
        // VNRecognizeTextRequest -> VNRecognizedTextObservation) runs
        // end-to-end without error on every run, not just this once.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ocr-test.png");
        let status = std::process::Command::new("screencapture")
            .arg(&path)
            .status()
            .expect("screencapture is always present on macOS");
        assert!(status.success());

        let result = ocr_image(&path);
        assert!(result.is_ok(), "OCR should not error: {result:?}");
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
