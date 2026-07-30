use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("clipboard error: {0}")]
    Clipboard(#[from] arboard::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
