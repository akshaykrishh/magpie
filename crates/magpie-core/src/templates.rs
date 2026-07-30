use rusqlite::{params, OptionalExtension};

use crate::db::now_iso;
use crate::error::{Error, Result};
use crate::model::{Capture, Template};
use crate::Store;

const TEMPLATE_COLUMNS: &str = "id, title, body, created_at";

fn template_from_row(row: &rusqlite::Row) -> rusqlite::Result<Template> {
    Ok(Template {
        id: row.get("id")?,
        title: row.get("title")?,
        body: row.get("body")?,
        created_at: row.get("created_at")?,
    })
}

impl Store {
    pub fn create_template(&self, title: &str, body: &str) -> Result<Template> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO templates (title, body, created_at) VALUES (?1, ?2, ?3)",
                params![title, body, now_iso()],
            )?;
            let id = conn.last_insert_rowid();
            get_template_tx(conn, id)
        })
    }

    pub fn update_template(&self, id: i64, title: &str, body: &str) -> Result<Template> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE templates SET title = ?1, body = ?2 WHERE id = ?3",
                params![title, body, id],
            )?;
            get_template_tx(conn, id)
        })
    }

    pub fn delete_template(&self, id: i64) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM templates WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    pub fn get_template(&self, id: i64) -> Result<Template> {
        self.with_conn(|conn| get_template_tx(conn, id))
    }

    pub fn list_templates(&self) -> Result<Vec<Template>> {
        self.with_conn(|conn| {
            let sql =
                format!("SELECT {TEMPLATE_COLUMNS} FROM templates ORDER BY created_at DESC");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], template_from_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Copies a template's body into a new capture, promoted straight into
    /// the given project's Now. The template itself is untouched and stays
    /// in the library -- "Run on nexa-erp" must not consume the template,
    /// since the whole point is running the same prompt again elsewhere
    /// later (see docs/design.md "one stream, one working set").
    pub fn instantiate_template(&self, template_id: i64, project_id: Option<i64>) -> Result<Capture> {
        let template = self.get_template(template_id)?;
        let capture = self.capture(&template.body, None)?;
        self.assign_project(capture.id, project_id)?;
        self.promote(capture.id)
    }

    /// Same template, instantiated into several projects at once -- the
    /// "queue this everywhere" motion from docs/design.md's multi-project
    /// section. Each instantiation is independent: one project's copy
    /// failing to promote doesn't roll back the others.
    pub fn instantiate_template_into_many(
        &self,
        template_id: i64,
        project_ids: &[Option<i64>],
    ) -> Result<Vec<Capture>> {
        project_ids
            .iter()
            .map(|&project_id| self.instantiate_template(template_id, project_id))
            .collect()
    }
}

fn get_template_tx(conn: &rusqlite::Connection, id: i64) -> Result<Template> {
    let sql = format!("SELECT {TEMPLATE_COLUMNS} FROM templates WHERE id = ?1");
    conn.query_row(&sql, params![id], template_from_row)
        .optional()?
        .ok_or(Error::TemplateNotFound(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_update_delete_round_trip() {
        let store = Store::open_in_memory().unwrap();
        let t = store.create_template("Review", "Review this diff for bugs").unwrap();
        assert_eq!(t.title, "Review");

        let updated = store.update_template(t.id, "Review v2", "Review strictly").unwrap();
        assert_eq!(updated.body, "Review strictly");

        store.delete_template(t.id).unwrap();
        assert!(matches!(
            store.get_template(t.id).unwrap_err(),
            Error::TemplateNotFound(_)
        ));
    }

    #[test]
    fn instantiate_copies_body_and_promotes_without_consuming_template() {
        let store = Store::open_in_memory().unwrap();
        let project = store
            .get_or_create_project("magpie", Some("git@github.com:x/magpie.git"), None)
            .unwrap();
        let t = store.create_template("Security review", "Look for exploitable issues").unwrap();

        let capture = store.instantiate_template(t.id, Some(project.id)).unwrap();
        assert_eq!(capture.body, "Look for exploitable issues");
        assert!(capture.in_now());
        assert_eq!(capture.project_id, Some(project.id));

        // The template itself is untouched -- instantiating must not
        // consume or modify it, since the point is reusing it again later.
        let still_there = store.get_template(t.id).unwrap();
        assert_eq!(still_there.body, "Look for exploitable issues");
        assert_eq!(store.list_templates().unwrap().len(), 1);
    }

    #[test]
    fn instantiate_into_many_projects_creates_independent_copies() {
        let store = Store::open_in_memory().unwrap();
        let a = store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();
        let b = store
            .get_or_create_project("b", Some("git@github.com:x/b.git"), None)
            .unwrap();
        let t = store.create_template("Update changelog", "Update CHANGELOG.md").unwrap();

        let captures = store
            .instantiate_template_into_many(t.id, &[Some(a.id), Some(b.id)])
            .unwrap();
        assert_eq!(captures.len(), 2);
        assert_ne!(captures[0].id, captures[1].id);

        let now_a = store.list_now(Some(a.id)).unwrap();
        let now_b = store.list_now(Some(b.id)).unwrap();
        assert_eq!(now_a.len(), 1);
        assert_eq!(now_b.len(), 1);
    }
}
