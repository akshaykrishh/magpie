use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub remote_url: Option<String>,
    pub common_git_dir: Option<String>,
    pub created_at: String,
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

    pub source_id: Option<i64>,
    pub merged_into: Option<i64>,
}

impl Capture {
    pub fn in_now(&self) -> bool {
        self.queue_pos.is_some()
    }

    pub fn is_leased(&self) -> bool {
        self.lease_session.is_some()
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: i64,
    pub at: String,
    pub actor: String,
    pub action: String,
    pub capture_id: Option<i64>,
}
