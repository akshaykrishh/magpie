use rusqlite::params;

use crate::db::now_iso;
use crate::error::Result;
use crate::model::{Pack, Template};
use crate::templates::{template_from_row, TEMPLATE_COLUMNS};
use crate::Store;

/// What a pack manifest (`magpie.json`) parses into. Reading the manifest
/// and fetching it from git are the CLI layer's job -- this module only
/// knows how to turn already-parsed data into rows.
#[derive(Debug, Clone, Default)]
pub struct ParsedPack {
    pub name: String,
    pub description: Option<String>,
    pub prompts: Vec<ParsedPrompt>,
}

#[derive(Debug, Clone)]
pub struct ParsedPrompt {
    pub title: String,
    pub description: Option<String>,
    pub body: String,
    /// The manifest's declared `{name: {description, default}}` variable
    /// metadata for this prompt, already serialized to JSON -- stored
    /// as-is, never parsed or validated here.
    pub variables_json: Option<String>,
}

fn pack_from_row(row: &rusqlite::Row) -> rusqlite::Result<Pack> {
    Ok(Pack {
        id: row.get("id")?,
        source_url: row.get("source_url")?,
        name: row.get("name")?,
        description: row.get("description")?,
        imported_at: row.get("imported_at")?,
    })
}

const PACK_COLUMNS: &str = "id, source_url, name, description, imported_at";

