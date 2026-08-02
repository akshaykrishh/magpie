mod across;
mod aim;
mod capture_flow;
mod commands;
mod dead_pid_sweep;
mod panels;
mod purge_sweep;
mod sessions_view;
mod settings_commands;
mod state;
mod toast;
mod tray;

use std::sync::Mutex;

use tauri::Manager;
use tauri_plugin_global_shortcut::{Shortcut, ShortcutState};

// `pub(crate)` (rather than private) so `settings_commands::get_hotkey_settings`
// can fall back to these defaults when no `settings` row has been written yet.
pub(crate) const HOTKEY: &str = "CommandOrControl+Shift+M";
/// A modifier variant of the capture hotkey rather than something
/// unrelated (e.g. Cmd+Shift+S, which would shadow every other app's Save
/// As while magpie is running) -- the fourth modifier makes collision with
/// any existing app shortcut essentially impossible while staying
/// mnemonically paired with the capture hotkey it extends.
pub(crate) const SCREENSHOT_HOTKEY: &str = "CommandOrControl+Shift+Alt+M";
/// Across's chord -- fixed, not user-remappable like the capture/screenshot
/// pair (see settings_commands.rs's key allowlist, which doesn't cover
/// this one), so it never needs `HotkeyRuntime`'s rebind-tracking: it's
/// parsed once at startup and compared directly in the handler below.
const ACROSS_HOTKEY: &str = "CommandOrControl+Alt+K";

