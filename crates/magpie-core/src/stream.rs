use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::model::Capture;
use crate::Store;

/// One stream row, joined against everything the redesigned stream needs
/// to render without a per-row IPC round trip: provenance (source app,
/// window title), the `◐ READING TEXT…` OCR-pending state, the project
/// name, a merge count, and the leasing session's label/client. Before
/// this, rendering a row's provenance chip meant a separate
/// `get_capture_blob`/`get_blob_image_data_url` call per screenshot row on
/// mount -- fine at 13 captures, a real N+1 at the design's own 1,284.
///
/// Nests the plain `Capture` rather than flattening its 20-odd fields
/// again -- `row.capture.id`, `row.project_name`, etc.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamRow {
    pub capture: Capture,
    pub source_app_name: Option<String>,
    pub source_window_title: Option<String>,
    pub source_url: Option<String>,
    /// `Some` only when this capture has a screenshot blob -- check this
    /// (not `ocr_text`) to know whether to render a thumbnail at all,
    /// since `ocr_text` is legitimately `None` both for "no blob" and for
    /// "blob exists, OCR hasn't finished yet" (the `◐ READING TEXT…`
    /// state) -- `blob_mime` disambiguates the two.
    pub blob_mime: Option<String>,
    pub blob_width: Option<i64>,
    pub blob_height: Option<i64>,
    pub ocr_text: Option<String>,
    pub project_name: Option<String>,
    /// How many other captures were merged into this one (`MERGED ×N`).
    /// Zero for a capture nothing has ever been merged into, not `None` --
    /// there's no "unknown" reading of this the way there is for
    /// provenance confidence.
    pub merged_count: i64,
    pub session_ordinal: Option<i64>,
    pub session_client: Option<String>,
}

fn stream_row_from_row(row: &Row) -> rusqlite::Result<StreamRow> {
    let capture = Capture {
        id: row.get("id")?,
        kind: row.get("kind")?,
        body: row.get("body")?,
        created_at: row.get("created_at")?,
        done_at: row.get("done_at")?,
        failed_reason: row.get("failed_reason")?,
        queue_pos: row.get("queue_pos")?,
        project_id: row.get("project_id")?,
        branch: row.get("branch")?,
        lease_session: row.get("lease_session")?,
        lease_client: row.get("lease_client")?,
        lease_pid: row.get("lease_pid")?,
        lease_at: row.get("lease_at")?,
        lease_head_commit: row.get("lease_head_commit")?,
        handback_note: row.get("handback_note")?,
        diff_stat: row.get("diff_stat")?,
        handback_at: row.get("handback_at")?,
        source_id: row.get("source_id")?,
        merged_into: row.get("merged_into")?,
        section_id: row.get("section_id")?,
        deleted_at: row.get("deleted_at")?,
        session_id: row.get("session_id")?,
        filed_confidence: row.get("filed_confidence")?,
    };
    Ok(StreamRow {
        capture,
        source_app_name: row.get("app_name")?,
        source_window_title: row.get("window_title")?,
        source_url: row.get("url")?,
        blob_mime: row.get("blob_mime")?,
        blob_width: row.get("blob_width")?,
        blob_height: row.get("blob_height")?,
        ocr_text: row.get("ocr_text")?,
        project_name: row.get("project_name")?,
        merged_count: row.get("merged_count")?,
        session_ordinal: row.get("session_ordinal")?,
        session_client: row.get("session_client")?,
    })
}

