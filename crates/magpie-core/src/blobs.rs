use rusqlite::{params, OptionalExtension, Row};

use crate::captures::{get_capture_tx, NewSource};
use crate::db::now_iso;
use crate::error::{Error, Result};
use crate::model::{Blob, Capture};
use crate::Store;

fn blob_from_row(row: &Row) -> rusqlite::Result<Blob> {
    Ok(Blob {
        id: row.get("id")?,
        capture_id: row.get("capture_id")?,
        path: row.get("path")?,
        mime: row.get("mime")?,
        width: row.get("width")?,
        height: row.get("height")?,
        ocr_text: row.get("ocr_text")?,
    })
}

const BLOB_COLUMNS: &str = "id, capture_id, path, mime, width, height, ocr_text";

impl Store {
    /// A screenshot capture: the image lives on disk at `path`, the capture
    /// row that represents it in the stream has an empty body (there's no
    /// user-authored text yet -- OCR fills `blobs.ocr_text` in separately,
    /// once recognition finishes, via `set_blob_ocr_text`). Provenance
    /// works exactly like a text capture.
    pub fn capture_screenshot(
        &self,
        path: &str,
        mime: &str,
        width: Option<i64>,
        height: Option<i64>,
        source: Option<NewSource>,
    ) -> Result<Capture> {
        self.with_conn(|conn| {
            let source_id = match source {
                Some(s) => {
                    conn.execute(
                        "INSERT INTO sources (app_name, bundle_id, window_title, url, captured_at)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![s.app_name, s.bundle_id, s.window_title, s.url, now_iso()],
                    )?;
                    Some(conn.last_insert_rowid())
                }
                None => None,
            };

            conn.execute(
                "INSERT INTO captures (body, created_at, source_id) VALUES ('', ?1, ?2)",
                params![now_iso(), source_id],
            )?;
            let capture_id = conn.last_insert_rowid();

            conn.execute(
                "INSERT INTO blobs (capture_id, path, mime, width, height, ocr_text)
                 VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
                params![capture_id, path, mime, width, height],
            )?;

            get_capture_tx(conn, capture_id)
        })
    }

    pub fn get_blob_for_capture(&self, capture_id: i64) -> Result<Option<Blob>> {
        self.with_conn(|conn| {
            let sql = format!("SELECT {BLOB_COLUMNS} FROM blobs WHERE capture_id = ?1");
            conn.query_row(&sql, params![capture_id], blob_from_row)
                .optional()
                .map_err(Error::from)
        })
    }

    /// Called once OCR recognition finishes for a blob. Firing this on a
    /// screenshot capture's only blob is what makes it show up in search --
    /// see the `blobs_fts_ocr_au` trigger in migrations/0002_screenshots.sql.
    pub fn set_blob_ocr_text(&self, blob_id: i64, text: &str) -> Result<Blob> {
        self.with_conn(|conn| {
            let changed = conn.execute(
                "UPDATE blobs SET ocr_text = ?1 WHERE id = ?2",
                params![text, blob_id],
            )?;
            if changed == 0 {
                return Err(Error::BlobNotFound(blob_id));
            }
            let sql = format!("SELECT {BLOB_COLUMNS} FROM blobs WHERE id = ?1");
            conn.query_row(&sql, params![blob_id], blob_from_row)
                .map_err(Error::from)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_screenshot_creates_a_capture_with_an_attached_blob() {
        let store = Store::open_in_memory().unwrap();
        let capture = store
            .capture_screenshot("/tmp/shot.png", "image/png", Some(1200), Some(800), None)
            .unwrap();

        assert_eq!(capture.body, "");
        let blob = store.get_blob_for_capture(capture.id).unwrap().unwrap();
        assert_eq!(blob.path, "/tmp/shot.png");
        assert_eq!(blob.mime, "image/png");
        assert_eq!(blob.width, Some(1200));
        assert_eq!(blob.height, Some(800));
        assert_eq!(blob.ocr_text, None);
    }

    #[test]
    fn plain_captures_have_no_blob() {
        let store = Store::open_in_memory().unwrap();
        let capture = store.capture("just text", None).unwrap();
        assert!(store.get_blob_for_capture(capture.id).unwrap().is_none());
    }

    #[test]
    fn screenshot_capture_records_its_provenance() {
        let store = Store::open_in_memory().unwrap();
        let capture = store
            .capture_screenshot(
                "/tmp/shot.png",
                "image/png",
                None,
                None,
                Some(NewSource {
                    app_name: Some("Safari".into()),
                    ..Default::default()
                }),
            )
            .unwrap();
        assert!(capture.source_id.is_some());
    }

    #[test]
    fn set_ocr_text_makes_the_screenshot_findable_by_search() {
        let store = Store::open_in_memory().unwrap();
        let capture = store
            .capture_screenshot("/tmp/shot.png", "image/png", None, None, None)
            .unwrap();
        let blob = store.get_blob_for_capture(capture.id).unwrap().unwrap();

        assert!(store.search("invoice", 10).unwrap().is_empty());

        store.set_blob_ocr_text(blob.id, "invoice total 42 dollars").unwrap();

        let results = store.search("invoice", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, capture.id);
    }

    #[test]
    fn set_ocr_text_on_missing_blob_errors() {
        let store = Store::open_in_memory().unwrap();
        let err = store.set_blob_ocr_text(999, "text").unwrap_err();
        assert!(matches!(err, Error::BlobNotFound(999)));
    }

    #[test]
    fn deleting_a_screenshot_capture_removes_its_blob_and_search_entry() {
        let store = Store::open_in_memory().unwrap();
        let capture = store
            .capture_screenshot("/tmp/shot.png", "image/png", None, None, None)
            .unwrap();
        let blob = store.get_blob_for_capture(capture.id).unwrap().unwrap();
        store.set_blob_ocr_text(blob.id, "receipt total").unwrap();

        store.with_conn(|conn| {
            conn.execute("DELETE FROM captures WHERE id = ?1", params![capture.id])?;
            Ok(())
        }).unwrap();

        assert!(store.get_blob_for_capture(capture.id).unwrap().is_none());
        assert!(store.search("receipt", 10).unwrap().is_empty());
    }
}
