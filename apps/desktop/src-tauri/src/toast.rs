// The capture toast: a non-activating panel (see panels.rs for the shared
// NSPanel plumbing this is now a thin caller over) that reports what a
// capture hotkey press just did, never taking focus from whatever app the
// user was in.

use serde::Serialize;

/// A proposed filing destination named in a `Captured` toast -- never
/// written to the capture's `project_id` just by being shown (see
/// docs/design.md "nothing is ever auto-filed"); committing it is a
/// separate, explicit action (see capture_flow.rs's tap-to-confirm).
///
/// Always rendered as a *guess* (dashed underline, per the design's visual
/// language), never as "certain": certain filing only happens when an MCP
/// session's `project::detect()` reads an actual git remote (see
/// crates/magpie-mcp/src/project.rs), and agent-driven captures never show
/// a toast at all -- there is no code path today where this window's
/// "Captured" toast could honestly claim certainty. If one is ever added,
/// this struct is where a `confidence` field belongs; don't add one before
/// something real can set it.
#[derive(Debug, Clone, Serialize)]
pub struct GuessedDestination {
    pub project_id: i64,
    pub project_name: String,
}

/// Everything the toast window can be told to show.
///
/// Only two of the design's four visual states are constructed by any code
/// path today -- `Captured` (with or without a destination) and `Nothing`.
/// The third, "chord" state ("Into Now" -- hold the hotkey to promote
/// straight into the queue) is a distinct gesture from stage 9's
/// hold-to-aim picker (see aim.rs) and isn't scoped by the redesign plan
/// at all; it is deliberately not added here as an always-`None` field or
/// a never-constructed variant ahead of whatever eventually specifies it.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToastPayload {
    /// A capture landed in the stream. `destination` is `None` when no
    /// project exists yet to guess -- a real state at the earliest data
    /// tiers, not an error (see the redesign plan's "zero"/"sparse" tiers).
    /// `first_capture` is true only for the very first capture this
    /// installation has ever made (see capture_flow.rs's
    /// `toast_capture_count` setting) -- it holds the toast for longer
    /// with an explanatory message instead of the usual pill, per the
    /// design's "capture #1 spells it out."
    Captured {
        capture_id: i64,
        destination: Option<GuessedDestination>,
        first_capture: bool,
    },
    /// Nothing was captured, or a resolvable/expected failure occurred --
    /// covers "Nothing to capture," Secure Input blocked, and capture or
    /// screenshot insert failures. One shared muted visual treatment (see
    /// toast.ts); the specific reason always lives in `message` rather
    /// than being collapsed to a single generic string, since e.g. Secure
    /// Input blocked needs a different fix from an empty clipboard.
    Nothing { message: String },
}

use tauri::AppHandle;

use crate::panels;

pub const TOAST_LABEL: &str = "toast";

pub fn init_toast_panel(app: &AppHandle) {
    panels::to_non_activating_panel(app, TOAST_LABEL);
}

pub fn show_toast(app: &AppHandle) {
    panels::show_panel(app, TOAST_LABEL);
}

pub fn hide_toast(app: &AppHandle) {
    panels::hide_panel(app, TOAST_LABEL);
}
