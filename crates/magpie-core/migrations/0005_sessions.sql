-- One row per MCP connection (see crates/magpie-mcp/src/lib.rs's
-- MagpieServer::new / serve_stdio) -- what used to be an in-memory-only
-- UUID string now persists long enough for a future UI to show "who's
-- doing what". `client` starts NULL: MCP's clientInfo handshake result
-- isn't available until the first tool call, not at connection time (see
-- Store::touch_session_active). No expiry column on purpose -- sessions
-- end via liveness (graceful stdio-close or the dead-pid sweep), never a
-- timer; see docs/design.md "MCP contract".
CREATE TABLE sessions (
    id              TEXT PRIMARY KEY,
    client          TEXT,
    pid             INTEGER NOT NULL,
    project_id      INTEGER REFERENCES projects(id) ON DELETE SET NULL,
    branch          TEXT,
    started_at      TEXT NOT NULL,
    last_active_at  TEXT,
    ended_at        TEXT,
    leased_count    INTEGER NOT NULL DEFAULT 0,
    completed_count INTEGER NOT NULL DEFAULT 0,
    failed_count    INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX sessions_project_id_idx ON sessions (project_id);
CREATE INDEX sessions_active_idx ON sessions (id) WHERE ended_at IS NULL;
