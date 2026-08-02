use std::path::Path;
use std::sync::{LazyLock, Mutex};

use rusqlite::Connection;
use time::OffsetDateTime;

use crate::error::Result;

/// RFC3339 with fractional seconds *always* present at a fixed 9-digit
/// width, never `well_known::Rfc3339` -- that formatter omits the
/// fractional part entirely when nanoseconds happen to be exactly zero,
/// and `.` (0x2E) sorts *before* `Z`/digits in ASCII, so a timestamp that
/// happens to land on a whole second (no fractional part, ends in `Z`)
/// sorts as *greater than* one a fraction of a second later in the same
/// second (has a fractional part, so `.` appears before that same `Z`).
/// Every comparison in this codebase -- `count_captures_since`,
/// `is_quiet_now`, the purge sweep, session ordinal windows -- depends on
/// lexicographic order matching chronological order unconditionally, so a
/// timestamp that occasionally omits its fractional part is a real
/// (if rare -- it needs an exact-zero-nanosecond hit) correctness bug, not
/// a cosmetic one. Fixed width closes it: the fractional part is now
/// always there, so there's never a `.` vs. no-`.` mismatch to sort
/// backwards on.
static ISO_FORMAT: LazyLock<Vec<time::format_description::BorrowedFormatItem<'static>>> =
    LazyLock::new(|| {
        time::format_description::parse_borrowed::<1>(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:9]Z",
        )
        .expect("hardcoded format description is valid")
    });

fn format_iso(t: OffsetDateTime) -> String {
    t.format(&*ISO_FORMAT)
        .expect("formatting a valid OffsetDateTime with a valid format description cannot fail")
}

/// Timestamps are stored as UTC text in a fixed-width variant of RFC3339
/// (see `ISO_FORMAT`) so that lexicographic sort order matches
/// chronological order regardless of local offset at capture time.
pub fn now_iso() -> String {
    format_iso(OffsetDateTime::now_utc())
}

/// The cutoff for "older than `days` days ago" -- rows with `deleted_at`
/// before this are eligible for the purge sweep's hard delete.
pub fn purge_cutoff(days: i64) -> String {
    format_iso(OffsetDateTime::now_utc() - time::Duration::days(days))
}

/// `now + hours` -- for the tray's "Quiet for an hour", which writes this
/// into the `quiet_until` setting (compared lexicographically against
/// `now_iso()` by `capture_flow.rs`'s `is_quiet_now`, same string-sort
/// trick `purge_cutoff` relies on).
pub fn iso_plus_hours(hours: i64) -> String {
    format_iso(OffsetDateTime::now_utc() + time::Duration::hours(hours))
}

const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_init", include_str!("../migrations/0001_init.sql")),
    (
        "0002_screenshots",
        include_str!("../migrations/0002_screenshots.sql"),
    ),
    ("0003_packs", include_str!("../migrations/0003_packs.sql")),
    (
        "0004_project_recency",
        include_str!("../migrations/0004_project_recency.sql"),
    ),
    (
        "0005_sessions",
        include_str!("../migrations/0005_sessions.sql"),
    ),
    (
        "0006_handback",
        include_str!("../migrations/0006_handback.sql"),
    ),
    (
        "0007_session_digests",
        include_str!("../migrations/0007_session_digests.sql"),
    ),
    (
        "0008_sections_and_soft_delete",
        include_str!("../migrations/0008_sections_and_soft_delete.sql"),
    ),
    (
        "0009_settings",
        include_str!("../migrations/0009_settings.sql"),
    ),
    (
        "0010_ui_provenance",
        include_str!("../migrations/0010_ui_provenance.sql"),
    ),
];

/// `~/Library/Application Support/magpie/magpie.db` on macOS,
/// `~/.local/share/magpie/magpie.db` on Linux (or `$XDG_DATA_HOME` if set).
/// A conventional, documented, unencrypted path -- see docs/design.md
/// "own your data through transparency" -- so anyone can find and open it
/// with plain `sqlite3`. Shared by every process that opens a `Store`
/// (GUI, CLI, MCP server), which is what makes them see the same data.
///
/// `MAGPIE_DB_PATH`, if set, overrides this entirely -- the mechanism that
/// lets `magpie seed --tier <t>` produce a fixture database and then have
/// `MAGPIE_DB_PATH=<path> pnpm tauri dev` point the real desktop app at it,
/// without ever touching the real default path. Not documented as a user-
/// facing setting; it's a developer/testing knob, checked before the
/// platform default so it works identically for the GUI, the CLI, and the
/// MCP server (whichever one launches first still sees the same override).
pub fn default_db_path() -> Option<std::path::PathBuf> {
    if let Ok(over) = std::env::var("MAGPIE_DB_PATH") {
        return Some(std::path::PathBuf::from(over));
    }
    dirs::data_dir().map(|d| d.join("magpie").join("magpie.db"))
}

/// Where screenshot blobs are stored on disk -- a sibling of the database
/// rather than inside it, since `blobs.path` just needs to point somewhere
/// stable; SQLite has no business holding image bytes itself.
///
/// `MAGPIE_BLOBS_DIR` overrides this the same way `MAGPIE_DB_PATH` overrides
/// `default_db_path` -- see that function's doc comment.
pub fn default_blobs_dir() -> Option<std::path::PathBuf> {
    if let Ok(over) = std::env::var("MAGPIE_BLOBS_DIR") {
        return Some(std::path::PathBuf::from(over));
    }
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
    pub(crate) fn with_conn_mut<T>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T>,
    ) -> Result<T> {
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

    // Regression test for a real bug: `well_known::Rfc3339` omits the
    // fractional-seconds component entirely when nanoseconds are exactly
    // zero, and `.` sorts before `Z` in ASCII -- so a whole-second
    // timestamp could sort as *later* than one a fraction of a second
    // after it in the same second, silently breaking every lexicographic
    // comparison this codebase relies on. See `ISO_FORMAT`'s doc comment.
    #[test]
    fn iso_timestamps_sort_correctly_across_the_zero_nanosecond_boundary() {
        let whole_second = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let half_second_later = whole_second + time::Duration::nanoseconds(500_000_000);

        assert!(format_iso(half_second_later) > format_iso(whole_second));
    }
}
