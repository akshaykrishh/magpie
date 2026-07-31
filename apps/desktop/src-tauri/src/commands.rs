use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use magpie_capture::Capabilities;
use magpie_core::{
    AuditEntry, Blob, Capture, Project, ProjectOverview, Section, Session, Tag, Template,
};
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
pub fn list_merge_sources(
    state: State<AppState>,
    merged_capture_id: i64,
) -> CmdResult<Vec<Capture>> {
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
pub fn list_sessions(state: State<AppState>, project_id: Option<i64>) -> CmdResult<Vec<Session>> {
    map_err(state.store.list_sessions(project_id))
}

#[tauri::command]
pub fn list_projects_overview(state: State<AppState>) -> CmdResult<Vec<ProjectOverview>> {
    map_err(state.store.list_projects_overview())
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

#[tauri::command]
pub fn list_templates(state: State<AppState>) -> CmdResult<Vec<Template>> {
    map_err(state.store.list_templates())
}

#[tauri::command]
pub fn create_template(state: State<AppState>, title: String, body: String) -> CmdResult<Template> {
    map_err(state.store.create_template(&title, &body))
}

#[tauri::command]
pub fn update_template(
    state: State<AppState>,
    id: i64,
    title: String,
    body: String,
) -> CmdResult<Template> {
    map_err(state.store.update_template(id, &title, &body))
}

#[tauri::command]
pub fn delete_template(state: State<AppState>, id: i64) -> CmdResult<()> {
    map_err(state.store.delete_template(id))
}

#[tauri::command]
pub fn delete_capture(state: State<AppState>, id: i64) -> CmdResult<Capture> {
    map_err(state.store.soft_delete_capture(id))
}

#[tauri::command]
pub fn restore_capture(state: State<AppState>, id: i64) -> CmdResult<Capture> {
    map_err(state.store.restore_capture(id))
}

#[tauri::command]
pub fn list_recently_deleted_captures(state: State<AppState>) -> CmdResult<Vec<Capture>> {
    map_err(state.store.list_recently_deleted_captures())
}

#[tauri::command]
pub fn restore_template(state: State<AppState>, id: i64) -> CmdResult<Template> {
    map_err(state.store.restore_template(id))
}

#[tauri::command]
pub fn list_recently_deleted_templates(state: State<AppState>) -> CmdResult<Vec<Template>> {
    map_err(state.store.list_recently_deleted_templates())
}

#[tauri::command]
pub fn create_section(state: State<AppState>, name: String) -> CmdResult<Section> {
    map_err(state.store.create_section(&name))
}

#[tauri::command]
pub fn rename_section(state: State<AppState>, id: i64, name: String) -> CmdResult<Section> {
    map_err(state.store.rename_section(id, &name))
}

#[tauri::command]
pub fn list_sections(state: State<AppState>) -> CmdResult<Vec<Section>> {
    map_err(state.store.list_sections())
}

#[tauri::command]
pub fn reorder_section(
    state: State<AppState>,
    id: i64,
    after_id: Option<i64>,
) -> CmdResult<Section> {
    map_err(state.store.reorder_section(id, after_id))
}

#[tauri::command]
pub fn delete_section(state: State<AppState>, id: i64) -> CmdResult<()> {
    map_err(state.store.delete_section(id))
}

#[tauri::command]
pub fn restore_section(state: State<AppState>, id: i64) -> CmdResult<Section> {
    map_err(state.store.restore_section(id))
}

#[tauri::command]
pub fn assign_capture_section(
    state: State<AppState>,
    id: i64,
    section_id: Option<i64>,
) -> CmdResult<Capture> {
    map_err(state.store.assign_capture_section(id, section_id))
}

#[tauri::command]
pub fn assign_template_section(
    state: State<AppState>,
    id: i64,
    section_id: Option<i64>,
) -> CmdResult<Template> {
    map_err(state.store.assign_template_section(id, section_id))
}

#[tauri::command]
pub fn instantiate_template(
    state: State<AppState>,
    template_id: i64,
    project_id: Option<i64>,
) -> CmdResult<Capture> {
    map_err(state.store.instantiate_template(template_id, project_id))
}

/// The `{{name}}` placeholders a template's body references -- the GUI
/// asks for this before showing the "Run" fill-in form, rather than
/// re-deriving it client-side, so the server's parsing is always the one
/// source of truth for what counts as a placeholder.
#[tauri::command]
pub fn get_template_variables(state: State<AppState>, template_id: i64) -> CmdResult<Vec<String>> {
    map_err(state.store.template_variables(template_id))
}

#[tauri::command]
pub fn instantiate_template_with_values(
    state: State<AppState>,
    template_id: i64,
    project_id: Option<i64>,
    values: std::collections::HashMap<String, String>,
) -> CmdResult<Capture> {
    map_err(
        state
            .store
            .instantiate_template_with_values(template_id, project_id, &values),
    )
}

/// "Queue this everywhere" -- one template, instantiated into several
/// projects' Now at once (see docs/design.md's multi-project section).
#[tauri::command]
pub fn instantiate_template_into_many(
    state: State<AppState>,
    template_id: i64,
    project_ids: Vec<Option<i64>>,
) -> CmdResult<Vec<Capture>> {
    map_err(
        state
            .store
            .instantiate_template_into_many(template_id, &project_ids),
    )
}

/// What an agent did while you were away -- see docs/design.md "Agent
/// trust": this is what turns that from a mystery into a scroll.
#[tauri::command]
pub fn list_audit(state: State<AppState>, limit: i64) -> CmdResult<Vec<AuditEntry>> {
    map_err(state.store.list_audit(limit))
}

#[tauri::command]
pub fn get_capture_blob(state: State<AppState>, capture_id: i64) -> CmdResult<Option<Blob>> {
    map_err(state.store.get_blob_for_capture(capture_id))
}

/// The blob's image bytes, base64-encoded as a `data:` URL the webview can
/// drop straight into an `<img src>`. Chosen over Tauri's asset protocol
/// specifically to avoid widening this app's permission surface -- the
/// asset protocol needs its own filesystem scope declared in
/// `capabilities/default.json`, where today `core:default` plus
/// `opener:default` covers everything. Passing image bytes over IPC as a
/// string is well within what Tauri's IPC handles routinely for a local
/// desktop app, and needs no new capability grant at all.
#[tauri::command]
pub fn get_blob_image_data_url(
    state: State<AppState>,
    capture_id: i64,
) -> CmdResult<Option<String>> {
    let Some(blob) = map_err(state.store.get_blob_for_capture(capture_id))? else {
        return Ok(None);
    };
    let bytes = std::fs::read(&blob.path).map_err(|e| e.to_string())?;
    Ok(Some(format!(
        "data:{};base64,{}",
        blob.mime,
        BASE64.encode(bytes)
    )))
}

/// Opens System Settings directly to the Accessibility pane, rather than
/// triggering AXIsProcessTrustedWithOptions' own prompt dialog -- that API
/// needs a CFDictionary built with the kAXTrustedCheckOptionPrompt key,
/// which means hand-rolling CFBoolean/CFDictionary construction with the
/// same raw-CoreFoundation-ownership risk already avoided once for window
/// titles (see magpie-capture's MacosBackend doc comment). Opening the pane
/// directly needs none of that and gets the user to the same place: the
/// toggle they have to flip either way.
#[tauri::command]
pub fn open_accessibility_settings(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_opener::OpenerExt;
        app.opener()
            .open_url(
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
                None::<String>,
            )
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Ok(())
    }
}
