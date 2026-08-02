use std::time::Duration;

use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Listener, Manager,
};

use crate::capture_flow::{is_quiet_now, QUIET_UNTIL_KEY};
use crate::state::AppState;

const MAIN_LABEL: &str = "main";
const DOCK_LABEL: &str = "dock";
const SETTINGS_LABEL: &str = "settings";
const TRAY_ID: &str = "magpie-tray";
const DEFAULT_NOW_CAP: i64 = 7;

/// A monochrome "template" image, not the full-color app icon -- macOS
/// tints template images automatically to match the current menu bar
/// appearance (light/dark, plus the "reduce transparency"/tinted-menu-bar
/// cases a hardcoded color would get wrong). 44x44px (the @2x rendition of
/// a 22pt menu bar icon) for a crisp result on Retina displays.
const TRAY_ICON_TEMPLATE: &[u8] = include_bytes!("../icons/tray-icon-template.png");

/// How often the tray rebuilds to pick up MCP-driven activity (a session
/// connecting, `queue_take`, ...) -- an agent's process can't emit a Tauri
/// event into this one, so there's no live push for it (same tradeoff
/// documented on the session strip's poll, `App.tsx`). Menu-bar chrome is
/// low-urgency enough that this interval is longer than the strip's.
const REBUILD_POLL: Duration = Duration::from_secs(15);

/// A menu-bar icon that toggles the main window and the pinned dock, shows
/// live project/Now/session/Unfiled status, and Quit -- this is what makes
/// closing either window sensible (see `hide_instead_of_close`) rather than
/// stranding the user with no way to get them back. The status portion is
/// rebuilt (not just toggled) whenever it might be stale -- see
/// `rebuild_tray_menu`.
pub fn init_tray(app: &AppHandle) -> tauri::Result<()> {
    let icon = Image::from_bytes(TRAY_ICON_TEMPLATE)?;

    let tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(true)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "toggle_dock" => toggle_dock_window(app),
            "settings" => show_settings_window(app),
            "now" | "unfiled" => show_main_window(app),
            "across" => crate::across::toggle(app),
            "quiet_toggle" => {
                toggle_quiet(app);
                rebuild_tray_menu(app);
            }
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

    // The menu itself is built by `rebuild_tray_menu` (shared with every
    // later rebuild) rather than duplicated here for the initial state.
    rebuild_tray_menu_for(app, &tray);

    // Event-driven: fires the moment this window (or the dock) knows the
    // status changed. Doesn't cover MCP-only activity -- the poll below
    // does.
    let now_handle = app.clone();
    app.listen("now:changed", move |_| rebuild_tray_menu(&now_handle));
    let capture_handle = app.clone();
    app.listen("capture:added", move |_| rebuild_tray_menu(&capture_handle));

    let poll_handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(REBUILD_POLL);
        rebuild_tray_menu(&poll_handle);
    });

    Ok(())
}

fn tray_icon(app: &AppHandle) -> Option<TrayIcon> {
    app.tray_by_id(TRAY_ID)
}

fn rebuild_tray_menu(app: &AppHandle) {
    if let Some(tray) = tray_icon(app) {
        rebuild_tray_menu_for(app, &tray);
    }
}

