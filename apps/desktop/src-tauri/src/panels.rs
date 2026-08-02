// Shared non-activating NSPanel plumbing, proven in the M0 focus spike on
// the toast window (see toast.rs's original doc comment) and generalized
// here so the aim panel (stage 9) reuses the exact same technique instead
// of a second copy that could drift. One panel *type* is enough for both:
// `FromWindow::from_window` is handed the real window and its label at
// conversion time, so the same `NonActivatingPanel` class can wrap two
// different windows ("toast", "aim") and each is tracked under its own
// label by tauri-nspanel's own panel store.
//
// Across (stage 10) needs the opposite -- `can_become_key_window: true`,
// since it must accept typed input -- so it gets a second, distinct panel
// type (`KeyableFloatingPanel`, below) rather than a config flag on
// `NonActivatingPanel`, whose whole contract (leaned on by the toast and
// aim panel) is that it can *never* take focus.
//
// Both types are declared in one `tauri_panel!` invocation, not two: the
// macro expands to a set of module-level `use` statements every time it's
// invoked, and two separate invocations in the same module would import
// the same names twice (a compile error) -- the macro's own multi-panel
// form exists for exactly this.

use tauri::{AppHandle, Manager};

#[cfg(target_os = "macos")]
use tauri_nspanel::{tauri_panel, ManagerExt};

#[cfg(target_os = "macos")]
tauri_panel! {
    panel!(NonActivatingPanel {
        config: {
            can_become_key_window: false,
            can_become_main_window: false,
            is_floating_panel: true
        }
    })
    panel!(KeyableFloatingPanel {
        config: {
            can_become_key_window: true,
            can_become_main_window: false,
            is_floating_panel: true
        }
    })
}

#[cfg(target_os = "macos")]
pub use tauri_nspanel::WebviewWindowExt as PanelWebviewWindowExt;

/// `NSFloatingWindowLevel` (AppKit's stable public constant, hardcoded
/// since `objc2_app_kit` exposes it only as a C constant, not a Rust one).
/// `setFloatingPanel:true` (what the `is_floating_panel` config key above
/// actually sets -- see its own doc comment) only changes panel *behavior*
/// (e.g. surviving app deactivation); it does **not** raise the window's
/// *level*, so without this, `orderFrontRegardless`/`show_and_make_key`
/// only wins ordering within this app's own windows and the panel can
/// still end up behind whatever other application is currently frontmost.
/// This is what actually makes "float over whatever app you're in" true.
const NS_FLOATING_WINDOW_LEVEL: i64 = 3;

/// Converts `label`'s already-declared window into a non-activating
/// NSPanel -- shown with `order_front_regardless` rather than
/// `make_key_window`, so it can appear over whatever app is focused
/// without ever stealing that focus. Panics on failure like the rest of
/// this crate's setup-time calls: a window that fails to convert here is
/// a build-time regression, not a runtime condition to recover from.
#[cfg(target_os = "macos")]
pub fn to_non_activating_panel(app: &AppHandle, label: &str) {
    let window = app
        .get_webview_window(label)
        .unwrap_or_else(|| panic!("{label} window must be declared in tauri.conf.json"));
    let panel = window.to_panel::<NonActivatingPanel>().unwrap_or_else(|_| {
        panic!("failed to convert {label} window into a non-activating NSPanel")
    });
    panel.set_level(NS_FLOATING_WINDOW_LEVEL);
}

#[cfg(not(target_os = "macos"))]
pub fn to_non_activating_panel(_app: &AppHandle, _label: &str) {}

/// The Across-only counterpart to `to_non_activating_panel` -- see
/// `KeyableFloatingPanel`'s doc comment above.
#[cfg(target_os = "macos")]
pub fn to_keyable_panel(app: &AppHandle, label: &str) {
    let window = app
        .get_webview_window(label)
        .unwrap_or_else(|| panic!("{label} window must be declared in tauri.conf.json"));
    let panel = window
        .to_panel::<KeyableFloatingPanel>()
        .unwrap_or_else(|_| {
            panic!("failed to convert {label} window into a keyable floating NSPanel")
        });
    panel.set_level(NS_FLOATING_WINDOW_LEVEL);
}

#[cfg(not(target_os = "macos"))]
pub fn to_keyable_panel(_app: &AppHandle, _label: &str) {}

/// Tauri's own `window.show()`/`window.hide()`, called alongside every
/// panel-level show/hide below -- discovered empirically while building
/// Across (stage 10): calling only the raw NSPanel methods
/// (`orderFrontRegardless` / `makeKeyAndOrderFront`, what `Panel::show`
/// and `show_and_make_key` do) marks the NSWindow visible and frontmost,
/// but never drives Tauri's own WKWebView-attach/compositing step, so the
/// panel ends up geometrically "shown" (`is_visible()` true, correct
/// frame) yet renders nothing at all. Tauri's `show()`/`hide()` still work
/// perfectly fine on a window that's been reclassed to NSPanel -- they
/// operate on the same underlying NSWindow -- so this just also calls
/// them rather than replacing the panel-level calls, which are still what
/// gets ordering/focus semantics right (`show()` alone would take key
/// focus, wrong for the toast/aim panels).
#[cfg(target_os = "macos")]
fn ensure_webview_shown(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.show();
    }
}

#[cfg(target_os = "macos")]
fn ensure_webview_hidden(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.hide();
    }
}

#[cfg(target_os = "macos")]
pub fn show_panel(app: &AppHandle, label: &str) {
    ensure_webview_shown(app, label);
    if let Ok(panel) = app.get_webview_panel(label) {
        panel.show();
    }
}

#[cfg(target_os = "macos")]
pub fn hide_panel(app: &AppHandle, label: &str) {
    if let Ok(panel) = app.get_webview_panel(label) {
        panel.hide();
    }
    ensure_webview_hidden(app, label);
}

/// Shows `label`'s panel *and* makes it the key window -- unlike
/// `show_panel` (`orderFrontRegardless` only, for the toast/aim panels
/// that must never take focus), this is for Across, which needs real
/// keyboard focus to accept typed input.
#[cfg(target_os = "macos")]
pub fn show_and_make_key_panel(app: &AppHandle, label: &str) {
    ensure_webview_shown(app, label);
    if let Ok(panel) = app.get_webview_panel(label) {
        panel.show_and_make_key();
    }
}

#[cfg(not(target_os = "macos"))]
pub fn show_and_make_key_panel(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

// Linux/Windows fall back to a plain window show/hide, not yet verified
// non-activating -- see the redesign plan's closing note on Linux staying
// unverified for exactly this reason.
#[cfg(not(target_os = "macos"))]
pub fn show_panel(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.show();
    }
}

#[cfg(not(target_os = "macos"))]
pub fn hide_panel(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.hide();
    }
}
