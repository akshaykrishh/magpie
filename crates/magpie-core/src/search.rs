use rusqlite::params;

use crate::captures::capture_from_row;
use crate::captures::CAPTURE_COLUMNS;
use crate::error::Result;
use crate::model::Capture;
use crate::Store;

/// Turn free-form user input into a safe FTS5 MATCH expression. FTS5's query
/// syntax treats `"`, `(`, `)`, `-`, `*`, `AND`/`OR`/`NOT`/`NEAR` as
/// operators -- passing a raw search box string straight into MATCH throws
/// a syntax error the moment someone types something like `fix (this` or
/// pastes text containing a stray quote. Instead, split on whitespace and
/// treat every token as a literal phrase (FTS5 string literals escape `"`
/// by doubling it), ANDed together. This can't error on any input short of
/// an empty query, at the cost of not exposing FTS5's operator syntax to
/// users -- an acceptable trade for a capture tool's search box.
///
/// Each phrase gets a trailing `*` *outside* the closing quote, which FTS5
/// documents as a prefix-match marker on the quoted phrase -- the quoting
/// still fully protects against operator injection, since the tokenizer
/// only ever sees the literal contents between the quotes; the `*` is
/// parsed separately, after the string closes. Without this, a live search
/// box feels broken while typing: "term" doesn't match "terminal" until the
/// whole word is typed, since an unmarked phrase is an exact-token match.
fn sanitize_query(query: &str) -> Option<String> {
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|tok| format!("\"{}\"*", tok.replace('"', "\"\"")))
        .collect();
    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" AND "))
    }
}

impl Store {
    /// Full-text search over capture bodies, ranked by relevance (bm25).
    /// Merged-away captures (superseded by a merge) are excluded -- the
    /// merged capture that replaced them is what should surface.
    pub fn search(&self, query: &str, limit: i64) -> Result<Vec<Capture>> {
        let Some(fts_query) = sanitize_query(query) else {
            return Ok(Vec::new());
        };

        self.with_conn(|conn| {
            let sql = format!(
                "SELECT c.{cols} FROM captures c
                 JOIN captures_fts f ON f.rowid = c.id
                 WHERE captures_fts MATCH ?1 AND c.merged_into IS NULL
                 ORDER BY bm25(captures_fts) ASC
                 LIMIT ?2",
                cols = CAPTURE_COLUMNS.replace(", ", ", c."),
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![fts_query, limit], capture_from_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_matching_capture() {
        let store = Store::open_in_memory().unwrap();
        store.capture("the OAuth redirect is broken", None).unwrap();
        store.capture("unrelated note about lunch", None).unwrap();

        let results = store.search("oauth", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].body.contains("OAuth"));
    }

    #[test]
    fn ranks_more_relevant_matches_first() {
        let store = Store::open_in_memory().unwrap();
        store.capture("session timeout session timeout", None).unwrap();
        store.capture("a passing mention of session", None).unwrap();

        let results = store.search("session", 10).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results[0].body.contains("session timeout session timeout"));
    }

    #[test]
    fn multi_word_query_requires_all_terms() {
        let store = Store::open_in_memory().unwrap();
        store.capture("fix the auth bug", None).unwrap();
        store.capture("fix the payment bug", None).unwrap();

        let results = store.search("auth bug", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].body.contains("auth"));
    }

    #[test]
    fn partial_word_matches_as_a_prefix() {
        // A live search box searches on every keystroke -- "term" must
        // match "terminal" immediately, not only once the whole word is
        // typed. Without prefix marking, an unmarked FTS token is an exact-
        // token match, so a partial word would return nothing until it was
        // typed in full.
        let store = Store::open_in_memory().unwrap();
        store.capture("fix the terminal capture bug", None).unwrap();

        let results = store.search("term", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].body.contains("terminal"));
    }

    #[test]
    fn adversarial_input_does_not_error() {
        let store = Store::open_in_memory().unwrap();
        store.capture("normal capture", None).unwrap();

        // Unbalanced quote, bare FTS operators, parens -- all of these
        // would be syntax errors as raw MATCH expressions.
        for q in [
            "fix (this",
            "\"unterminated",
            "AND OR NOT",
            "col:body",
            "* wildcard *",
            "",
        ] {
            store.search(q, 10).unwrap();
        }
    }

    #[test]
    fn merged_away_captures_are_excluded_from_search() {
        let store = Store::open_in_memory().unwrap();
        let a = store.capture("paragraph one about caching", None).unwrap();
        let b = store.capture("paragraph two about caching", None).unwrap();
        store.merge(&[a.id, b.id]).unwrap();

        let results = store.search("caching", 10).unwrap();
        assert_eq!(results.len(), 1, "only the merged capture should surface");
        assert!(results[0].body.contains("paragraph one"));
        assert!(results[0].body.contains("paragraph two"));
    }
}
