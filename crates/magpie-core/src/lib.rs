//! Domain model, storage, search, and export for magpie.

mod audit;
mod captures;
mod db;
mod error;
mod export;
mod lease;
mod merge;
mod model;
mod projects;
mod search;
mod tags;
mod templates;

pub use captures::NewSource;
pub use db::{default_db_path, now_iso, Store};
pub use error::{Error, Result};
pub use export::CaptureExport;
pub use lease::LeaseIdentity;
pub use model::{AuditEntry, Capture, Project, Source, Tag, Template};
