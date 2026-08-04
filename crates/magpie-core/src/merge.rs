use rusqlite::params;

use crate::captures::{capture_from_row, get_active_capture_tx, get_capture_tx, CAPTURE_COLUMNS};
use crate::db::now_iso;
use crate::error::{Error, Result};
use crate::model::Capture;
use crate::Store;

impl Store {
    /// Merge several captures into one. The new capture's body is their
    /// bodies concatenated in chronological order (oldest first -- this
    /// matches the "select several paragraphs top to bottom, merge" use
    /// case a merge exists for). Sources are re-pointed via `merged_into`
    /// rather than deleted, so each original's own provenance survives and
    /// stays reachable through `list_merge_sources` -- merging must not
    /// destroy the "captured from X" information that made it worth saving.
    /// The whole operation is one transaction: a crash partway through must
    /// never leave a new capture created without its sources re-pointed, or
    /// vice versa.
    pub fn merge(&self, ids: &[i64]) -> Result<Capture> {
        if ids.len() < 2 {
            return Err(Error::MergeNeedsAtLeastTwo);
        }

        self.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;

            // Active-only fetch: merging a soft-deleted capture (or one
            // deleted between selection and this call, e.g. from another
            // window) must fail with the same CaptureNotFound a genuinely
            // missing id would, not silently absorb a deleted row's body
            // into the new merged capture.
            let mut sources = Vec::with_capacity(ids.len());
            for &id in ids {
                sources.push(get_active_capture_tx(&tx, id)?);
            }
            sources.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));

            let body = sources
                .iter()
                .map(|c| c.body.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");

            tx.execute(
                "INSERT INTO captures (body, created_at) VALUES (?1, ?2)",
                params![body, now_iso()],
            )?;
            let merged_id = tx.last_insert_rowid();

            for &id in ids {
                // Guarded the same way as every other UPDATE in this batch
                // -- defense in depth alongside the active-only fetch above,
                // in case a source's deleted_at changes between the two.
                tx.execute(
                    "UPDATE captures SET merged_into = ?1 WHERE id = ?2 AND deleted_at IS NULL",
                    params![merged_id, id],
                )?;
            }

            let merged = get_capture_tx(&tx, merged_id)?;
            tx.commit()?;
            Ok(merged)
        })
    }

    /// The original captures a merged capture was built from, oldest first
    /// -- each still carries its own provenance.
    pub fn list_merge_sources(&self, merged_capture_id: i64) -> Result<Vec<Capture>> {
        self.with_conn(|conn| {
            let sql = format!(
                "SELECT {CAPTURE_COLUMNS} FROM captures
                 WHERE merged_into = ?1
                 ORDER BY created_at ASC, id ASC"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![merged_capture_id], capture_from_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_concatenates_bodies_in_chronological_order() {
        let store = Store::open_in_memory().unwrap();
        let a = store.capture("first paragraph", None).unwrap();
        let b = store.capture("second paragraph", None).unwrap();
        let c = store.capture("third paragraph", None).unwrap();

        // Select out of order, as a user clicking checkboxes would.
        let merged = store.merge(&[c.id, a.id, b.id]).unwrap();

        assert_eq!(
            merged.body,
            "first paragraph\n\nsecond paragraph\n\nthird paragraph"
        );
    }

    #[test]
    fn merge_preserves_each_childs_own_provenance() {
        let store = Store::open_in_memory().unwrap();
        let a = store
            .capture(
                "from chatgpt",
                Some(crate::NewSource {
                    app_name: Some("ChatGPT".into()),
                    ..Default::default()
                }),
            )
            .unwrap();
        let b = store
            .capture(
                "from claude",
                Some(crate::NewSource {
                    app_name: Some("Claude".into()),
                    ..Default::default()
                }),
            )
            .unwrap();

        let merged = store.merge(&[a.id, b.id]).unwrap();

        let sources = store.list_merge_sources(merged.id).unwrap();
        assert_eq!(sources.len(), 2);
        // Each child keeps its own source_id -- merging must not erase
        // where each piece actually came from.
        assert!(sources.iter().all(|c| c.source_id.is_some()));

        let a_after = store.get_capture(a.id).unwrap();
        let b_after = store.get_capture(b.id).unwrap();
        assert_eq!(a_after.merged_into, Some(merged.id));
        assert_eq!(b_after.merged_into, Some(merged.id));
        // Originals still exist with their bodies intact, just absorbed.
        assert_eq!(a_after.body, "from chatgpt");
        assert_eq!(b_after.body, "from claude");
    }

    #[test]
    fn merged_captures_disappear_from_the_active_stream() {
        let store = Store::open_in_memory().unwrap();
        let a = store.capture("one", None).unwrap();
        let b = store.capture("two", None).unwrap();
        store.merge(&[a.id, b.id]).unwrap();

        let stream = store.list_stream(None, 10, 0).unwrap();
        let ids: Vec<i64> = stream.iter().map(|c| c.id).collect();
        assert!(!ids.contains(&a.id));
        assert!(!ids.contains(&b.id));
    }

    #[test]
    fn merging_fewer_than_two_errors() {
        let store = Store::open_in_memory().unwrap();
        let a = store.capture("solo", None).unwrap();
        let err = store.merge(&[a.id]).unwrap_err();
        assert!(matches!(err, Error::MergeNeedsAtLeastTwo));
    }

    #[test]
    fn merging_a_soft_deleted_source_errors_instead_of_absorbing_it() {
        // Same bug class as promote/mark_done/etc: merge() is a
        // pre-existing mutator this batch newly exposed through the
        // unified action surface without retrofitting the deleted_at
        // guard. A soft-deleted capture must be rejected with
        // CaptureNotFound, not silently merged in.
        let store = Store::open_in_memory().unwrap();
        let a = store.capture("first paragraph", None).unwrap();
        let b = store.capture("second paragraph", None).unwrap();
        store.soft_delete_capture(b.id).unwrap();

        let err = store.merge(&[a.id, b.id]).unwrap_err();
        assert!(matches!(err, Error::CaptureNotFound(id) if id == b.id));

        // No orphaned merge result should have been created either -- the
        // whole operation is one transaction, so the failed lookup must
        // roll back any INSERT that happened before it.
        let a_after = store.get_capture(a.id).unwrap();
        assert_eq!(a_after.merged_into, None);
    }
}
