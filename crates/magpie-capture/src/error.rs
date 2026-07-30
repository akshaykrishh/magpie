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
}

pub type Result<T> = std::result::Result<T, Error>;
