//! Domain model, storage, search, and export for magpie.

mod captures;
mod db;
mod error;
mod model;
mod projects;
mod tags;

pub use captures::NewSource;
pub use db::{now_iso, Store};
pub use error::{Error, Result};
pub use model::{AuditEntry, Capture, Project, Source, Tag, Template};
