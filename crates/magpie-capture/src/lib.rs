//! Cross-platform capture backends: clipboard-watch and native selection-read.

mod backend;
mod clipboard;
mod error;
#[cfg(target_os = "macos")]
mod macos;

pub use backend::{CaptureBackend, CaptureMode, Capabilities, SourceInfo};
pub use clipboard::ClipboardBackend;
pub use error::{Error, Result};
#[cfg(target_os = "macos")]
pub use macos::MacosBackend;
