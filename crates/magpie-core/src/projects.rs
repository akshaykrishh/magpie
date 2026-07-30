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
    })
}

const PROJECT_COLUMNS: &str = "id, name, remote_url, common_git_dir, created_at";

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
                    return Ok(p);
                }
            } else if let Some(common_git_dir) = common_git_dir {
                let sql =
                    format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE common_git_dir = ?1");
                if let Some(p) = conn
                    .query_row(&sql, params![common_git_dir], project_from_row)
                    .optional()?
                {
                    return Ok(p);
                }
            }

            conn.execute(
                "INSERT INTO projects (name, remote_url, common_git_dir, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![name, remote_url, common_git_dir, now_iso()],
            )?;
            let id = conn.last_insert_rowid();
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
}
