use tauri::{Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::state::AppState;
use crate::HotkeyRuntime;

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

/// Rebinds the capture or screenshot global hotkey at runtime.
///
/// Validates that `combo` carries at least one modifier (a bare key would
/// hijack normal typing the moment it's registered), unregisters the
/// currently-active shortcut for `kind`, and attempts to register `combo`
/// in its place. Registration can fail if another application already owns
/// that combination -- a real, expected failure mode. On failure the old
/// shortcut is re-registered so the app never ends up with *no* hotkey
/// registered for `kind`, and the failure is reported honestly rather than
/// silently ignored. The new binding is only persisted via
/// `Store::set_setting` once OS registration has actually succeeded.
#[tauri::command]
pub fn set_hotkey(
    app: tauri::AppHandle,
    state: State<AppState>,
    kind: String,
    combo: String,
) -> CmdResult<()> {
    let has_modifier = ["Command", "Control", "Alt", "Shift", "Cmd", "Ctrl", "Option"]
        .iter()
        .any(|m| combo.contains(m));
    if !has_modifier {
        return Err(format!(
            "\"{combo}\" has no modifier key -- binding a bare key would break normal typing"
        ));
    }

    let new_shortcut: Shortcut =
        combo.parse().map_err(|e| format!("invalid shortcut syntax: {e}"))?;

    let setting_key = match kind.as_str() {
        "capture" => "capture_hotkey",
        "screenshot" => "screenshot_hotkey",
        other => return Err(format!("unknown hotkey kind: {other}")),
    };

    // Used only for the error/rollback message below -- the actual
    // `Shortcut` value to unregister comes from `HotkeyRuntime`, which is
    // guaranteed to match what's really registered with the OS right now
    // (kept in sync by this same command on every successful call).
    let previous_combo = state
        .store
        .get_setting(setting_key)
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| {
            if kind == "capture" {
                crate::HOTKEY.to_string()
            } else {
                crate::SCREENSHOT_HOTKEY.to_string()
            }
        });

    let runtime = app.state::<HotkeyRuntime>();
    let slot = match kind.as_str() {
        "capture" => &runtime.capture,
        "screenshot" => &runtime.screenshot,
        _ => unreachable!("kind was already validated above"),
    };
    let mut current = slot.lock().unwrap();
    let old_shortcut: Shortcut = *current;

    let gs = app.global_shortcut();
    let _ = gs.unregister(old_shortcut);

    // Registration can fail if another app already owns this combination --
    // that's a real, expected failure mode, not an edge case to ignore. On
    // failure, restore the old binding so the app doesn't silently end up
    // with no hotkey registered at all, and report the failure honestly
    // rather than claiming success.
    if let Err(e) = gs.register(new_shortcut) {
        let _ = gs.register(old_shortcut);
        return Err(format!(
            "could not register \"{combo}\": {e} (still using \"{previous_combo}\")"
        ));
    }

    *current = new_shortcut;
    drop(current);

    state.store.set_setting(setting_key, &combo).map_err(|e| e.to_string())
}
