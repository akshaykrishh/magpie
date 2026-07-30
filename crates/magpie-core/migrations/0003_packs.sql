-- Prompt packs: shared, git-hosted collections of templates. A pack is
-- just a git repo with a manifest at its root -- no server, no accounts,
-- no hosting. `source_url` is what re-running an import matches against to
-- decide "these templates already exist, update them" versus "this is a
-- new pack".
CREATE TABLE packs (
    id          INTEGER PRIMARY KEY,
    source_url  TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    description TEXT,
    imported_at TEXT NOT NULL
);

-- description and variables_json are optional metadata a pack manifest can
-- supply per prompt (variables_json holds the manifest's declared
-- {name: {description, default}} map, used to label the fill-in form
-- rather than showing bare {{token}} names) -- templates created directly
-- in the app simply leave both NULL. Variable *values* are never stored
-- here: substitution happens at instantiation time by scanning the body
-- for {{token}} occurrences, so editing a template's body is always the
-- single source of truth for what it needs filled in.
ALTER TABLE templates ADD COLUMN description TEXT;
ALTER TABLE templates ADD COLUMN variables_json TEXT;
ALTER TABLE templates ADD COLUMN pack_id INTEGER REFERENCES packs(id) ON DELETE SET NULL;

CREATE INDEX templates_pack_id_idx ON templates (pack_id);

-- Lets a re-import UPSERT by (pack_id, title) instead of the application
-- doing a read-then-write that could race with a concurrent import.
-- Standalone templates (pack_id NULL) are unconstrained -- nothing stops a
-- user from creating two templates with the same title on their own.
CREATE UNIQUE INDEX templates_pack_id_title_uq
    ON templates (pack_id, title) WHERE pack_id IS NOT NULL;
