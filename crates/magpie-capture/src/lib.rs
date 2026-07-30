//! Cross-platform capture backends: clipboard-watch and native selection-read.

mod backend;
mod clipboard;
mod error;
mod freshness;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
mod png;

pub use backend::{Capabilities, CaptureBackend, CaptureMode, ScreenshotCapture, SourceInfo};
pub use clipboard::ClipboardBackend;
pub use error::{Error, Result};
#[cfg(target_os = "linux")]
pub use linux::LinuxBackend;
#[cfg(target_os = "macos")]
pub use macos::MacosBackend;
