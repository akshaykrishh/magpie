// Stage 9 -- the hold-to-aim gesture: hold the capture hotkey past
// capture_flow::TAP_THRESHOLD and a panel appears listing the guessed
// project alongside up to nine others (by recency), each bound to a digit
// held down with the capture hotkey's own modifiers. Releasing the hotkey
// resolves whichever one is currently selected via `Store::file_capture`.
//
// This has to be a real window, not an in-webview overlay: it must show
// over whatever app the user is in, and the main window may not even be
// open. See panels.rs for the non-activating NSPanel technique it shares
// with the toast.
//
// The one hard constraint, plainly: a non-activating panel's webview
// cannot receive keyboard events -- that is the entire point of a panel
// that never takes focus. So digit selection is handled here in Rust via
// tauri-plugin-global-shortcut, registered with the capture hotkey's own
// modifiers (never bare digits, which would swallow typing app-wide), not
// in the aim webview's own JS.

use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Shortcut, ShortcutState};

use crate::panels;
use crate::state::AppState;
use crate::toast::GuessedDestination;
use crate::HotkeyRuntime;

pub const AIM_LABEL: &str = "aim";

/// The guess plus up to nine others, mapped to a 10-key digit row (1-9,
/// then 0) with the capture hotkey's own modifiers -- see `digit_shortcuts`.
const MAX_CANDIDATES: i64 = 10;

/// How long an armed gesture is allowed to sit un-released before the
/// watchdog force-dismisses it -- a safety net for a leaked digit
/// registration (see this module's doc comment), not a real interaction
/// timeout; nobody holds a hotkey down for ten real seconds on purpose.
const WATCHDOG: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Debug, Clone, Serialize)]
pub struct AimCandidate {
    pub project_id: i64,
    pub project_name: String,
    /// The literal digit ("1".."9", then "0") this candidate is bound to --
    /// the aim webview renders this next to each row rather than
    /// re-deriving index-to-digit itself.
    pub digit: char,
}

#[derive(Debug, Clone, Serialize)]
struct AimShowPayload {
    candidates: Vec<AimCandidate>,
    selected_index: usize,
}

#[derive(Debug, Clone, Serialize)]
struct AimSelectPayload {
    selected_index: usize,
}

/// A live hold-to-aim gesture -- see `AppState::aim_session`. Cleared
/// (taken) the moment it's resolved or force-dismissed, mirroring
/// `state::PendingGuess`.
pub struct AimSession {
    capture_id: i64,
    candidates: Vec<GuessedDestination>,
    selected_index: usize,
}

/// '1'..'9' then '0' for indices 0..9, matching the physical digit row
/// left to right -- not ascending numeric order, which would put '0' (the
/// tenth slot) before '1' (the first).
fn digit_for_index(index: usize) -> char {
    if index == 9 {
        '0'
    } else {
        char::from(b'1' + index as u8)
    }
}

fn code_for_digit(digit: char) -> Code {
    match digit {
        '1' => Code::Digit1,
        '2' => Code::Digit2,
        '3' => Code::Digit3,
        '4' => Code::Digit4,
        '5' => Code::Digit5,
        '6' => Code::Digit6,
        '7' => Code::Digit7,
        '8' => Code::Digit8,
        '9' => Code::Digit9,
        '0' => Code::Digit0,
        _ => unreachable!("digit_for_index only ever produces '1'..'9' or '0'"),
    }
}

/// The ten digit shortcuts the aim gesture registers while it's live, all
/// sharing the capture hotkey's own current modifiers (not the hardcoded
/// `HOTKEY` const -- `set_hotkey` can rebind it at runtime, see lib.rs's
/// `HotkeyRuntime`). Recomputed on demand rather than cached, since it's
/// ten cheap struct constructions and caching would need its own
/// invalidation the moment the hotkey is rebound.
fn digit_shortcuts(app: &AppHandle) -> Vec<Shortcut> {
    let runtime = app.state::<HotkeyRuntime>();
    let mods = runtime
        .capture
        .lock()
        .expect("capture hotkey mutex poisoned")
        .mods;
    (0..MAX_CANDIDATES as usize)
        .map(|i| Shortcut::new(Some(mods), code_for_digit(digit_for_index(i))))
        .collect()
}

