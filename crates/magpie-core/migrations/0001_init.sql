-- Identity is the git remote URL, falling back to the common git dir. This
-- unifies worktrees of the same repo into one project instead of fragmenting
-- the backlog per checkout. Partial unique indexes enforce that at the DB
-- level rather than trusting app code to dedupe.
CREATE TABLE projects (
    id             INTEGER PRIMARY KEY,
    name           TEXT NOT NULL,
    remote_url     TEXT,
    common_git_dir TEXT,
    created_at     TEXT NOT NULL
);

CREATE UNIQUE INDEX projects_remote_url_uq
    ON projects (remote_url) WHERE remote_url IS NOT NULL;
CREATE UNIQUE INDEX projects_common_git_dir_uq
    ON projects (common_git_dir) WHERE common_git_dir IS NOT NULL;

CREATE TABLE sources (
    id           INTEGER PRIMARY KEY,
    app_name     TEXT,
    bundle_id    TEXT,
    window_title TEXT,
    url          TEXT,
    captured_at  TEXT NOT NULL
);

-- The single stream + Now working set. queue_pos NULL means "in the stream
-- only"; non-NULL means "promoted into Now" and its value is the sort key.
-- queue_pos is REAL (fractional indexing) so reordering within Now never
-- requires renumbering siblings -- inserting between two items is just
-- picking the midpoint of their positions.
CREATE TABLE captures (
    id            INTEGER PRIMARY KEY,
    body          TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    done_at       TEXT,
    failed_reason TEXT,

    queue_pos     REAL,
    project_id    INTEGER REFERENCES projects(id) ON DELETE SET NULL,
    branch        TEXT,

    -- MCP lease state. No auto-expiry by design (see docs/design.md) --
    -- recovery is stdio-close + dead-pid sweep, both driven from these
    -- columns, never a timer.
    lease_session TEXT,
    lease_client  TEXT,
    lease_pid     INTEGER,
    lease_at      TEXT,

    source_id     INTEGER REFERENCES sources(id) ON DELETE SET NULL,
    merged_into   INTEGER REFERENCES captures(id) ON DELETE SET NULL
);

CREATE INDEX captures_project_id_idx ON captures (project_id);
CREATE INDEX captures_created_at_idx ON captures (created_at DESC);
CREATE INDEX captures_queue_pos_idx
    ON captures (project_id, queue_pos) WHERE queue_pos IS NOT NULL;
CREATE INDEX captures_lease_session_idx
    ON captures (lease_session) WHERE lease_session IS NOT NULL;

-- Persist; instantiated (copied) into a project's Now, never drained
-- themselves. See docs/design.md "one stream, one working set".
CREATE TABLE templates (
    id         INTEGER PRIMARY KEY,
    title      TEXT NOT NULL,
    body       TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE tags (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE capture_tags (
    capture_id INTEGER NOT NULL REFERENCES captures(id) ON DELETE CASCADE,
    tag_id     INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (capture_id, tag_id)
);

-- M4 territory (screenshots + OCR); table exists now so no later migration
-- is needed just to add it.
CREATE TABLE blobs (
    id         INTEGER PRIMARY KEY,
    capture_id INTEGER NOT NULL REFERENCES captures(id) ON DELETE CASCADE,
    path       TEXT NOT NULL,
    mime       TEXT NOT NULL,
    width      INTEGER,
    height     INTEGER,
    ocr_text   TEXT
);

-- Every MCP action, shown in the GUI. See docs/design.md "Agent trust".
CREATE TABLE audit (
    id         INTEGER PRIMARY KEY,
    at         TEXT NOT NULL,
    actor      TEXT NOT NULL,
    action     TEXT NOT NULL,
    capture_id INTEGER REFERENCES captures(id) ON DELETE SET NULL
);

CREATE INDEX audit_at_idx ON audit (at DESC);

-- External-content FTS5 over captures.body, kept in sync by triggers below.
-- OCR text (blobs.ocr_text) joins the index in M4; scoped to body only for
-- now since nothing produces ocr_text yet.
CREATE VIRTUAL TABLE captures_fts USING fts5(
    body,
    content = 'captures',
    content_rowid = 'id'
);

CREATE TRIGGER captures_fts_ai AFTER INSERT ON captures BEGIN
    INSERT INTO captures_fts(rowid, body) VALUES (new.id, new.body);
END;

CREATE TRIGGER captures_fts_ad AFTER DELETE ON captures BEGIN
    INSERT INTO captures_fts(captures_fts, rowid, body) VALUES ('delete', old.id, old.body);
END;

CREATE TRIGGER captures_fts_au AFTER UPDATE OF body ON captures BEGIN
    INSERT INTO captures_fts(captures_fts, rowid, body) VALUES ('delete', old.id, old.body);
    INSERT INTO captures_fts(rowid, body) VALUES (new.id, new.body);
END;