const STREAM_ROW_SQL: &str = "
    SELECT c.id, c.kind, c.body, c.created_at, c.done_at, c.failed_reason, c.queue_pos,
           c.project_id, c.branch, c.lease_session, c.lease_client, c.lease_pid, c.lease_at,
           c.lease_head_commit, c.handback_note, c.diff_stat, c.handback_at, c.source_id,
           c.merged_into, c.section_id, c.deleted_at, c.session_id, c.filed_confidence,
           s.app_name, s.window_title, s.url,
           b.mime AS blob_mime, b.width AS blob_width, b.height AS blob_height, b.ocr_text,
           p.name AS project_name,
           (SELECT COUNT(*) FROM captures m WHERE m.merged_into = c.id) AS merged_count,
           ss.ordinal AS session_ordinal, ss.client AS session_client
    FROM captures c
    LEFT JOIN sources  s  ON s.id  = c.source_id
    LEFT JOIN blobs    b  ON b.capture_id = c.id
    LEFT JOIN projects p  ON p.id  = c.project_id
    LEFT JOIN sessions ss ON ss.id = c.session_id
    WHERE c.merged_into IS NULL AND c.deleted_at IS NULL AND (?1 = 0 OR c.project_id IS ?2)
    ORDER BY c.created_at DESC, c.id DESC
    LIMIT ?3 OFFSET ?4
";

