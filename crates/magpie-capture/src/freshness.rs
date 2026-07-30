use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

/// Tracks whether newly read text is actually new, or stale content that
/// was already sitting on the clipboard the last time (or ever) this was
/// checked.
///
/// Seeded at construction from whatever's already there, so content that
/// predates the backend even starting is never mistaken for a fresh
/// capture. This is what fixes a real bug found while testing against
/// Terminal.app: without a seeded baseline, unrelated leftover clipboard
/// content -- anything copied at any earlier point, for any reason -- gets
/// reported as a brand new capture the moment the hotkey is next pressed,
/// regardless of whether the user copied anything at all.
pub struct FreshnessTracker {
    last_seen_hash: Mutex<Option<u64>>,
}

impl FreshnessTracker {
    pub fn seeded_with(initial: Option<&str>) -> Self {
        Self {
            last_seen_hash: Mutex::new(initial.filter(|s| !s.is_empty()).map(hash_text)),
        }
    }

    /// `Some(text)` only if it differs from the last text this tracker has
    /// ever seen; `None` otherwise (including for empty/no text -- there's
    /// nothing to capture either way). Only updates the remembered baseline
    /// when returning `Some` -- a `None`/empty read must not erase what's
    /// actually on the clipboard from the tracker's memory.
    pub fn check(&self, text: Option<String>) -> Option<String> {
        let text = text.filter(|t| !t.trim().is_empty())?;
        let hash = hash_text(&text);
        let mut last_seen = self.last_seen_hash.lock().expect("freshness mutex poisoned");
        if *last_seen == Some(hash) {
            return None;
        }
        *last_seen = Some(hash);
        Some(text)
    }
}

fn hash_text(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_content_is_not_treated_as_fresh() {
        let tracker = FreshnessTracker::seeded_with(Some("already here"));
        assert_eq!(
            tracker.check(Some("already here".to_string())),
            None,
            "content present at construction must not read as a fresh capture"
        );
    }

    #[test]
    fn new_content_is_fresh() {
        let tracker = FreshnessTracker::seeded_with(Some("old"));
        assert_eq!(
            tracker.check(Some("new".to_string())),
            Some("new".to_string())
        );
    }

    #[test]
    fn repeating_the_same_capture_is_not_fresh_again() {
        let tracker = FreshnessTracker::seeded_with(None);
        assert_eq!(
            tracker.check(Some("x".to_string())),
            Some("x".to_string())
        );
        assert_eq!(tracker.check(Some("x".to_string())), None);
    }

    #[test]
    fn recopying_after_something_else_is_fresh_again() {
        let tracker = FreshnessTracker::seeded_with(None);
        assert_eq!(tracker.check(Some("x".to_string())), Some("x".to_string()));
        assert_eq!(tracker.check(Some("y".to_string())), Some("y".to_string()));
        assert_eq!(
            tracker.check(Some("x".to_string())),
            Some("x".to_string()),
            "re-copying content seen two captures ago must count as fresh again"
        );
    }

    #[test]
    fn none_and_empty_are_never_fresh_and_dont_disturb_the_baseline() {
        let tracker = FreshnessTracker::seeded_with(Some("baseline"));
        assert_eq!(tracker.check(None), None);
        assert_eq!(tracker.check(Some(String::new())), None);
        assert_eq!(tracker.check(Some("   ".to_string())), None);
        // The baseline must still be intact -- none of the above should
        // have overwritten it with `None`.
        assert_eq!(tracker.check(Some("baseline".to_string())), None);
    }
}