/// Builds the status portion fresh from the store every time -- Tauri tray
/// menus are static once built (see the redesign plan's stage 8), so
/// "5/7 NOW", the project/branch header, Unfiled, and Sessions all go stale
/// unless the whole menu is replaced. Cheap enough (a handful of small
/// queries) to do on every event/poll tick rather than diffing.
fn rebuild_tray_menu_for(app: &AppHandle, tray: &TrayIcon) {
    let state = app.state::<AppState>();
    let store = &state.store;

    let mut items: Vec<Box<dyn tauri::menu::IsMenuItem<tauri::Wry>>> = Vec::new();

    // Project + branch header -- an explicit pin wins, otherwise the same
    // "exactly one project has a live session" derivation the main
    // window's useProjectSignal uses. Absent (no header item at all) when
    // neither resolves to a single project -- see the redesign plan's
    // "there is no default focused project".
    let overview = store.list_projects_overview().unwrap_or_default();
    let pinned_id: Option<i64> = store
        .get_setting("pinned_project_id")
        .ok()
        .flatten()
        .filter(|v| !v.is_empty())
        .and_then(|v| v.parse().ok());
    let focused = pinned_id
        .and_then(|id| overview.iter().find(|o| o.project_id == Some(id)))
        .or_else(|| {
            let mut live = overview
                .iter()
                .filter(|o| o.project_id.is_some() && o.active_session_count > 0);
            match (live.next(), live.next()) {
                (Some(only), None) => Some(only),
                _ => None,
            }
        });
    if let Some(p) = focused {
        let label = match &p.branch {
            Some(b) => format!("{} · on {b}", p.project_name),
            None => p.project_name.clone(),
        };
        if let Ok(item) = MenuItem::with_id(app, "header", label, false, None::<&str>) {
            items.push(Box::new(item));
        }
    }

    // "5/7 NOW" -- the same count/cap NowColumn shows, clickable straight
    // through to the main window rather than left purely informational.
    let now_count = store.list_now(None).map(|v| v.len() as i64).unwrap_or(0);
    let now_cap: i64 = store
        .get_setting("now_cap")
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_NOW_CAP);
    if let Ok(item) = MenuItem::with_id(
        app,
        "now",
        format!("{now_count}/{now_cap} Now"),
        true,
        None::<&str>,
    ) {
        items.push(Box::new(item));
    }

    // Unfiled -- earned: absent entirely at zero, per the redesign plan's
    // "earned, not requested" table. Opens the main window; it doesn't
    // claim to scroll to the unfiled lane (that's ⌥U, in-window only --
    // see StreamView.tsx).
    let unfiled_count = overview
        .iter()
        .find(|o| o.project_id.is_none())
        .and_then(|o| o.unfiled_count)
        .unwrap_or(0);
    if unfiled_count > 0 {
        if let Ok(item) = MenuItem::with_id(
            app,
            "unfiled",
            format!("Unfiled · {unfiled_count}"),
            true,
            None::<&str>,
        ) {
            items.push(Box::new(item));
        }
    }

    // Sessions -- earned: absent when none are live. A submenu (not a
    // single count) so every live session is enumerated by name, matching
    // the redesign plan's "discoverability is paid by enumeration".
    // Read-only for now -- revoking a specific lease from here would need
    // per-capture ids the submenu doesn't carry; the session strip and dock
    // are where that action lives.
    let live_sessions: Vec<_> = store
        .list_sessions(None)
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s.ended_at.is_none())
        .collect();
    if !live_sessions.is_empty() {
        let mut session_items: Vec<MenuItem<tauri::Wry>> = Vec::new();
        for s in &live_sessions {
            let label_ord = s
                .ordinal
                .map(|o| format!("S{o}"))
                .unwrap_or_else(|| "S?".into());
            let client = s.client.as_deref().unwrap_or("connecting");
            let label = match &s.branch {
                Some(b) => format!("{label_ord} · {client} · {b}"),
                None => format!("{label_ord} · {client}"),
            };
            if let Ok(item) = MenuItem::new(app, label, false, None::<&str>) {
                session_items.push(item);
            }
        }
        let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = session_items
            .iter()
            .map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>)
            .collect();
        if let Ok(submenu) = Submenu::with_items(
            app,
            format!("Sessions ({})", live_sessions.len()),
            true,
            &refs,
        ) {
            items.push(Box::new(submenu));
        }
    }

    // Across -- earned at ≥2 real projects, matching CommandPalette's own
    // gate (overlays/CommandPalette.tsx) and the redesign plan's
    // earned-surfaces table: below that, Across has nothing to roll up
    // that the single project row above doesn't already show.
    let project_count = overview.iter().filter(|o| o.project_id.is_some()).count();
    if project_count >= 2 {
        if let Ok(item) = MenuItem::with_id(
            app,
            "across",
            "Across all projects · ⌥⌘K",
            true,
            None::<&str>,
        ) {
            items.push(Box::new(item));
        }
    }

    let show = MenuItem::with_id(app, "show", "Show magpie", true, None::<&str>);
    let toggle_dock = MenuItem::with_id(app, "toggle_dock", "Toggle Now dock", true, None::<&str>);
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>);
    let quiet_label = if is_quiet_now(store) {
        "Stop being quiet"
    } else {
        "Quiet for an hour"
    };
    let quiet = MenuItem::with_id(app, "quiet_toggle", quiet_label, true, None::<&str>);
    let quit = PredefinedMenuItem::quit(app, Some("Quit magpie"));
    let separator = PredefinedMenuItem::separator(app);

    let (Ok(show), Ok(toggle_dock), Ok(settings), Ok(quiet), Ok(quit), Ok(separator)) =
        (show, toggle_dock, settings, quiet, quit, separator)
    else {
        eprintln!("magpie: failed to build a static tray menu item");
        return;
    };

    items.push(Box::new(separator));
    items.push(Box::new(show));
    items.push(Box::new(toggle_dock));
    items.push(Box::new(quiet));
    items.push(Box::new(settings));
    let quit_separator = PredefinedMenuItem::separator(app);
    if let Ok(quit_separator) = quit_separator {
        items.push(Box::new(quit_separator));
    }
    items.push(Box::new(quit));

    let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        items.iter().map(|i| i.as_ref()).collect();
    match Menu::with_items(app, &refs) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
        }
        Err(e) => eprintln!("magpie: failed to rebuild tray menu: {e}"),
    }
}

fn toggle_quiet(app: &AppHandle) {
    let state = app.state::<AppState>();
    let store = &state.store;
    let value = if is_quiet_now(store) {
        magpie_core::now_iso()
    } else {
        magpie_core::iso_plus_hours(1)
    };
    if let Err(e) = store.set_setting(QUIET_UNTIL_KEY, &value) {
        eprintln!("magpie: failed to set {QUIET_UNTIL_KEY}: {e}");
    }
}

/// `pub(crate)` so `commands::select_across_project` (the redesign's
/// Across ⌘⌥K picker) can bring the main window forward the same way the
/// tray's own "Show magpie" item does, rather than a second copy that
/// could drift.
pub(crate) fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// `pub(crate)` (not private) so `commands::show_settings_window` -- the
/// redesign's ⌘K → Settings entry -- can call the exact same show/focus
/// logic the tray menu already uses, rather than a second copy that could
/// drift (e.g. one remembering `unminimize()`, the other not).
pub(crate) fn show_settings_window(app: &AppHandle) {
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
