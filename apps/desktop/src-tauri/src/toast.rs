// Non-activating toast window, proven in the M0 focus spike: the "toast"
// window (declared in tauri.conf.json) is converted into an NSPanel that can
// never become key/main, then shown with `orderFrontRegardless` (via
// Panel::show) rather than `makeKeyWindow` -- see tauri-nspanel's panel.rs.
// Linux/Windows fall back to a plain window show/hide, not yet verified
// non-activating (no Linux machine available to test on yet).

use tauri::{AppHandle, Manager};

pub const TOAST_LABEL: &str = "toast";

#[cfg(target_os = "macos")]
use tauri_nspanel::{tauri_panel, ManagerExt};

#[cfg(target_os = "macos")]
tauri_panel! {
    panel!(ToastPanel {
        config: {
            can_become_key_window: false,
            can_become_main_window: false,
            is_floating_panel: true
        }
    })
}

#[cfg(target_os = "macos")]
pub use tauri_nspanel::WebviewWindowExt as ToastWebviewWindowExt;

#[cfg(target_os = "macos")]
pub fn init_toast_panel(app: &AppHandle) {
    let window = app
        .get_webview_window(TOAST_LABEL)
        .expect("toast window must be declared in tauri.conf.json");
    window
        .to_panel::<ToastPanel>()
        .expect("failed to convert toast window into a non-activating NSPanel");
}

#[cfg(not(target_os = "macos"))]
pub fn init_toast_panel(_app: &AppHandle) {}

#[cfg(target_os = "macos")]
pub fn show_toast(app: &AppHandle) {
    if let Ok(panel) = app.get_webview_panel(TOAST_LABEL) {
        panel.show();
    }
}

#[cfg(target_os = "macos")]
pub fn hide_toast(app: &AppHandle) {
    if let Ok(panel) = app.get_webview_panel(TOAST_LABEL) {
        panel.hide();
    }
}

#[cfg(not(target_os = "macos"))]
pub fn show_toast(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(TOAST_LABEL) {
        let _ = window.show();
    }
}

#[cfg(not(target_os = "macos"))]
pub fn hide_toast(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(TOAST_LABEL) {
        let _ = window.hide();
    }
}
