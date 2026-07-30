use rusqlite::{params, OptionalExtension};

use crate::captures::{capture_from_row, CAPTURE_COLUMNS};
use crate::error::Result;
use crate::model::Tag;
use crate::Store;

impl Store {
    /// Find-or-create by name, then attach to the capture. Idempotent --
    /// tagging the same capture with the same name twice is a no-op.
    pub fn add_tag(&self, capture_id: i64, name: &str) -> Result<Tag> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO tags (name) VALUES (?1) ON CONFLICT (name) DO NOTHING",
                params![name],
            )?;
            let tag_id: i64 =
                conn.query_row("SELECT id FROM tags WHERE name = ?1", params![name], |r| {
                    r.get(0)
                })?;
            conn.execute(
                "INSERT INTO capture_tags (capture_id, tag_id) VALUES (?1, ?2)
                 ON CONFLICT (capture_id, tag_id) DO NOTHING",
                params![capture_id, tag_id],
            )?;
            Ok(Tag {
                id: tag_id,
                name: name.to_string(),
            })
        })
    }

    pub fn remove_tag(&self, capture_id: i64, name: &str) -> Result<()> {
        self.with_conn(|conn| {
            let tag_id: Option<i64> = conn
                .query_row("SELECT id FROM tags WHERE name = ?1", params![name], |r| {
                    r.get(0)
                })
                .optional()?;
            if let Some(tag_id) = tag_id {
                conn.execute(
                    "DELETE FROM capture_tags WHERE capture_id = ?1 AND tag_id = ?2",
                    params![capture_id, tag_id],
                )?;
            }
            Ok(())
        })
    }

    pub fn list_tags_for_capture(&self, capture_id: i64) -> Result<Vec<Tag>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT t.id, t.name FROM tags t
                 JOIN capture_tags ct ON ct.tag_id = t.id
                 WHERE ct.capture_id = ?1
                 ORDER BY t.name COLLATE NOCASE",
            )?;
            let rows = stmt.query_map(params![capture_id], |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Every capture currently tagged with `name`, most recent first.
    pub fn list_captures_by_tag(&self, name: &str) -> Result<Vec<crate::Capture>> {
        self.with_conn(|conn| {
            let sql = format!(
                "SELECT c.{cols} FROM captures c
                 JOIN capture_tags ct ON ct.capture_id = c.id
                 JOIN tags t ON t.id = ct.tag_id
                 WHERE t.name = ?1
                 ORDER BY c.created_at DESC, c.id DESC",
                cols = CAPTURE_COLUMNS.replace(", ", ", c."),
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![name], capture_from_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_untag_round_trip() {
        let store = Store::open_in_memory().unwrap();
        let c = store.capture("fix the bug", None).unwrap();

        store.add_tag(c.id, "bug").unwrap();
        store.add_tag(c.id, "urgent").unwrap();
        let tags = store.list_tags_for_capture(c.id).unwrap();
        assert_eq!(tags.len(), 2);

        store.remove_tag(c.id, "urgent").unwrap();
        let tags = store.list_tags_for_capture(c.id).unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "bug");
    }

    #[test]
    fn tagging_twice_is_idempotent() {
        let store = Store::open_in_memory().unwrap();
        let c = store.capture("fix the bug", None).unwrap();
        store.add_tag(c.id, "bug").unwrap();
        store.add_tag(c.id, "bug").unwrap();
        assert_eq!(store.list_tags_for_capture(c.id).unwrap().len(), 1);
    }

    #[test]
    fn list_captures_by_tag_finds_only_tagged_ones() {
        let store = Store::open_in_memory().unwrap();
        let a = store.capture("tagged", None).unwrap();
        let _b = store.capture("untagged", None).unwrap();
        store.add_tag(a.id, "refactor").unwrap();

        let found = store.list_captures_by_tag("refactor").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, a.id);
    }
}
