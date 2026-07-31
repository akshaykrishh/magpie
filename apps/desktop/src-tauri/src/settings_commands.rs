use tauri::State;

use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

/// Reads the two global hotkeys for display in the Settings window,
/// falling back to the hardcoded defaults (`crate::HOTKEY` /
/// `crate::SCREENSHOT_HOTKEY`) when no `settings` row exists yet -- the
/// common case before Task 28 ever writes one. Rebinding itself is Task
/// 28's job; this command only reads.
#[tauri::command]
pub fn get_hotkey_settings(state: State<AppState>) -> CmdResult<serde_json::Value> {
    let capture = state
        .store
        .get_setting("capture_hotkey")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| crate::HOTKEY.to_string());
    let screenshot = state
        .store
        .get_setting("screenshot_hotkey")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| crate::SCREENSHOT_HOTKEY.to_string());
    Ok(serde_json::json!({ "capture": capture, "screenshot": screenshot }))
}
