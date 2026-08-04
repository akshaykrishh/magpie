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

    /// Soft-delete: hidden immediately, recoverable via `restore_template`
    /// or the Recently Deleted view until the purge sweep hard-deletes it.
    pub fn delete_template(&self, id: i64) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE templates SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
                params![now_iso(), id],
            )?;
            Ok(())
        })
    }

    pub fn restore_template(&self, id: i64) -> Result<Template> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE templates SET deleted_at = NULL WHERE id = ?1",
                params![id],
            )?;
            get_template_tx(conn, id)
        })
    }

    pub fn get_template(&self, id: i64) -> Result<Template> {
        self.with_conn(|conn| get_template_tx(conn, id))
    }

    pub fn list_templates(&self) -> Result<Vec<Template>> {
        self.with_conn(|conn| {
            let sql = format!(
                "SELECT {TEMPLATE_COLUMNS} FROM templates
                 WHERE deleted_at IS NULL ORDER BY created_at DESC"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], template_from_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// `deleted_at IS NOT NULL` is the only filter needed here: unlike
    /// captures, templates have no merge/absorption concept, so there's no
    /// analogous "absorbed row" to exclude. `pack_id` just tags a
    /// template's origin pack -- it doesn't affect visibility here.
    pub fn list_recently_deleted_templates(&self) -> Result<Vec<Template>> {
        self.with_conn(|conn| {
            let sql = format!(
                "SELECT {TEMPLATE_COLUMNS} FROM templates
                 WHERE deleted_at IS NOT NULL ORDER BY deleted_at DESC"
            );
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

    pub fn assign_template_section(&self, id: i64, section_id: Option<i64>) -> Result<Template> {
        self.with_conn(|conn| {
            if let Some(section_id) = section_id {
                conn.query_row(
                    "SELECT id FROM sections WHERE id = ?1 AND deleted_at IS NULL",
                    params![section_id],
                    |_| Ok(()),
                )
                .optional()?
                .ok_or(Error::SectionNotFound(section_id))?;
            }
            conn.execute(
                "UPDATE templates SET section_id = ?1 WHERE id = ?2 AND deleted_at IS NULL",
                params![section_id, id],
            )?;
            // Active-only refetch: a soft-deleted template (or one deleted
            // concurrently between the UPDATE above and this refetch) must
            // fall through to TemplateNotFound rather than Ok(template with
            // an unchanged section_id) -- same bug class as captures.rs's
            // mutators, fixed the same way.
            get_active_template_tx(conn, id)
        })
    }

    pub fn purge_expired_templates(&self, cutoff: &str) -> Result<usize> {
        self.with_conn(|conn| {
            Ok(conn.execute(
                "DELETE FROM templates WHERE deleted_at IS NOT NULL AND deleted_at < ?1",
                params![cutoff],
            )?)
        })
    }

    /// Single-id hard-delete for "Delete Permanently" in the Recently
    /// Deleted view -- see `captures::delete_capture_permanently`'s doc
    /// comment for the full rationale (same shape, applied to templates).
    /// The `deleted_at IS NOT NULL` guard ensures this can only ever remove
    /// a row already soft-deleted, never an active one.
    pub fn delete_template_permanently(&self, id: i64) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM templates WHERE id = ?1 AND deleted_at IS NOT NULL",
                params![id],
            )?;
            Ok(())
        })
    }
}

fn get_template_tx(conn: &rusqlite::Connection, id: i64) -> Result<Template> {
    let sql = format!("SELECT {TEMPLATE_COLUMNS} FROM templates WHERE id = ?1");
    conn.query_row(&sql, params![id], template_from_row)
        .optional()?
        .ok_or(Error::TemplateNotFound(id))
}

/// Same as `get_template_tx`, but treats a soft-deleted row as not-found --
/// see `captures::get_active_capture_tx`'s doc comment for the full
/// rationale (the same convention, applied to templates). Used by
/// `assign_template_section`'s refetch; do NOT use this for `get_template`
/// or `restore_template`, which legitimately need to see a soft-deleted row.
fn get_active_template_tx(conn: &rusqlite::Connection, id: i64) -> Result<Template> {
    let sql =
        format!("SELECT {TEMPLATE_COLUMNS} FROM templates WHERE id = ?1 AND deleted_at IS NULL");
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
        // Soft-delete: the row survives (recoverable via restore_template),
        // it's just hidden from the active list.
        assert!(store.get_template(t.id).unwrap().deleted_at.is_some());
        assert!(store.list_templates().unwrap().is_empty());
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

    #[test]
    fn assign_template_section_round_trips() {
        let store = Store::open_in_memory().unwrap();
        let t = store.create_template("title", "body").unwrap();
        let s = store.create_section("Prompts").unwrap();
        let assigned = store.assign_template_section(t.id, Some(s.id)).unwrap();
        assert_eq!(assigned.section_id, Some(s.id));
    }

    #[test]
    fn delete_template_is_soft_and_restore_reverses_it() {
        let store = Store::open_in_memory().unwrap();
        let t = store.create_template("title", "body").unwrap();

        store.delete_template(t.id).unwrap();
        assert!(store.list_templates().unwrap().is_empty());

        let restored = store.restore_template(t.id).unwrap();
        assert_eq!(restored.deleted_at, None);
        assert_eq!(store.list_templates().unwrap().len(), 1);
    }

    #[test]
    fn assign_template_section_refetch_treats_a_soft_deleted_template_as_not_found() {
        // Same Task-3 re-fetch bug as captures: the UPDATE was already
        // guarded with `AND deleted_at IS NULL`, but the refetch afterward
        // used the unfiltered get_template_tx, so a soft-deleted template's
        // no-op UPDATE would still return Ok(template) with the unchanged
        // section_id instead of TemplateNotFound.
        let store = Store::open_in_memory().unwrap();
        let t = store.create_template("title", "body").unwrap();
        let s = store.create_section("Prompts").unwrap();
        store.delete_template(t.id).unwrap();

        let err = store.assign_template_section(t.id, Some(s.id)).unwrap_err();
        assert!(matches!(err, Error::TemplateNotFound(id) if id == t.id));
    }

    #[test]
    fn delete_template_permanently_only_removes_an_already_soft_deleted_row() {
        let store = Store::open_in_memory().unwrap();
        let active = store.create_template("active", "body").unwrap();
        let deleted = store.create_template("deleted", "body").unwrap();
        store.delete_template(deleted.id).unwrap();

        // Must be a safe no-op on a non-deleted template -- this hard-delete
        // may only ever remove a row that's already soft-deleted.
        store.delete_template_permanently(active.id).unwrap();
        assert!(store.get_template(active.id).is_ok());

        store.delete_template_permanently(deleted.id).unwrap();
        assert!(store.get_template(deleted.id).is_err());
    }

    #[test]
    fn list_recently_deleted_templates_returns_only_deleted_ones() {
        let store = Store::open_in_memory().unwrap();
        let t = store.create_template("title", "body").unwrap();
        store.delete_template(t.id).unwrap();
        let deleted = store.list_recently_deleted_templates().unwrap();
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].id, t.id);
    }
}
