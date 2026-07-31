-- Simple key/value settings storage -- first user: the two remappable
-- global hotkeys (capture, screenshot). See
-- docs/superpowers/specs/2026-07-31-capture-list-v2-design.md's
-- "Custom Shortcuts".
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
