use rusqlite::{params, OptionalExtension, Row};

use crate::db::now_iso;
use crate::error::{Error, Result};
use crate::model::Project;
use crate::Store;

fn project_from_row(row: &Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get("id")?,
        name: row.get("name")?,
        remote_url: row.get("remote_url")?,
        common_git_dir: row.get("common_git_dir")?,
        created_at: row.get("created_at")?,
        last_active_at: row.get("last_active_at")?,
    })
}

const PROJECT_COLUMNS: &str =
    "id, name, remote_url, common_git_dir, created_at, last_active_at";

/// Bump a project's recency signal -- called whenever a capture is filed
/// into it, so `list_projects_by_recency` reflects "projects I've actually
/// touched lately", not alphabetical order. Not a public `Store` method:
/// it's an internal side effect of filing, never an action a caller takes
/// on its own.
pub(crate) fn touch_project_active_tx(
    conn: &rusqlite::Connection,
    project_id: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE projects SET last_active_at = ?1 WHERE id = ?2",
        params![now_iso(), project_id],
    )?;
    Ok(())
}

impl Store {
    /// Identity is the git remote URL, falling back to the common git dir --
    /// this is what unifies worktrees of the same repo into one project
    /// instead of fragmenting the backlog per checkout (see docs/design.md).
    /// Idempotent: calling this again for the same identity returns the
    /// existing project rather than creating a duplicate.
    pub fn get_or_create_project(
        &self,
        name: &str,
        remote_url: Option<&str>,
        common_git_dir: Option<&str>,
    ) -> Result<Project> {
        self.with_conn(|conn| {
            if let Some(remote_url) = remote_url {
                let sql = format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE remote_url = ?1");
                if let Some(p) = conn
                    .query_row(&sql, params![remote_url], project_from_row)
                    .optional()?
                {
                    touch_project_active_tx(conn, p.id)?;
                    let sql = format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id = ?1");
                    return Ok(conn.query_row(&sql, params![p.id], project_from_row)?);
                }
            } else if let Some(common_git_dir) = common_git_dir {
                let sql =
                    format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE common_git_dir = ?1");
                if let Some(p) = conn
                    .query_row(&sql, params![common_git_dir], project_from_row)
                    .optional()?
                {
                    touch_project_active_tx(conn, p.id)?;
                    let sql = format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id = ?1");
                    return Ok(conn.query_row(&sql, params![p.id], project_from_row)?);
                }
            }

            conn.execute(
                "INSERT INTO projects (name, remote_url, common_git_dir, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![name, remote_url, common_git_dir, now_iso()],
            )?;
            let id = conn.last_insert_rowid();
            touch_project_active_tx(conn, id)?;
            let sql = format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id = ?1");
            Ok(conn.query_row(&sql, params![id], project_from_row)?)
        })
    }

    pub fn get_project(&self, id: i64) -> Result<Project> {
        self.with_conn(|conn| {
            let sql = format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id = ?1");
            conn.query_row(&sql, params![id], project_from_row)
                .optional()?
                .ok_or(Error::ProjectNotFound(id))
        })
    }

    pub fn list_projects(&self) -> Result<Vec<Project>> {
        self.with_conn(|conn| {
            let sql =
                format!("SELECT {PROJECT_COLUMNS} FROM projects ORDER BY name COLLATE NOCASE");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], project_from_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Projects ordered by most-recently-touched first (via
    /// `touch_project_active_tx`), untouched projects last. This is the
    /// ranking the desktop app's capture-filing guess uses -- see
    /// docs/superpowers/plans/2026-07-31-confidence-aware-capture-filing.md.
    pub fn list_projects_by_recency(&self, limit: i64) -> Result<Vec<Project>> {
        self.with_conn(|conn| {
            let sql = format!(
                "SELECT {PROJECT_COLUMNS} FROM projects
                 ORDER BY last_active_at IS NULL, last_active_at DESC, id DESC
                 LIMIT ?1"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![limit], project_from_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_or_create_is_idempotent_by_remote_url() {
        let store = Store::open_in_memory().unwrap();
        let a = store
            .get_or_create_project("magpie", Some("git@github.com:x/magpie.git"), None)
            .unwrap();
        let b = store
            .get_or_create_project("magpie", Some("git@github.com:x/magpie.git"), None)
            .unwrap();
        assert_eq!(a.id, b.id);
    }

    #[test]
    fn worktrees_of_the_same_repo_share_one_project() {
        let store = Store::open_in_memory().unwrap();
        // Two worktrees, same remote, different names passed in (as if
        // derived from different checkout directory names) -- identity
        // should still collapse to one project.
        let a = store
            .get_or_create_project("magpie", Some("git@github.com:x/magpie.git"), None)
            .unwrap();
        let b = store
            .get_or_create_project(
                "magpie-feature-x-worktree",
                Some("git@github.com:x/magpie.git"),
                None,
            )
            .unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(store.list_projects().unwrap().len(), 1);
    }

    #[test]
    fn falls_back_to_common_git_dir_when_no_remote() {
        let store = Store::open_in_memory().unwrap();
        let a = store
            .get_or_create_project("local-only", None, Some("/Users/me/code/local-only/.git"))
            .unwrap();
        let b = store
            .get_or_create_project("local-only", None, Some("/Users/me/code/local-only/.git"))
            .unwrap();
        assert_eq!(a.id, b.id);
    }

    #[test]
    fn different_repos_get_different_projects() {
        let store = Store::open_in_memory().unwrap();
        let a = store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();
        let b = store
            .get_or_create_project("b", Some("git@github.com:x/b.git"), None)
            .unwrap();
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn list_projects_by_recency_orders_touched_projects_first() {
        let store = Store::open_in_memory().unwrap();
        let a = store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();
        let b = store
            .get_or_create_project("b", Some("git@github.com:x/b.git"), None)
            .unwrap();
        // a has the lower id (created first), so an id-descending tie-break
        // alone would rank b first. Re-touch a -- if that actually worked,
        // a (despite its lower id) must now outrank b.
        store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();

        let ranked = store.list_projects_by_recency(10).unwrap();
        assert_eq!(ranked[0].id, a.id);
        assert_eq!(ranked[1].id, b.id);
    }

    #[test]
    fn list_projects_by_recency_respects_limit() {
        let store = Store::open_in_memory().unwrap();
        store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();
        store
            .get_or_create_project("b", Some("git@github.com:x/b.git"), None)
            .unwrap();
        let ranked = store.list_projects_by_recency(1).unwrap();
        assert_eq!(ranked.len(), 1);
    }

    #[test]
    fn newly_created_project_has_a_recency_timestamp() {
        let store = Store::open_in_memory().unwrap();
        let p = store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();
        assert!(p.last_active_at.is_some());
    }
}
