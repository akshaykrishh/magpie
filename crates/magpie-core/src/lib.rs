//! Domain model, storage, search, and export for magpie.

mod audit;
mod blobs;
mod captures;
mod db;
mod error;
mod export;
mod lease;
mod merge;
mod model;
mod packs;
mod placeholders;
mod projects;
mod search;
mod sections;
mod sessions;
mod sources;
mod tags;
mod templates;

pub use captures::NewSource;
pub use db::{default_blobs_dir, default_db_path, now_iso, Store};
pub use error::{Error, Result};
pub use export::CaptureExport;
pub use lease::LeaseIdentity;
pub use model::{AuditEntry, Blob, Capture, Pack, Project, Section, Session, Source, Tag, Template};
pub use packs::{ParsedPack, ParsedPrompt};
pub use projects::ProjectOverview;
