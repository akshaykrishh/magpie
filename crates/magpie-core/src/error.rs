use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),

    #[error("capture {0} not found")]
    CaptureNotFound(i64),

    #[error("project {0} not found")]
    ProjectNotFound(i64),

    #[error("capture {0} is not leased")]
    NotLeased(i64),

    #[error("capture {0} is leased by a different session")]
    LeaseMismatch(i64),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