/// Called `capture_flow::TAP_THRESHOLD` after a capture hotkey press if
/// nothing has resolved the pending guess yet -- the press has become a
/// hold. Shows the aim panel and arms the digit shortcuts, unless there
/// are fewer than two destinations to aim at, in which case holding does
/// exactly what tapping does (see `resolve_if_active`'s caller) and this
/// is a no-op -- the redesign plan's earned-surfaces table: hold-to-aim
/// only exists at ≥2 destinations.
pub fn arm(app: &AppHandle, capture_id: i64) {
    let state = app.state::<AppState>();
    let candidates: Vec<GuessedDestination> =
        match state.store.list_projects_by_recency(MAX_CANDIDATES) {
            Ok(projects) => projects
                .into_iter()
                .map(|p| GuessedDestination {
                    project_id: p.id,
                    project_name: p.name,
                })
                .collect(),
            Err(e) => {
                eprintln!("magpie: aim candidate lookup failed: {e}");
                return;
            }
        };
    if candidates.len() < 2 {
        return;
    }

    let generation = state.aim_generation.fetch_add(1, Ordering::SeqCst) + 1;
    *state
        .aim_session
        .lock()
        .expect("aim_session mutex poisoned") = Some(AimSession {
        capture_id,
        candidates: candidates.clone(),
        selected_index: 0,
    });

    let shortcuts = digit_shortcuts(app);
    let register_result =
        app.global_shortcut()
            .on_shortcuts(shortcuts, move |app, shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    on_digit_pressed(app, shortcut);
                }
            });
    if let Err(e) = register_result {
        eprintln!("magpie: failed to register aim digit shortcuts: {e}");
        *state
            .aim_session
            .lock()
            .expect("aim_session mutex poisoned") = None;
        return;
    }

    let payload = AimShowPayload {
        candidates: label_candidates(&candidates),
        selected_index: 0,
    };
    let _ = app.emit_to(AIM_LABEL, "aim:show", payload);

    // AppKit panel calls must happen on the main thread -- see toast.rs's
    // fire_toast for the M0 crash this avoids repeating. `arm` itself
    // always runs on capture_flow's background timer thread, never the
    // main thread, so this dispatch is never a no-op hop to itself.
    let app_for_show = app.clone();
    let _ = app.run_on_main_thread(move || {
        panels::show_panel(&app_for_show, AIM_LABEL);
    });

    let app_for_watchdog = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(WATCHDOG);
        force_dismiss_if_stale(&app_for_watchdog, generation);
    });
}

fn label_candidates(candidates: &[GuessedDestination]) -> Vec<AimCandidate> {
    candidates
        .iter()
        .enumerate()
        .map(|(i, d)| AimCandidate {
            project_id: d.project_id,
            project_name: d.project_name.clone(),
            digit: digit_for_index(i),
        })
        .collect()
}

fn on_digit_pressed(app: &AppHandle, shortcut: &Shortcut) {
    let shortcuts = digit_shortcuts(app);
    let Some(index) = shortcuts.iter().position(|s| s == shortcut) else {
        return;
    };

    let state = app.state::<AppState>();
    let mut guard = state
        .aim_session
        .lock()
        .expect("aim_session mutex poisoned");
    let Some(session) = guard.as_mut() else {
        return;
    };
    if index >= session.candidates.len() {
        return;
    }
    session.selected_index = index;
    drop(guard);

    let _ = app.emit_to(
        AIM_LABEL,
        "aim:select",
        AimSelectPayload {
            selected_index: index,
        },
    );
}

/// Resolves an in-flight aim gesture for `capture_id` if one exists,
/// unregistering its digit shortcuts and hiding the panel either way --
/// called once from `capture_flow::on_capture_hotkey_released` regardless
/// of how the hold-vs-tap timing played out, since this is the only place
/// that knows whether a gesture actually took over. Returns the project
/// the gesture landed on, or `None` if no gesture was live (a tap, or a
/// hold with too few destinations to aim at) -- the caller falls back to
/// the original guess in that case.
pub fn resolve_if_active(app: &AppHandle, capture_id: i64) -> Option<i64> {
    let state = app.state::<AppState>();
    let session = {
        let mut guard = state
            .aim_session
            .lock()
            .expect("aim_session mutex poisoned");
        match guard.as_ref() {
            Some(s) if s.capture_id == capture_id => guard.take(),
            _ => None,
        }
    }?;

    dismiss(app);
    Some(session.candidates[session.selected_index].project_id)
}

/// The watchdog's own resolution path: force-dismisses a gesture that's
/// still armed `WATCHDOG` after it started, without committing anything
/// (unlike `resolve_if_active`) -- ten seconds with no release is treated
/// as abandoned, not as a stale choice worth filing. `generation` pins
/// this check to the exact gesture that scheduled it, so it can never
/// dismiss a *later* gesture that happened to start within the window.
fn force_dismiss_if_stale(app: &AppHandle, generation: u64) {
    let state = app.state::<AppState>();
    let is_stale = {
        let guard = state
            .aim_session
            .lock()
            .expect("aim_session mutex poisoned");
        guard.is_some() && state.aim_generation.load(Ordering::SeqCst) == generation
    };
    if !is_stale {
        return;
    }
    *state
        .aim_session
        .lock()
        .expect("aim_session mutex poisoned") = None;
    eprintln!("magpie: aim gesture {generation} force-dismissed by watchdog (no release seen)");
    dismiss(app);
}

fn dismiss(app: &AppHandle) {
    let shortcuts = digit_shortcuts(app);
    if let Err(e) = app.global_shortcut().unregister_multiple(shortcuts) {
        eprintln!("magpie: failed to unregister aim digit shortcuts: {e}");
    }
    let app_for_hide = app.clone();
    let _ = app.run_on_main_thread(move || {
        panels::hide_panel(&app_for_hide, AIM_LABEL);
    });
}

pub fn init_aim_panel(app: &AppHandle) {
    panels::to_non_activating_panel(app, AIM_LABEL);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digit_for_index_runs_one_through_nine_then_zero() {
        let digits: String = (0..10).map(digit_for_index).collect();
        assert_eq!(digits, "1234567890");
    }
}
