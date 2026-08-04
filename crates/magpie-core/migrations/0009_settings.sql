-- Simple key/value settings storage -- first user: the two remappable
-- global hotkeys (capture, screenshot).
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
