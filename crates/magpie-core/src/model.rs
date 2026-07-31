use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub remote_url: Option<String>,
    pub common_git_dir: Option<String>,
    pub created_at: String,
    pub last_active_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub client: Option<String>,
    pub pid: i64,
    pub project_id: Option<i64>,
    pub branch: Option<String>,
    pub started_at: String,
    pub last_active_at: Option<String>,
    pub ended_at: Option<String>,
    pub leased_count: i64,
    pub completed_count: i64,
    pub failed_count: i64,
    pub handback_count: i64,
    pub captures_during_session: Option<i64>,
    pub unpromoted_at_end: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub id: i64,
    pub app_name: Option<String>,
    pub bundle_id: Option<String>,
    pub window_title: Option<String>,
    pub url: Option<String>,
    pub captured_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capture {
    pub id: i64,
    /// `"capture"` for everything a human or agent captured; `"session_digest"`
    /// for a synthetic summary magpie writes when a session ends (see
    /// `Store::end_session`). Both live in the same stream and are both
    /// searchable -- a digest is a special kind of row, not a special table.
    pub kind: String,
    pub body: String,
    pub created_at: String,
    pub done_at: Option<String>,
    pub failed_reason: Option<String>,

    pub queue_pos: Option<f64>,
    pub project_id: Option<i64>,
    pub branch: Option<String>,

    pub lease_session: Option<String>,
    pub lease_client: Option<String>,
    pub lease_pid: Option<i64>,
    pub lease_at: Option<String>,
    pub lease_head_commit: Option<String>,

    pub handback_note: Option<String>,
    pub diff_stat: Option<String>,
    pub handback_at: Option<String>,

    pub source_id: Option<i64>,
    pub merged_into: Option<i64>,
    pub section_id: Option<i64>,
    pub deleted_at: Option<String>,
}

impl Capture {
    pub fn in_now(&self) -> bool {
        self.queue_pos.is_some()
    }

    pub fn is_leased(&self) -> bool {
        self.lease_session.is_some()
    }

    /// A handed-back item stays in Now (it isn't done) but isn't active
    /// work either -- this is what a future UI checks to render the
    /// "needs review" state distinctly from "leased" or "open".
    pub fn needs_review(&self) -> bool {
        self.handback_at.is_some() && self.done_at.is_none()
    }

    pub fn is_session_digest(&self) -> bool {
        self.kind == "session_digest"
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Blob {
    pub id: i64,
    pub capture_id: i64,
    pub path: String,
    pub mime: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub ocr_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Template {
    pub id: i64,
    pub title: String,
    pub body: String,
    pub created_at: String,
    pub description: Option<String>,
    /// A pack manifest's declared metadata for this template's `{{name}}`
    /// placeholders, serialized as JSON (`{name: {description, default}}`).
    /// `None` for templates without any declared variable metadata --
    /// substitution itself never depends on this, only the fill-in form's
    /// labels do (see `docs/design.md` for the manifest shape).
    pub variables_json: Option<String>,
    pub pack_id: Option<i64>,
    pub section_id: Option<i64>,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pack {
    pub id: i64,
    pub source_url: String,
    pub name: String,
    pub description: Option<String>,
    pub imported_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub id: i64,
    pub name: String,
    pub position: f64,
    pub created_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: i64,
    pub at: String,
    pub actor: String,
    pub action: String,
    pub capture_id: Option<i64>,
}
