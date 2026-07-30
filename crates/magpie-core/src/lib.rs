//! Domain model, storage, search, and export for magpie.

mod db;
mod error;
mod model;

pub use db::{now_iso, Store};
pub use error::{Error, Result};
pub use model::{AuditEntry, Capture, Project, Source, Tag, Template};
