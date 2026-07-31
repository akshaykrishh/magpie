-- Sections: a lightweight, global, ordered, single-membership grouping for
-- captures and templates -- distinct from tags (many-to-many, unordered).
-- See docs/superpowers/specs/2026-07-31-capture-list-v2-design.md.
CREATE TABLE sections (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    position REAL NOT NULL,
    created_at TEXT NOT NULL,
    deleted_at TEXT
);

ALTER TABLE captures ADD COLUMN section_id INTEGER REFERENCES sections(id);
ALTER TABLE templates ADD COLUMN section_id INTEGER REFERENCES sections(id);

-- Soft-delete: hidden from every view/search immediately, purged after
-- ~30 days. See "Deletion" in the design spec.
ALTER TABLE captures ADD COLUMN deleted_at TEXT;
ALTER TABLE templates ADD COLUMN deleted_at TEXT;

CREATE INDEX captures_section_id_idx ON captures (section_id) WHERE section_id IS NOT NULL;
CREATE INDEX templates_section_id_idx ON templates (section_id) WHERE section_id IS NOT NULL;
CREATE INDEX captures_deleted_at_idx ON captures (deleted_at) WHERE deleted_at IS NOT NULL;
CREATE INDEX templates_deleted_at_idx ON templates (deleted_at) WHERE deleted_at IS NOT NULL;
CREATE INDEX sections_deleted_at_idx ON sections (deleted_at) WHERE deleted_at IS NOT NULL;
