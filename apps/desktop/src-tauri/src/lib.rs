// M0 focus spike: prove a global hotkey can show a toast window without
// stealing OS keyboard focus from whatever app the user is typing in.
//
// macOS: the "toast" window (declared in tauri.conf.json) is converted into
// an NSPanel that can never become key/main, then shown with `orderFrontRegardless`
// (via Panel::show) rather than `makeKeyWindow` — see tauri-nspanel's panel.rs.
// Linux/Windows: falls back to a plain window show/hide. This path is NOT yet
// verified non-activating (Linux needs layer-shell / per-DE testing later);
// it exists so the spike still compiles and runs everywhere the app targets.

use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::ShortcutState;

#[cfg(target_os = "macos")]
use tauri_nspanel::{tauri_panel, ManagerExt, WebviewWindowExt as NsPanelWebviewWindowExt};

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

const HOTKEY: &str = "CommandOrControl+Shift+M";
const TOAST_LABEL: &str = "toast";
const TOAST_VISIBLE_MS: u64 = 1800;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_nspanel::init());
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts([HOTKEY])
                .expect("invalid hotkey spec")
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        fire_toast(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();

            #[cfg(target_os = "macos")]
            {
                let window = handle
                    .get_webview_window(TOAST_LABEL)
                    .expect("toast window must be declared in tauri.conf.json");
                window
                    .to_panel::<ToastPanel>()
                    .expect("failed to convert toast window into a non-activating NSPanel");
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn fire_toast(app: &AppHandle) {
    let _ = app.emit("toast:fired", ());
    let _ = app.emit_to(TOAST_LABEL, "toast:show", "Captured");

    show_toast(app);

    // AppKit window/panel calls (hide_toast -> Panel::hide -> orderOut:) must
    // happen on the main thread. Sleeping on a background thread and hopping
    // back via run_on_main_thread keeps the delay off the event loop without
    // touching AppKit off-thread — doing that crashes with EXC_BREAKPOINT in
    // -[NSWindow _doOrderWindow:], which is what happened before this fix.
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(TOAST_VISIBLE_MS));
        let for_main_thread = app.clone();
        let _ = app.run_on_main_thread(move || {
            hide_toast(&for_main_thread);
        });
    });
}

#[cfg(target_os = "macos")]
fn show_toast(app: &AppHandle) {
    if let Ok(panel) = app.get_webview_panel(TOAST_LABEL) {
        panel.show();
    }
}

#[cfg(target_os = "macos")]
fn hide_toast(app: &AppHandle) {
    if let Ok(panel) = app.get_webview_panel(TOAST_LABEL) {
        panel.hide();
    }
}

#[cfg(not(target_os = "macos"))]
fn show_toast(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(TOAST_LABEL) {
        let _ = window.show();
    }
}

#[cfg(not(target_os = "macos"))]
fn hide_toast(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(TOAST_LABEL) {
        let _ = window.hide();
    }
}
