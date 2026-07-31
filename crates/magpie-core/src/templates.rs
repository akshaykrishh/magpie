use std::collections::HashMap;

use rusqlite::{params, OptionalExtension};

use crate::db::now_iso;
use crate::error::{Error, Result};
use crate::model::{Capture, Template};
use crate::placeholders::{extract_variables, substitute_variables};
use crate::Store;

pub(crate) const TEMPLATE_COLUMNS: &str =
    "id, title, body, created_at, description, variables_json, pack_id, section_id, deleted_at";

pub(crate) fn template_from_row(row: &rusqlite::Row) -> rusqlite::Result<Template> {
    Ok(Template {
        id: row.get("id")?,
        title: row.get("title")?,
        body: row.get("body")?,
        created_at: row.get("created_at")?,
        description: row.get("description")?,
        variables_json: row.get("variables_json")?,
        pack_id: row.get("pack_id")?,
        section_id: row.get("section_id")?,
        deleted_at: row.get("deleted_at")?,
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
            let sql = format!("SELECT {TEMPLATE_COLUMNS} FROM templates ORDER BY created_at DESC");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], template_from_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Copies a template's body into a new capture, promoted straight into
    /// the given project's Now. The template itself is untouched and stays
    /// in the library -- "Run on nexa-erp" must not consume the template,
    /// since the whole point is running the same prompt again elsewhere
    /// later (see docs/design.md "one stream, one working set"). Equivalent
    /// to `instantiate_template_with_values` with no values supplied --
    /// harmless for a template with no `{{placeholders}}`, since
    /// substitution with nothing to substitute is a no-op.
    pub fn instantiate_template(
        &self,
        template_id: i64,
        project_id: Option<i64>,
    ) -> Result<Capture> {
        self.instantiate_template_with_values(template_id, project_id, &HashMap::new())
    }

    /// Same as `instantiate_template`, but fills in the template's
    /// `{{name}}` placeholders from `values` first. A placeholder with no
    /// entry in `values` is left literal in the resulting capture -- see
    /// `placeholders::substitute_variables`.
    pub fn instantiate_template_with_values(
        &self,
        template_id: i64,
        project_id: Option<i64>,
        values: &HashMap<String, String>,
    ) -> Result<Capture> {
        let template = self.get_template(template_id)?;
        let body = substitute_variables(&template.body, values);
        let capture = self.capture(&body, None)?;
        self.assign_project(capture.id, project_id)?;
        self.promote(capture.id)
    }

    /// The `{{name}}` placeholders a template's body references -- what a
    /// fill-in form needs to ask for before instantiating it.
    pub fn template_variables(&self, template_id: i64) -> Result<Vec<String>> {
        let template = self.get_template(template_id)?;
        Ok(extract_variables(&template.body))
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
    fn template_variables_lists_placeholders_in_the_body() {
        let store = Store::open_in_memory().unwrap();
        let t = store
            .create_template(
                "Fix a test",
                "Investigate why {{test_name}} fails in {{package}}",
            )
            .unwrap();
        assert_eq!(
            store.template_variables(t.id).unwrap(),
            vec!["test_name", "package"]
        );
    }

    #[test]
    fn instantiate_with_values_substitutes_placeholders() {
        let store = Store::open_in_memory().unwrap();
        let t = store
            .create_template("Fix a test", "Investigate why {{test_name}} fails")
            .unwrap();

        let mut values = HashMap::new();
        values.insert("test_name".to_string(), "test_login_flow".to_string());
        let capture = store
            .instantiate_template_with_values(t.id, None, &values)
            .unwrap();

        assert_eq!(capture.body, "Investigate why test_login_flow fails");
        // The template itself keeps its placeholder, unrendered -- only the
        // instantiated capture gets the filled-in text.
        assert_eq!(
            store.get_template(t.id).unwrap().body,
            "Investigate why {{test_name}} fails"
        );
    }

    #[test]
    fn instantiate_without_values_leaves_placeholders_literal() {
        let store = Store::open_in_memory().unwrap();
        let t = store
            .create_template("Fix a test", "Investigate why {{test_name}} fails")
            .unwrap();

        let capture = store.instantiate_template(t.id, None).unwrap();
        assert_eq!(capture.body, "Investigate why {{test_name}} fails");
    }

    #[test]
    fn create_update_delete_round_trip() {
        let store = Store::open_in_memory().unwrap();
        let t = store
            .create_template("Review", "Review this diff for bugs")
            .unwrap();
        assert_eq!(t.title, "Review");

        let updated = store
            .update_template(t.id, "Review v2", "Review strictly")
            .unwrap();
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
        let t = store
            .create_template("Security review", "Look for exploitable issues")
            .unwrap();

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
        let t = store
            .create_template("Update changelog", "Update CHANGELOG.md")
            .unwrap();

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

    #[test]
    fn new_templates_default_to_no_section_and_not_deleted() {
        let store = Store::open_in_memory().unwrap();
        let t = store.create_template("title", "body").unwrap();
        assert_eq!(t.section_id, None);
        assert_eq!(t.deleted_at, None);
    }
}
