use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

const MAIN_LABEL: &str = "main";
const DOCK_LABEL: &str = "dock";
const SETTINGS_LABEL: &str = "settings";

/// A monochrome "template" image, not the full-color app icon -- macOS
/// tints template images automatically to match the current menu bar
/// appearance (light/dark, plus the "reduce transparency"/tinted-menu-bar
/// cases a hardcoded color would get wrong). 44x44px (the @2x rendition of
/// a 22pt menu bar icon) for a crisp result on Retina displays.
const TRAY_ICON_TEMPLATE: &[u8] = include_bytes!("../icons/tray-icon-template.png");

/// A menu-bar icon that toggles the main window and the pinned dock, plus
/// Quit -- this is what makes closing either window sensible (see
/// `hide_instead_of_close`) rather than stranding the user with no way to
/// get them back.
pub fn init_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show magpie", true, None::<&str>)?;
    let toggle_dock = MenuItem::with_id(app, "toggle_dock", "Toggle Now dock", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit = PredefinedMenuItem::quit(app, Some("Quit magpie"))?;
    let menu = Menu::with_items(app, &[&show, &toggle_dock, &settings, &quit])?;

    let icon = Image::from_bytes(TRAY_ICON_TEMPLATE)?;

    TrayIconBuilder::new()
        .icon(icon)
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "toggle_dock" => toggle_dock_window(app),
            "settings" => show_settings_window(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn show_settings_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SETTINGS_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn toggle_dock_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(DOCK_LABEL) else {
        return;
    };
    let is_visible = window.is_visible().unwrap_or(false);
    if is_visible {
        let _ = window.hide();
    } else {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Closing the main window or the dock should hide it, not quit the app or
/// destroy the webview -- otherwise the tray icon (the only way back in
/// once closed) would be pointing at a dead process, or reopening the dock
/// would mean reloading it from scratch. Standard behaviour for a
/// menu-bar-resident utility.
pub fn hide_instead_of_close(window: &tauri::Window, event: &tauri::WindowEvent) {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = window.hide();
    }
}
