use rusqlite::{params, OptionalExtension};

use crate::error::Result;
use crate::model::Source;
use crate::Store;

impl Store {
    pub fn get_source(&self, id: i64) -> Result<Option<Source>> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, app_name, bundle_id, window_title, url, captured_at
                 FROM sources WHERE id = ?1",
                params![id],
                |row| {
                    Ok(Source {
                        id: row.get(0)?,
                        app_name: row.get(1)?,
                        bundle_id: row.get(2)?,
                        window_title: row.get(3)?,
                        url: row.get(4)?,
                        captured_at: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{NewSource, Store};

    #[test]
    fn resolves_a_captures_source() {
        let store = Store::open_in_memory().unwrap();
        let c = store
            .capture(
                "hello",
                Some(NewSource {
                    app_name: Some("Cursor".into()),
                    ..Default::default()
                }),
            )
            .unwrap();

        let source = store.get_source(c.source_id.unwrap()).unwrap().unwrap();
        assert_eq!(source.app_name.as_deref(), Some("Cursor"));
    }

    #[test]
    fn missing_source_is_none_not_an_error() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.get_source(999).unwrap().is_none());
    }
}