impl Store {
    /// Imports (or re-imports) a pack: upserts the pack row by
    /// `source_url`, then upserts each prompt as a template keyed by
    /// (pack_id, title). Re-running this against the same `source_url` is
    /// what "pull the pack's latest changes" means -- existing templates
    /// update in place rather than duplicating, since a pack is meant to
    /// be re-imported as its source repo changes, not imported once and
    /// forgotten. The whole import is one transaction: a crash partway
    /// through must never leave the pack row updated with some prompts
    /// imported and others missing.
    pub fn import_pack(
        &self,
        source_url: &str,
        parsed: &ParsedPack,
    ) -> Result<(Pack, Vec<Template>)> {
        self.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;

            tx.execute(
                "INSERT INTO packs (source_url, name, description, imported_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (source_url) DO UPDATE SET
                     name = excluded.name,
                     description = excluded.description,
                     imported_at = excluded.imported_at",
                params![source_url, parsed.name, parsed.description, now_iso()],
            )?;
            let pack_id: i64 = tx.query_row(
                "SELECT id FROM packs WHERE source_url = ?1",
                params![source_url],
                |r| r.get(0),
            )?;

            let mut templates = Vec::with_capacity(parsed.prompts.len());
            for prompt in &parsed.prompts {
                tx.execute(
                    "INSERT INTO templates (title, body, created_at, description, variables_json, pack_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT (pack_id, title) WHERE pack_id IS NOT NULL DO UPDATE SET
                         body = excluded.body,
                         description = excluded.description,
                         variables_json = excluded.variables_json",
                    params![
                        prompt.title,
                        prompt.body,
                        now_iso(),
                        prompt.description,
                        prompt.variables_json,
                        pack_id,
                    ],
                )?;
                let sql =
                    format!("SELECT {TEMPLATE_COLUMNS} FROM templates WHERE pack_id = ?1 AND title = ?2");
                templates.push(tx.query_row(&sql, params![pack_id, prompt.title], template_from_row)?);
            }

            let sql = format!("SELECT {PACK_COLUMNS} FROM packs WHERE id = ?1");
            let pack = tx.query_row(&sql, params![pack_id], pack_from_row)?;

            tx.commit()?;
            Ok((pack, templates))
        })
    }

    pub fn list_packs(&self) -> Result<Vec<Pack>> {
        self.with_conn(|conn| {
            let sql = format!("SELECT {PACK_COLUMNS} FROM packs ORDER BY imported_at DESC");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], pack_from_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn list_pack_templates(&self, pack_id: i64) -> Result<Vec<Template>> {
        self.with_conn(|conn| {
            let sql = format!(
                "SELECT {TEMPLATE_COLUMNS} FROM templates WHERE pack_id = ?1 ORDER BY title"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![pack_id], template_from_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pack() -> ParsedPack {
        ParsedPack {
            name: "Test Pack".to_string(),
            description: Some("A pack for tests".to_string()),
            prompts: vec![
                ParsedPrompt {
                    title: "Fix a test".to_string(),
                    description: Some("Diagnose a failing test".to_string()),
                    body: "Investigate why {{test_name}} fails".to_string(),
                    variables_json: Some(
                        r#"{"test_name":{"description":"which test"}}"#.to_string(),
                    ),
                },
                ParsedPrompt {
                    title: "Update changelog".to_string(),
                    description: None,
                    body: "Update CHANGELOG.md".to_string(),
                    variables_json: None,
                },
            ],
        }
    }

    #[test]
    fn import_creates_pack_and_templates() {
        let store = Store::open_in_memory().unwrap();
        let (pack, templates) = store
            .import_pack("https://example.com/pack.git", &sample_pack())
            .unwrap();

        assert_eq!(pack.name, "Test Pack");
        assert_eq!(templates.len(), 2);
        assert!(templates.iter().all(|t| t.pack_id == Some(pack.id)));

        let listed = store.list_templates().unwrap();
        assert_eq!(listed.len(), 2);
    }

    #[test]
    fn reimporting_the_same_source_updates_instead_of_duplicating() {
        let store = Store::open_in_memory().unwrap();
        let (pack_v1, _) = store
            .import_pack("https://example.com/pack.git", &sample_pack())
            .unwrap();

        let mut updated = sample_pack();
        updated.name = "Test Pack v2".to_string();
        updated.prompts[0].body = "Investigate why {{test_name}} fails in {{package}}".to_string();

        let (pack_v2, templates) = store
            .import_pack("https://example.com/pack.git", &updated)
            .unwrap();

        assert_eq!(
            pack_v1.id, pack_v2.id,
            "re-import must update the same pack row"
        );
        assert_eq!(pack_v2.name, "Test Pack v2");
        assert_eq!(store.list_packs().unwrap().len(), 1);

        // Still exactly two templates -- not four -- and the one that
        // changed picked up its new body.
        assert_eq!(store.list_templates().unwrap().len(), 2);
        let fix_test = templates.iter().find(|t| t.title == "Fix a test").unwrap();
        assert_eq!(
            fix_test.body,
            "Investigate why {{test_name}} fails in {{package}}"
        );
    }

    #[test]
    fn two_different_packs_can_each_have_a_template_with_the_same_title() {
        let store = Store::open_in_memory().unwrap();
        store
            .import_pack("https://example.com/pack-a.git", &sample_pack())
            .unwrap();
        store
            .import_pack("https://example.com/pack-b.git", &sample_pack())
            .unwrap();

        // (pack_id, title) uniqueness is per-pack, not global -- both packs
        // legitimately ship a prompt called "Fix a test".
        assert_eq!(store.list_templates().unwrap().len(), 4);
    }

    #[test]
    fn list_pack_templates_only_returns_that_packs_templates() {
        let store = Store::open_in_memory().unwrap();
        let (pack_a, _) = store
            .import_pack("https://example.com/pack-a.git", &sample_pack())
            .unwrap();
        store
            .import_pack("https://example.com/pack-b.git", &sample_pack())
            .unwrap();

        let a_templates = store.list_pack_templates(pack_a.id).unwrap();
        assert_eq!(a_templates.len(), 2);
        assert!(a_templates.iter().all(|t| t.pack_id == Some(pack_a.id)));
    }

    #[test]
    fn a_pack_template_can_be_instantiated_like_any_other() {
        let store = Store::open_in_memory().unwrap();
        let (_, templates) = store
            .import_pack("https://example.com/pack.git", &sample_pack())
            .unwrap();
        let fix_test = templates.iter().find(|t| t.title == "Fix a test").unwrap();

        assert_eq!(
            store.template_variables(fix_test.id).unwrap(),
            vec!["test_name"]
        );

        let mut values = std::collections::HashMap::new();
        values.insert("test_name".to_string(), "test_login".to_string());
        let capture = store
            .instantiate_template_with_values(fix_test.id, None, &values)
            .unwrap();
        assert_eq!(capture.body, "Investigate why test_login fails");
    }
}
