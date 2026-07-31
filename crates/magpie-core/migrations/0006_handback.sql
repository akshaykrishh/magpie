-- A third way to resolve a leased item, alongside done/fail: the agent did
-- something but wants a human to look before it counts as finished. See
-- Store::capture_handback (crates/magpie-core/src/lease.rs) and
-- capture_handback in crates/magpie-mcp/src/lib.rs.
--
-- lease_head_commit is set best-effort right after queue_take succeeds
-- (crates/magpie-mcp/src/lib.rs), and is what a later capture_handback
-- diffs against via `git diff --stat <commit>` -- never a number the agent
-- self-reports. It's cleared everywhere the other lease_* columns already
-- are (capture_complete, capture_fail, capture_handback itself,
-- release_leases_for_session, release_lease), since it's part of the same
-- "who currently holds this, and from when" lease state.
ALTER TABLE captures ADD COLUMN lease_head_commit TEXT;
ALTER TABLE captures ADD COLUMN handback_note TEXT;
ALTER TABLE captures ADD COLUMN diff_stat TEXT;
ALTER TABLE captures ADD COLUMN handback_at TEXT;

ALTER TABLE sessions ADD COLUMN handback_count INTEGER NOT NULL DEFAULT 0;
