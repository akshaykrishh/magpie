use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::model::Capture;
use crate::Store;

/// A capture bundled with its tags for export -- the wire/file format is a
/// superset of the bare `Capture` row so an exported JSON file is actually
/// useful on its own, not just a table dump.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptureExport {
    #[serde(flatten)]
    pub capture: Capture,
    pub tags: Vec<String>,
}

impl Store {
    /// Every active (non-merged-away) capture, most recent first, each with
    /// its tags attached.
    fn export_captures(&self) -> Result<Vec<CaptureExport>> {
        let captures = self.list_stream(None, i64::MAX, 0)?;
        captures
            .into_iter()
            .map(|capture| {
                let tags = self
                    .list_tags_for_capture(capture.id)?
                    .into_iter()
                    .map(|t| t.name)
                    .collect();
                Ok(CaptureExport { capture, tags })
            })
            .collect()
    }

    pub fn export_json(&self) -> Result<String> {
        let captures = self.export_captures()?;
        Ok(serde_json::to_string_pretty(&captures)?)
    }

    pub fn export_markdown(&self) -> Result<String> {
        let captures = self.export_captures()?;
        let mut out = String::new();
        for c in captures {
            out.push_str(&format!("## {}\n\n", c.capture.created_at));
            out.push_str(&c.capture.body);
            out.push_str("\n\n");
            if !c.tags.is_empty() {
                out.push_str(&format!("Tags: {}\n\n", c.tags.join(", ")));
            }
            out.push_str("---\n\n");
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_export_round_trips() {
        let store = Store::open_in_memory().unwrap();
        let c = store.capture("remember this", None).unwrap();
        store.add_tag(c.id, "idea").unwrap();

        let json = store.export_json().unwrap();
        let parsed: Vec<CaptureExport> = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].capture.id, c.id);
        assert_eq!(parsed[0].capture.body, "remember this");
        assert_eq!(parsed[0].tags, vec!["idea".to_string()]);
    }

    #[test]
    fn markdown_export_contains_body_and_tags() {
        let store = Store::open_in_memory().unwrap();
        let c = store.capture("ship it", None).unwrap();
        store.add_tag(c.id, "urgent").unwrap();

        let md = store.export_markdown().unwrap();
        assert!(md.contains("ship it"));
        assert!(md.contains("urgent"));
    }

    #[test]
    fn export_excludes_merged_away_captures() {
        let store = Store::open_in_memory().unwrap();
        let a = store.capture("part one", None).unwrap();
        let b = store.capture("part two", None).unwrap();
        store.merge(&[a.id, b.id]).unwrap();

        let json = store.export_json().unwrap();
        let parsed: Vec<CaptureExport> = serde_json::from_str(&json).unwrap();
        // Only the merged capture should be exported, not its two absorbed
        // originals -- otherwise export would double-count the content.
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].capture.body.contains("part one"));
        assert!(parsed[0].capture.body.contains("part two"));
    }
}