/// Tracks which `Shortcut` currently plays the "capture" and "screenshot"
/// roles, so the single process-wide `with_handler` closure below can tell
/// which logical action a firing shortcut corresponds to *after*
/// `settings_commands::set_hotkey` has rebound one of them at runtime.
///
/// This has to be mutable, shared state rather than values captured once
/// when the closure is built: the closure is registered exactly once, at
/// startup, but `set_hotkey` can swap out the underlying OS-level
/// registration at any later point. Comparing against a fixed
/// startup-time `Shortcut` would mean a rebound hotkey's events stop being
/// routed to `capture_flow` at all -- the OS would still fire the event,
/// but nothing here would recognize it as "the capture hotkey" anymore.
pub(crate) struct HotkeyRuntime {
    pub(crate) capture: Mutex<Shortcut>,
    pub(crate) screenshot: Mutex<Shortcut>,
    /// Not a `Mutex`: unlike `capture`/`screenshot`, this one is never
    /// rebound at runtime, so a plain value is enough.
    across: Shortcut,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Only reassigned on macOS (below) -- on every other target this
    // binding is never mutated, which clippy correctly flags there.
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut builder = tauri::Builder::default();

    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_nspanel::init());
    }

    // Opened here -- before the global-shortcut plugin is constructed --
    // so startup can honor any previously-saved hotkey overrides (written
    // by `set_hotkey`) instead of always registering the hardcoded
    // `HOTKEY`/`SCREENSHOT_HOTKEY` consts. A `Store` must only be opened
    // once per process, so this single instance is threaded through: read
    // from immediately below to resolve the strings to register, then
    // moved (not reopened) into `setup()` for `dead_pid_sweep`/
    // `purge_sweep`/`app.manage()`.
    let db_path = magpie_core::default_db_path()
        .expect("could not determine a data directory for this platform");
    let store = magpie_core::Store::open(&db_path)
        .unwrap_or_else(|e| panic!("failed to open database at {db_path:?}: {e}"));

    // A `get_setting` error here (as opposed to `Ok(None)`, the ordinary
    // "never overridden" case) is treated the same as unset rather than
    // panicking -- failing open to the hardcoded default is preferable to
    // refusing to start the app over what is, at worst, a stale/corrupt
    // optional override.
    let capture_hotkey = store
        .get_setting("capture_hotkey")
        .ok()
        .flatten()
        .unwrap_or_else(|| HOTKEY.to_string());
    let screenshot_hotkey = store
        .get_setting("screenshot_hotkey")
        .ok()
        .flatten()
        .unwrap_or_else(|| SCREENSHOT_HOTKEY.to_string());

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        // No .pubkey(...)/.endpoints(...) overrides -- both come from
        // tauri.conf.json's plugins.updater block above.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin({
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts([
                    capture_hotkey.as_str(),
                    screenshot_hotkey.as_str(),
                    ACROSS_HOTKEY,
                ])
                .expect("invalid hotkey spec (corrupt stored setting?)")
                .with_handler(|app, shortcut, event| {
                    let runtime = app.state::<HotkeyRuntime>();
                    let is_capture = *shortcut == *runtime.capture.lock().unwrap();
                    let is_screenshot = *shortcut == *runtime.screenshot.lock().unwrap();
                    let is_across = *shortcut == runtime.across;
                    if is_capture {
                        if event.state == ShortcutState::Pressed {
                            capture_flow::on_capture_hotkey(app);
                        } else if event.state == ShortcutState::Released {
                            capture_flow::on_capture_hotkey_released(app);
                        }
                    } else if is_screenshot && event.state == ShortcutState::Pressed {
                        capture_flow::on_screenshot_hotkey(app);
                    } else if is_across && event.state == ShortcutState::Pressed {
                        across::toggle(app);
                    }
                })
                .build()
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_stream,
            commands::list_stream_rows,
            commands::list_now,
            commands::add_typed_capture,
            commands::promote_capture,
            commands::demote_capture,
            commands::update_capture_body,
            commands::reorder_capture,
            commands::mark_capture_done,
            commands::reopen_capture,
            commands::search_captures,
            commands::merge_captures,
            commands::list_merge_sources,
            commands::assign_capture_project,
            commands::add_capture_tag,
            commands::remove_capture_tag,
            commands::list_capture_tags,
            commands::list_captures_by_tag,
            commands::list_projects,
            commands::get_or_create_project,
            commands::export_json,
            commands::export_markdown,
            commands::capture_capabilities,
            commands::open_accessibility_settings,
            commands::list_templates,
            commands::create_template,
            commands::update_template,
            commands::delete_template,
            commands::delete_capture,
            commands::restore_capture,
            commands::list_recently_deleted_captures,
            commands::delete_capture_permanently,
            commands::restore_template,
            commands::list_recently_deleted_templates,
            commands::delete_template_permanently,
            commands::create_section,
            commands::rename_section,
            commands::list_sections,
            commands::reorder_section,
            commands::delete_section,
            commands::restore_section,
            commands::assign_capture_section,
            commands::assign_template_section,
            commands::instantiate_template,
            commands::instantiate_template_into_many,
            commands::list_audit,
            commands::list_audit_enriched,
            commands::revoke_lease,
            commands::pin_capture_to_branch,
            commands::count_unfiled,
            commands::show_settings_window,
            sessions_view::list_sessions_view,
            commands::send_back_for_rework,
            commands::copy_text,
            commands::get_capture_blob,
            commands::get_blob_image_data_url,
            commands::copy_capture_image,
            commands::copy_capture_text,
            commands::copy_captures_as_checklist,
            commands::get_template_variables,
            commands::instantiate_template_with_values,
            commands::list_sessions,
            commands::list_projects_overview,
            settings_commands::get_hotkey_settings,
            settings_commands::set_hotkey,
            settings_commands::get_setting,
            settings_commands::set_setting,
            commands::select_across_project,
            commands::hide_across,
            commands::toggle_across,
        ])
        .on_window_event(|window, event| {
            if matches!(window.label(), "main" | "dock" | "settings") {
                tray::hide_instead_of_close(window, event);
            }
        })
        .setup(move |app| {
            // `store` was opened above (before the shortcut plugin was
            // built) so it could be read for startup registration -- it is
            // moved in here, not reopened, since a `Store` must only be
            // opened once per process.
            dead_pid_sweep::sweep(&store);
            purge_sweep::sweep(&store);

            // Seeds the runtime lookup the shortcut handler uses to route
            // events, with the same resolved strings that were just
            // registered with the OS above -- kept in sync with the OS
            // registration from here on only by `set_hotkey`.
            let capture_shortcut: Shortcut = capture_hotkey
                .parse()
                .expect("invalid hotkey spec (corrupt stored setting?)");
            let screenshot_shortcut: Shortcut = screenshot_hotkey
                .parse()
                .expect("invalid hotkey spec (corrupt stored setting?)");
            let across_shortcut: Shortcut = ACROSS_HOTKEY
                .parse()
                .expect("ACROSS_HOTKEY is a hardcoded valid spec");
            app.manage(HotkeyRuntime {
                capture: Mutex::new(capture_shortcut),
                screenshot: Mutex::new(screenshot_shortcut),
                across: across_shortcut,
            });

            let backend = state::make_backend();
            app.manage(state::AppState::new(store, backend));

            // Recurring purge: only touches the SQLite Store, never a
            // window/panel, so a plain background thread is safe here
            // (the AppKit main-thread rule from toast.rs's history only
            // applies to code that touches a Tauri window/panel).
            let app_handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(60 * 60 * 24));
                let state = app_handle.state::<state::AppState>();
                purge_sweep::sweep(&state.store);
            });

            // Menu-bar-resident utility: no Dock icon, no Cmd+Tab entry.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            toast::init_toast_panel(app.handle());
            aim::init_aim_panel(app.handle());
            across::init_across_panel(app.handle());
            tray::init_tray(app.handle())?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
