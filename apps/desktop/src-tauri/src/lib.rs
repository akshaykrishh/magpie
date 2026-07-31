mod capture_flow;
mod commands;
mod dead_pid_sweep;
mod purge_sweep;
mod state;
mod toast;
mod tray;

use tauri::Manager;
use tauri_plugin_global_shortcut::{Shortcut, ShortcutState};

const HOTKEY: &str = "CommandOrControl+Shift+M";
/// A modifier variant of the capture hotkey rather than something
/// unrelated (e.g. Cmd+Shift+S, which would shadow every other app's Save
/// As while magpie is running) -- the fourth modifier makes collision with
/// any existing app shortcut essentially impossible while staying
/// mnemonically paired with the capture hotkey it extends.
const SCREENSHOT_HOTKEY: &str = "CommandOrControl+Shift+Alt+M";

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

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin({
            let capture_shortcut: Shortcut = HOTKEY.parse().expect("invalid hotkey spec");
            let screenshot_shortcut: Shortcut =
                SCREENSHOT_HOTKEY.parse().expect("invalid hotkey spec");
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts([HOTKEY, SCREENSHOT_HOTKEY])
                .expect("invalid hotkey spec")
                .with_handler(move |app, shortcut, event| {
                    if *shortcut == capture_shortcut {
                        if event.state == ShortcutState::Pressed {
                            capture_flow::on_capture_hotkey(app);
                        } else if event.state == ShortcutState::Released {
                            capture_flow::on_capture_hotkey_released(app);
                        }
                    } else if *shortcut == screenshot_shortcut
                        && event.state == ShortcutState::Pressed
                    {
                        capture_flow::on_screenshot_hotkey(app);
                    }
                })
                .build()
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_stream,
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
            commands::restore_template,
            commands::list_recently_deleted_templates,
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
            commands::get_capture_blob,
            commands::get_blob_image_data_url,
            commands::copy_capture_image,
            commands::copy_capture_text,
            commands::copy_captures_as_checklist,
            commands::get_template_variables,
            commands::instantiate_template_with_values,
            commands::list_sessions,
            commands::list_projects_overview,
        ])
        .on_window_event(|window, event| {
            if matches!(window.label(), "main" | "dock") {
                tray::hide_instead_of_close(window, event);
            }
        })
        .setup(|app| {
            let db_path = magpie_core::default_db_path()
                .expect("could not determine a data directory for this platform");
            let store = magpie_core::Store::open(&db_path)
                .unwrap_or_else(|e| panic!("failed to open database at {db_path:?}: {e}"));
            dead_pid_sweep::sweep(&store);
            purge_sweep::sweep(&store);

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
            tray::init_tray(app.handle())?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
