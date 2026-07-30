use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("clipboard error: {0}")]
    Clipboard(#[from] arboard::Error),

    #[cfg(target_os = "macos")]
    #[error("failed to create a CGEventSource")]
    EventSourceCreation,

    #[cfg(target_os = "macos")]
    #[error("failed to create a CGEvent")]
    EventCreation,

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("OCR request failed: {0}")]
    Ocr(String),

    #[cfg(target_os = "linux")]
    #[error("screenshot portal request failed: {0}")]
    Portal(String),
}

pub type Result<T> = std::result::Result<T, Error>;
