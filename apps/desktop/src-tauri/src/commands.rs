use magpie_capture::Capabilities;
use magpie_core::{Capture, Project, Tag};
use serde::Deserialize;
use tauri::State;

use crate::state::AppState;

/// A dedicated wire type for "which slice of the stream" rather than
/// `Option<Option<i64>>` directly: serde's default `Option<Option<T>>`
/// handling collapses `None` and `Some(None)` to the same JSON `null`,
/// which would make "don't filter" and "Inbox only" indistinguishable over
/// IPC. This makes the three cases explicit instead.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectFilter {
    All,
    Inbox,
    Project { id: i64 },
}

impl ProjectFilter {
    fn into_query(self) -> Option<Option<i64>> {
        match self {
            ProjectFilter::All => None,
            ProjectFilter::Inbox => Some(None),
            ProjectFilter::Project { id } => Some(Some(id)),
        }
    }
}

type CmdResult<T> = Result<T, String>;

fn map_err<T>(r: magpie_core::Result<T>) -> CmdResult<T> {
    r.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_stream(
    state: State<AppState>,
    filter: ProjectFilter,
    limit: i64,
    offset: i64,
) -> CmdResult<Vec<Capture>> {
    map_err(state.store.list_stream(filter.into_query(), limit, offset))
}

#[tauri::command]
pub fn list_now(state: State<AppState>, project_id: Option<i64>) -> CmdResult<Vec<Capture>> {
    map_err(state.store.list_now(project_id))
}

/// A prompt typed directly into the app, as opposed to something captured
/// from elsewhere -- goes straight into Now rather than landing in the
/// stream first, since typing it here is already a deliberate act of
/// queuing work, not an ambiguous "keep this for later".
#[tauri::command]
pub fn add_typed_capture(state: State<AppState>, body: String) -> CmdResult<Capture> {
    let capture = map_err(state.store.capture(&body, None))?;
    map_err(state.store.promote(capture.id))
}

#[tauri::command]
pub fn promote_capture(state: State<AppState>, id: i64) -> CmdResult<Capture> {
    map_err(state.store.promote(id))
}

#[tauri::command]
pub fn demote_capture(state: State<AppState>, id: i64) -> CmdResult<Capture> {
    map_err(state.store.demote(id))
}

#[tauri::command]
pub fn reorder_capture(
    state: State<AppState>,
    id: i64,
    after_id: Option<i64>,
) -> CmdResult<Capture> {
    map_err(state.store.reorder(id, after_id))
}

#[tauri::command]
pub fn mark_capture_done(state: State<AppState>, id: i64) -> CmdResult<Capture> {
    map_err(state.store.mark_done(id))
}

#[tauri::command]
pub fn reopen_capture(state: State<AppState>, id: i64) -> CmdResult<Capture> {
    map_err(state.store.reopen(id))
}

#[tauri::command]
pub fn search_captures(
    state: State<AppState>,
    query: String,
    limit: i64,
) -> CmdResult<Vec<Capture>> {
    map_err(state.store.search(&query, limit))
}

#[tauri::command]
pub fn merge_captures(state: State<AppState>, ids: Vec<i64>) -> CmdResult<Capture> {
    map_err(state.store.merge(&ids))
}

#[tauri::command]
pub fn list_merge_sources(state: State<AppState>, merged_capture_id: i64) -> CmdResult<Vec<Capture>> {
    map_err(state.store.list_merge_sources(merged_capture_id))
}

#[tauri::command]
pub fn assign_capture_project(
    state: State<AppState>,
    id: i64,
    project_id: Option<i64>,
) -> CmdResult<Capture> {
    map_err(state.store.assign_project(id, project_id))
}

#[tauri::command]
pub fn add_capture_tag(state: State<AppState>, capture_id: i64, name: String) -> CmdResult<Tag> {
    map_err(state.store.add_tag(capture_id, &name))
}

#[tauri::command]
pub fn remove_capture_tag(state: State<AppState>, capture_id: i64, name: String) -> CmdResult<()> {
    map_err(state.store.remove_tag(capture_id, &name))
}

#[tauri::command]
pub fn list_capture_tags(state: State<AppState>, capture_id: i64) -> CmdResult<Vec<Tag>> {
    map_err(state.store.list_tags_for_capture(capture_id))
}

#[tauri::command]
pub fn list_captures_by_tag(state: State<AppState>, name: String) -> CmdResult<Vec<Capture>> {
    map_err(state.store.list_captures_by_tag(&name))
}

#[tauri::command]
pub fn list_projects(state: State<AppState>) -> CmdResult<Vec<Project>> {
    map_err(state.store.list_projects())
}

#[tauri::command]
pub fn get_or_create_project(
    state: State<AppState>,
    name: String,
    remote_url: Option<String>,
    common_git_dir: Option<String>,
) -> CmdResult<Project> {
    map_err(state.store.get_or_create_project(
        &name,
        remote_url.as_deref(),
        common_git_dir.as_deref(),
    ))
}

#[tauri::command]
pub fn export_json(state: State<AppState>) -> CmdResult<String> {
    map_err(state.store.export_json())
}

#[tauri::command]
pub fn export_markdown(state: State<AppState>) -> CmdResult<String> {
    map_err(state.store.export_markdown())
}

#[tauri::command]
pub fn capture_capabilities(state: State<AppState>) -> Capabilities {
    state.backend.capabilities()
}
