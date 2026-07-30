-- Recency signal for projects, bumped whenever a capture is filed into one
-- (see Store::touch_project_active_tx). Powers "projects ordered by recency
-- of your own activity, not alphabetically" for the desktop app's
-- capture-filing guess (docs/design.md's dock already describes this
-- ordering for the focused-project list; this extends it to a queryable
-- column instead of being implicit in session state).
ALTER TABLE projects ADD COLUMN last_active_at TEXT;

CREATE INDEX projects_last_active_at_idx
    ON projects (last_active_at DESC) WHERE last_active_at IS NOT NULL;
