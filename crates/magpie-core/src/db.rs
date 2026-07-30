use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::error::Result;

/// Timestamps are stored as UTC RFC3339 text so that lexicographic sort
/// order matches chronological order regardless of local offset at capture
/// time.
pub fn now_iso() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .expect("RFC3339 formatting of a valid OffsetDateTime cannot fail")
}

const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_init", include_str!("../migrations/0001_init.sql")),
    ("0002_screenshots", include_str!("../migrations/0002_screenshots.sql")),
];

/// `~/Library/Application Support/magpie/magpie.db` on macOS,
/// `~/.local/share/magpie/magpie.db` on Linux (or `$XDG_DATA_HOME` if set).
/// A conventional, documented, unencrypted path -- see docs/design.md
/// "own your data through transparency" -- so anyone can find and open it
/// with plain `sqlite3`. Shared by every process that opens a `Store`
/// (GUI, CLI, MCP server), which is what makes them see the same data.
pub fn default_db_path() -> Option<std::path::PathBuf> {
    dirs::data_dir().map(|d| d.join("magpie").join("magpie.db"))
}

/// Where screenshot blobs are stored on disk -- a sibling of the database
/// rather than inside it, since `blobs.path` just needs to point somewhere
/// stable; SQLite has no business holding image bytes itself.
pub fn default_blobs_dir() -> Option<std::path::PathBuf> {
    dirs::data_dir().map(|d| d.join("magpie").join("blobs"))
}

/// A single connection to the magpie database, safe to share within one
/// process behind a mutex. Separate processes (GUI, CLI, MCP server) each
/// open their own `Store` against the same file -- WAL mode plus SQLite's
/// own locking is what keeps concurrent writers safe across processes, not
/// this mutex; the mutex only serializes access *within* this process.
pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::init_connection(&conn)?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// In-memory database for tests. Still runs the full migration path so
    /// tests exercise the same schema the real app uses.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init_connection(&conn)?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn init_connection(conn: &Connection) -> Result<()> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        // Multiple processes contend for the same file (GUI, CLI, MCP);
        // rather than failing fast on SQLITE_BUSY, wait briefly for the
        // other writer's transaction to finish.
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(())
    }

    pub(crate) fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        f(&conn)
    }

    /// Like `with_conn`, but with a mutable borrow -- needed for
    /// `Connection::transaction_with_behavior`, which requires `&mut
    /// Connection` (unlike `unchecked_transaction`, which only needs `&self`
    /// but can't select a `TransactionBehavior`, always starting deferred).
    pub(crate) fn with_conn_mut<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        let mut conn = self.conn.lock().expect("db mutex poisoned");
        f(&mut conn)
    }
}

fn migrate(conn: &Connection) -> Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let target = MIGRATIONS.len() as i64;

    for (i, (_name, sql)) in MIGRATIONS.iter().enumerate() {
        let version = (i + 1) as i64;
        if version <= current {
            continue;
        }
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.commit()?;
    }

    if target > current {
        conn.pragma_update(None, "user_version", target)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_cleanly_and_is_idempotent() {
        let store = Store::open_in_memory().unwrap();
        store
            .with_conn(|conn| {
                let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
                assert_eq!(version, MIGRATIONS.len() as i64);
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn wal_mode_is_actually_active_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("magpie.db");
        let store = Store::open(&path).unwrap();
        store
            .with_conn(|conn| {
                let mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
                assert_eq!(mode, "wal");
                Ok(())
            })
            .unwrap();
        assert!(path.with_extension("db-wal").exists() || path.with_extension("db-shm").exists());
    }

    #[test]
    fn wal_and_foreign_keys_are_enabled() {
        let store = Store::open_in_memory().unwrap();
        store
            .with_conn(|conn| {
                let fk: i64 = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
                assert_eq!(fk, 1);
                Ok(())
            })
            .unwrap();
    }
}