impl Store {
    /// The stream, joined for rendering -- same filter/ordering contract
    /// as `list_stream` (`project_id: Some(None)` means Inbox only, `None`
    /// means every project), but returns `StreamRow` instead of bare
    /// `Capture` so a single call feeds provenance chips, the OCR-pending
    /// state, project names, merge counts, and session labels for every
    /// row at once. `list_stream` itself is untouched -- the CLI and
    /// existing tests keep using it, this is additive.
    ///
    /// The blob join assumes one blob per capture, matching the existing
    /// assumption in `get_blob_for_capture` (`query_row`, not
    /// `query_map`) -- a capture can't fan out into duplicate rows here.
    /// The one remaining per-row call after this is
    /// `get_blob_image_data_url` for actual thumbnail bytes, which the UI
    /// gates to on-screen rows via an IntersectionObserver rather than
    /// fetching for every row on mount.
    pub fn list_stream_rows(
        &self,
        project_id: Option<Option<i64>>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<StreamRow>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(STREAM_ROW_SQL)?;
            let filter_on = project_id.is_some();
            let want: Option<i64> = project_id.flatten();
            let rows = stmt.query_map(
                params![filter_on as i64, want, limit, offset],
                stream_row_from_row,
            )?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::captures::NewSource;
    use crate::lease::LeaseIdentity;

    #[test]
    fn plain_capture_has_no_source_or_blob_fields() {
        let store = Store::open_in_memory().unwrap();
        store.capture("hello", None).unwrap();

        let rows = store.list_stream_rows(None, 10, 0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source_app_name, None);
        assert_eq!(rows[0].blob_mime, None);
        assert_eq!(rows[0].project_name, None);
        assert_eq!(rows[0].merged_count, 0);
    }

    #[test]
    fn source_provenance_is_joined() {
        let store = Store::open_in_memory().unwrap();
        store
            .capture(
                "from safari",
                Some(NewSource {
                    app_name: Some("Safari".to_string()),
                    window_title: Some("tool.toml".to_string()),
                    ..Default::default()
                }),
            )
            .unwrap();

        let rows = store.list_stream_rows(None, 10, 0).unwrap();
        assert_eq!(rows[0].source_app_name.as_deref(), Some("Safari"));
        assert_eq!(rows[0].source_window_title.as_deref(), Some("tool.toml"));
    }

    #[test]
    fn screenshot_with_pending_ocr_has_blob_fields_but_no_text() {
        let store = Store::open_in_memory().unwrap();
        store
            .capture_screenshot("/tmp/shot.png", "image/png", Some(1600), Some(900), None)
            .unwrap();

        let rows = store.list_stream_rows(None, 10, 0).unwrap();
        assert_eq!(rows[0].blob_mime.as_deref(), Some("image/png"));
        assert_eq!(rows[0].blob_width, Some(1600));
        assert_eq!(
            rows[0].ocr_text, None,
            "this is the ◐ READING TEXT… state: a blob exists, OCR hasn't finished"
        );
    }

    #[test]
    fn screenshot_after_ocr_carries_the_recognized_text() {
        let store = Store::open_in_memory().unwrap();
        let shot = store
            .capture_screenshot("/tmp/shot.png", "image/png", None, None, None)
            .unwrap();
        let blob = store.get_blob_for_capture(shot.id).unwrap().unwrap();
        store.set_blob_ocr_text(blob.id, "recognized text").unwrap();

        let rows = store.list_stream_rows(None, 10, 0).unwrap();
        assert_eq!(rows[0].ocr_text.as_deref(), Some("recognized text"));
    }

    #[test]
    fn project_name_is_joined_for_filed_captures() {
        let store = Store::open_in_memory().unwrap();
        let proj = store
            .get_or_create_project(
                "magpie-core",
                Some("git@github.com:x/magpie-core.git"),
                None,
            )
            .unwrap();
        let c = store.capture("something", None).unwrap();
        store.assign_project(c.id, Some(proj.id)).unwrap();

        let rows = store.list_stream_rows(None, 10, 0).unwrap();
        assert_eq!(rows[0].project_name.as_deref(), Some("magpie-core"));
    }

    #[test]
    fn merged_count_reflects_how_many_were_folded_in() {
        let store = Store::open_in_memory().unwrap();
        let a = store.capture("a", None).unwrap();
        let b = store.capture("b", None).unwrap();
        let c = store.capture("c", None).unwrap();
        let merged = store.merge(&[a.id, b.id, c.id]).unwrap();

        let rows = store.list_stream_rows(None, 10, 0).unwrap();
        let merged_row = rows.iter().find(|r| r.capture.id == merged.id).unwrap();
        assert_eq!(merged_row.merged_count, 3);

        // The three originals are excluded from the stream entirely
        // (merged_into IS NOT NULL), same as list_stream's contract.
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn session_ordinal_and_client_are_joined_for_agent_captures() {
        let store = Store::open_in_memory().unwrap();
        store.create_session("sess-1", 111, None, None).unwrap();
        store.touch_session_active("sess-1", "claude-code").unwrap();

        store
            .capture_with_session("agent wrote this", None, Some("sess-1"))
            .unwrap();

        let rows = store.list_stream_rows(None, 10, 0).unwrap();
        assert_eq!(rows[0].session_ordinal, Some(1));
        assert_eq!(rows[0].session_client.as_deref(), Some("claude-code"));
    }

    #[test]
    fn filters_to_inbox_only_when_asked() {
        let store = Store::open_in_memory().unwrap();
        let proj = store
            .get_or_create_project("a", Some("git@github.com:x/a.git"), None)
            .unwrap();
        let filed = store.capture("filed", None).unwrap();
        store.assign_project(filed.id, Some(proj.id)).unwrap();
        store.capture("unfiled", None).unwrap();

        let inbox_only = store.list_stream_rows(Some(None), 10, 0).unwrap();
        assert_eq!(inbox_only.len(), 1);
        assert_eq!(inbox_only[0].capture.body, "unfiled");
    }

    #[test]
    fn does_not_fan_out_when_a_capture_has_a_lease() {
        // A regression guard: the join adds `sessions`, and a capture's
        // `session_id` (who captured it) is unrelated to `lease_session`
        // (who currently holds it) -- taking a lease on a plain capture
        // must not somehow duplicate its row via the sessions join.
        let store = Store::open_in_memory().unwrap();
        store.create_session("sess-1", 111, None, None).unwrap();
        let c = store.capture("something", None).unwrap();
        store.promote(c.id).unwrap();
        let identity = LeaseIdentity {
            session: "sess-1".to_string(),
            client: "claude-code".to_string(),
            pid: 111,
        };
        store.queue_take(None, None, &identity).unwrap();

        let rows = store.list_stream_rows(None, 10, 0).unwrap();
        assert_eq!(rows.len(), 1);
    }
}
