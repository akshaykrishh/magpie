// Stage 10 -- Across (⌘⌥K): the cross-project "what needs a look" rollup,
// floating over whatever app is focused like the aim panel, but unlike it
// this window must accept typed input (search-as-you-type over project
// names) -- so it converts to `panels::KeyableFloatingPanel`, not the
// toast/aim's never-key kind.
//
// The spike the redesign plan called for, and what it actually found:
// under `ActivationPolicy::Accessory` (this app never shows a Dock icon),
// `show_and_make_key` (`makeFirstResponder` + `orderFrontRegardless` +
// `makeKeyWindow`, all real AppKit calls) reliably delivers key focus --
// that part was never the problem. What doesn't work without help is
// *rendering*: `panels::show_panel`/`show_and_make_key_panel` also call
// Tauri's own `window.show()` for exactly this reason (see that module's
// doc comment) -- without it, a panel shown only via the raw NSPanel
// methods stays geometrically on-screen and focusable but paints nothing.
// The `ActivationPolicy::Regular` fallback the plan describes turned out
// not to be needed; if a future macOS/Tauri change regresses key focus
// specifically (not rendering), that's still the documented escape hatch.

use tauri::{AppHandle, Emitter, Manager};

use crate::panels;

pub const ACROSS_LABEL: &str = "across";

pub fn init_across_panel(app: &AppHandle) {
    panels::to_keyable_panel(app, ACROSS_LABEL);
}

/// Bound to the always-on ⌘⌥K global shortcut (see lib.rs) -- toggles
/// Across shut if it's already the frontmost thing, open otherwise. A
/// second press of the same chord that opened it is the obvious way to
/// dismiss it again without reaching for Escape.
pub fn toggle(app: &AppHandle) {
    let visible = app
        .get_webview_window(ACROSS_LABEL)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    if visible {
        hide(app);
    } else {
        show(app);
    }
}

fn show(app: &AppHandle) {
    panels::show_and_make_key_panel(app, ACROSS_LABEL);
    // A bare trigger, not a data payload -- the frontend already knows how
    // to fetch `list_projects_overview` itself (same IPC the dock and tray
    // poll), and refetching on every open is simplest given how cheap that
    // query is and how rarely this window opens.
    let _ = app.emit_to(ACROSS_LABEL, "across:show", ());
}

/// Also the target of `select_across_project` (commands.rs), once a
/// project's been picked -- the search field itself has no separate
/// "close" affordance, so anything that resolves this window's purpose
/// hides it the same way Escape / click-away does.
pub fn hide(app: &AppHandle) {
    panels::hide_panel(app, ACROSS_LABEL);
}
