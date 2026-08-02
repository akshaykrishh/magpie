-- Four additive columns for the redesign (see the "Slate" UI plan): small,
-- stable session labels; durable session attribution on captures; filing
-- provenance; and audit rows groupable by session. None of these change
-- any existing read path's default behavior -- every new column is
-- nullable and every new index is partial, so a row written before this
-- migration simply carries no opinion, and the UI is responsible for
-- rendering "unknown" rather than fabricating an answer (see the redesign
-- plan's "render as unknown, never invent").

-- Small, stable, per-project session labels (S1, S2, ...). Assigned once
-- at Store::create_session as the smallest slot not already held by a
-- live session in the same project scope (see sessions.rs), so a running
-- session is never renumbered under the user and the dock/strip/activity
-- overlay all show the same number for the same agent for the life of
-- that connection. Slots ARE reused once a session ends (the partial
-- unique index below only constrains *live* sessions), which keeps labels
-- short and stable-for-life at the cost of a slot meaning different
-- sessions on different days -- the activity overlay disambiguates that
-- with a date on non-live groups, not with ever-growing ordinals.
--
-- S0 is reserved for the human and is NEVER stored here -- see the
-- redesign plan's stage 7: a real row for the desktop app would mean
-- Store::end_session dropping a session-digest capture into the user's
-- own stream on every quit, which is wrong. S0 is synthesized in the
-- Tauri layer from the app's own process lifetime instead.
ALTER TABLE sessions ADD COLUMN ordinal INTEGER;

CREATE UNIQUE INDEX sessions_live_ordinal_uq
    ON sessions (coalesce(project_id, -1), ordinal)
    WHERE ended_at IS NULL AND ordinal IS NOT NULL;

-- Which session produced this capture, if any. NULL means "not from an
-- MCP session" -- the hotkey, the CLI, or a typed prompt -- rendered as S0
-- rather than left blank. Durable, unlike `lease_session` (cleared on
-- every lease resolution): this is "who captured it", not "who currently
-- holds it".
--
-- Deliberately no REFERENCES/FK here, matching `lease_session` (see
-- 0001_init.sql) and `lease.rs`'s `LeaseIdentity`: a session identity is
-- treated as a free-floating string everywhere else in this schema, never
-- required to correspond to a live `sessions` row at the moment it's
-- written. An FK here would be the one place enforcing a stricter
-- contract than the rest of the lease/session machinery already keeps.
ALTER TABLE captures ADD COLUMN session_id TEXT;

CREATE INDEX captures_session_id_idx ON captures (session_id) WHERE session_id IS NOT NULL;

-- How this capture came to have its `project_id`. 'certain' means an MCP
-- session's `project::detect()` read an actual git remote (see
-- crates/magpie-mcp/src/project.rs), or a human explicitly chose the
-- project themselves. 'guessed' means the desktop app's recency guess was
-- confirmed by a tap (see apps/desktop/src-tauri/src/capture_flow.rs).
-- NULL means unfiled, OR filed before this column existed -- deliberately
-- never backfilled, since there is no honest way to reconstruct which of
-- the two a pre-existing row was.
ALTER TABLE captures ADD COLUMN filed_confidence TEXT;

-- Lets the activity overlay group by session instead of by time (see the
-- redesign plan's stage 5b/6). NULL for every audit row written before
-- this migration -- those group into an explicit "Earlier" bucket in the
-- UI rather than being attributed to a guess. Independent of `audit.actor`
-- (a client *name* string, kept as-is for the human-readable log), since
-- two concurrent sessions from the same client are otherwise
-- indistinguishable in that column. No FK, same reasoning as
-- `captures.session_id` above.
ALTER TABLE audit ADD COLUMN session_id TEXT;
