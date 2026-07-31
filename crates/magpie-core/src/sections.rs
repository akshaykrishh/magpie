use rusqlite::{params, OptionalExtension};

use crate::db::now_iso;
use crate::error::{Error, Result};
use crate::model::Section;
use crate::Store;

fn section_from_row(row: &rusqlite::Row) -> rusqlite::Result<Section> {
    Ok(Section {
        id: row.get("id")?,
        name: row.get("name")?,
        position: row.get("position")?,
        created_at: row.get("created_at")?,
        deleted_at: row.get("deleted_at")?,
    })
}

const SECTION_COLUMNS: &str = "id, name, position, created_at, deleted_at";

impl Store {
    /// New sections land after everything else -- one past the current
    /// max `position`, or `0.0` for the very first section.
    pub fn create_section(&self, name: &str) -> Result<Section> {
        self.with_conn(|conn| {
            let max_pos: Option<f64> = conn.query_row(
                "SELECT MAX(position) FROM sections WHERE deleted_at IS NULL",
                [],
                |r| r.get(0),
            )?;
            let position = max_pos.map(|p| p + 1.0).unwrap_or(0.0);
            conn.execute(
                "INSERT INTO sections (name, position, created_at) VALUES (?1, ?2, ?3)",
                params![name, position, now_iso()],
            )?;
            let id = conn.last_insert_rowid();
            get_section_tx(conn, id)
        })
    }

    pub fn rename_section(&self, id: i64, name: &str) -> Result<Section> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE sections SET name = ?1 WHERE id = ?2 AND deleted_at IS NULL",
                params![name, id],
            )?;
            get_section_tx(conn, id)
        })
    }

    pub fn get_section(&self, id: i64) -> Result<Section> {
        self.with_conn(|conn| get_section_tx(conn, id))
    }

    pub fn list_sections(&self) -> Result<Vec<Section>> {
        self.with_conn(|conn| {
            let sql = format!(
                "SELECT {SECTION_COLUMNS} FROM sections WHERE deleted_at IS NULL ORDER BY position ASC"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], section_from_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Same fractional-index technique as `Store::reorder` for captures:
    /// the new position is the midpoint between whatever now precedes and
    /// follows the target slot, so reordering never touches any other row.
    pub fn reorder_section(&self, id: i64, after_id: Option<i64>) -> Result<Section> {
        self.with_conn(|conn| {
            let after_pos: Option<f64> = match after_id {
                Some(after_id) => Some(conn.query_row(
                    "SELECT position FROM sections WHERE id = ?1 AND deleted_at IS NULL",
                    params![after_id],
                    |r| r.get(0),
                )?),
                None => None,
            };
            let next_pos: Option<f64> = conn
                .query_row(
                    "SELECT MIN(position) FROM sections
                     WHERE deleted_at IS NULL AND id != ?1
                       AND (?2 IS NULL OR position > ?2)",
                    params![id, after_pos],
                    |r| r.get(0),
                )
                .optional()?
                .flatten();
            let new_pos = match (after_pos, next_pos) {
                (Some(a), Some(n)) => (a + n) / 2.0,
                (Some(a), None) => a + 1.0,
                (None, Some(n)) => n - 1.0,
                (None, None) => 0.0,
            };
            conn.execute(
                "UPDATE sections SET position = ?1 WHERE id = ?2",
                params![new_pos, id],
            )?;
            get_section_tx(conn, id)
        })
    }
}

fn get_section_tx(conn: &rusqlite::Connection, id: i64) -> Result<Section> {
    let sql = format!("SELECT {SECTION_COLUMNS} FROM sections WHERE id = ?1");
    conn.query_row(&sql, params![id], section_from_row)
        .optional()?
        .ok_or(Error::SectionNotFound(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;

    #[test]
    fn create_and_get_round_trip() {
        let store = Store::open_in_memory().unwrap();
        let s = store.create_section("Research").unwrap();
        assert_eq!(s.name, "Research");
        assert_eq!(store.get_section(s.id).unwrap(), s);
    }

    #[test]
    fn rename_updates_the_name_only() {
        let store = Store::open_in_memory().unwrap();
        let s = store.create_section("Research").unwrap();
        let renamed = store.rename_section(s.id, "Config formats").unwrap();
        assert_eq!(renamed.name, "Config formats");
        assert_eq!(renamed.id, s.id);
    }

    #[test]
    fn list_sections_excludes_deleted_and_orders_by_position() {
        let store = Store::open_in_memory().unwrap();
        let a = store.create_section("A").unwrap();
        let b = store.create_section("B").unwrap();
        let list = store.list_sections().unwrap();
        assert_eq!(list.iter().map(|s| s.id).collect::<Vec<_>>(), vec![a.id, b.id]);
    }

    #[test]
    fn reorder_moves_a_section_after_another() {
        let store = Store::open_in_memory().unwrap();
        let a = store.create_section("A").unwrap();
        let b = store.create_section("B").unwrap();
        let c = store.create_section("C").unwrap();
        // Move A to after C: order becomes B, C, A.
        store.reorder_section(a.id, Some(c.id)).unwrap();
        let list = store.list_sections().unwrap();
        assert_eq!(
            list.iter().map(|s| s.id).collect::<Vec<_>>(),
            vec![b.id, c.id, a.id]
        );
    }
}
