use rusqlite::{params, OptionalExtension};

use crate::error::Result;
use crate::Store;

impl Store {
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        self.with_conn(|conn| {
            Ok(conn
                .query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get(0))
                .optional()?)
        })
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::Store;

    #[test]
    fn get_setting_returns_none_when_unset() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.get_setting("capture_hotkey").unwrap(), None);
    }

    #[test]
    fn set_then_get_round_trips_and_overwrites() {
        let store = Store::open_in_memory().unwrap();
        store.set_setting("capture_hotkey", "CommandOrControl+Shift+M").unwrap();
        assert_eq!(
            store.get_setting("capture_hotkey").unwrap(),
            Some("CommandOrControl+Shift+M".to_string())
        );
        store.set_setting("capture_hotkey", "CommandOrControl+Shift+K").unwrap();
        assert_eq!(
            store.get_setting("capture_hotkey").unwrap(),
            Some("CommandOrControl+Shift+K".to_string())
        );
    }
}
