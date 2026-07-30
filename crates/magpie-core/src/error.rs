use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),

    #[error("capture {0} not found")]
    CaptureNotFound(i64),

    #[error("project {0} not found")]
    ProjectNotFound(i64),

    #[error("merge needs at least two captures")]
    MergeNeedsAtLeastTwo,

    #[error("template {0} not found")]
    TemplateNotFound(i64),

    #[error("capture {0} is not currently leased")]
    NotLeased(i64),

    #[error("capture {0} is not leased by session {1}")]
    LeaseMismatch(i64, String),

    #[error("blob {0} not found")]
    BlobNotFound(i64),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
