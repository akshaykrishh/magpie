//! Cross-platform capture backends: clipboard-watch and native selection-read.

mod backend;
mod clipboard;
mod error;

pub use backend::{CaptureBackend, CaptureMode, Capabilities, SourceInfo};
pub use clipboard::ClipboardBackend;
pub use error::{Error, Result};
