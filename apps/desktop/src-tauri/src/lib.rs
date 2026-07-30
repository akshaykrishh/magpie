mod capture_flow;
mod commands;
mod state;
mod toast;
mod tray;

use tauri::Manager;
use tauri_plugin_global_shortcut::ShortcutState;

const HOTKEY: &str = "CommandOrControl+Shift+M";

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
                        capture_flow::on_capture_hotkey(app);
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::list_stream,
            commands::list_now,
            commands::add_typed_capture,
            commands::promote_capture,
            commands::demote_capture,
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
            let backend = state::make_backend();
            app.manage(state::AppState::new(store, backend));

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
