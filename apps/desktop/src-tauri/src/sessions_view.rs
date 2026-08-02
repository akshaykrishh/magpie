use serde::Serialize;
use tauri::State;

use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

fn map_err<T>(r: magpie_core::Result<T>) -> CmdResult<T> {
    r.map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct SourceAppCount {
    pub app_name: String,
    pub count: i64,
}

/// One card in the session strip -- either a real `sessions` row (an agent
/// that connected over MCP) or the synthesized S0 card representing the
/// human's own use of this app. Deliberately one shape for both rather than
/// an enum: the frontend renders one component either way, and the fields
/// that don't apply to a given kind (an agent's `captures_since`/
/// `top_source_apps`, or S0's `holds`/`drained`/lease counters) are simply
/// left at their zero value rather than making the caller match on a
/// variant first.
#[derive(Serialize)]
pub struct SessionView {
    /// `0` for the synthesized S0 card, matching `ordinal`'s doc comment on
    /// `Session` -- S0 is never a real ordinal value.
    pub ordinal: i64,
    pub is_synthetic: bool,
    pub client: Option<String>,
    pub branch: Option<String>,
    pub started_at: String,
    /// Live lease count -- `0` for S0, which never holds leases.
    pub holds: i64,
    /// `completed_count` for a real session ("DRAINED" in the design doc's
    /// language); `0` for S0, which has no queue-completion concept.
    pub drained: i64,
    pub failed_count: i64,
    pub handback_count: i64,
    /// `Some` only for S0 -- captures made since this app process started.
    /// `None` for a real session: an agent's activity is already fully
    /// described by holds/drained/failed/handback, and `captures_during_session`
    /// is a `Session` row field that only gets its final value at
    /// `end_session`, not while the session is still live.
    pub captures_since: Option<i64>,
    /// Non-empty only for S0, most-frequent first. Always empty for a real
    /// session -- an agent doesn't have a "source app".
    pub top_source_apps: Vec<SourceAppCount>,
}

const TOP_SOURCE_APPS_LIMIT: i64 = 3;

/// Every live session for the strip: real MCP sessions that haven't ended,
/// plus the synthesized S0 card for the human. S0 is never written as a
/// `sessions` row -- see `AppState::app_started_at`'s doc comment for why
/// (a real row would mean `end_session` drops a digest capture into the
/// stream on every quit). Global, not scoped to a project -- matches the
/// rest of the main window (see the redesign plan's "there is no default
/// focused project").
#[tauri::command]
pub fn list_sessions_view(state: State<AppState>) -> CmdResult<Vec<SessionView>> {
    let sessions = map_err(state.store.list_sessions(None))?
        .into_iter()
        .filter(|s| s.ended_at.is_none());
    let holds = map_err(state.store.held_capture_counts())?;

    let mut views: Vec<SessionView> = sessions
        .map(|s| SessionView {
            ordinal: s.ordinal.unwrap_or_default(),
            is_synthetic: false,
            client: s.client,
            branch: s.branch,
            started_at: s.started_at,
            holds: holds.get(&s.id).copied().unwrap_or(0),
            drained: s.completed_count,
            failed_count: s.failed_count,
            handback_count: s.handback_count,
            captures_since: None,
            top_source_apps: Vec::new(),
        })
        .collect();
    views.sort_by_key(|v| v.ordinal);

    let captures_since = map_err(state.store.count_captures_since(&state.app_started_at))?;
    let top_source_apps = map_err(
        state
            .store
            .top_source_apps_since(&state.app_started_at, TOP_SOURCE_APPS_LIMIT),
    )?
    .into_iter()
    .map(|(app_name, count)| SourceAppCount { app_name, count })
    .collect();

    let mut result = vec![SessionView {
        ordinal: 0,
        is_synthetic: true,
        client: None,
        branch: None,
        started_at: state.app_started_at.clone(),
        holds: 0,
        drained: 0,
        failed_count: 0,
        handback_count: 0,
        captures_since: Some(captures_since),
        top_source_apps,
    }];
    result.append(&mut views);
    Ok(result)
}
