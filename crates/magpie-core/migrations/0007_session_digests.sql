-- A session digest is a captures row, not a new table: `kind` distinguishes
-- a synthetic "here's what happened when this session ended" summary
-- (written by Store::end_session) from everything a human or agent
-- actually captured. This is what makes a digest "searchable like any
-- capture" for free -- list_stream, capture_search, and the FTS sync
-- triggers already operate over every captures row with no column
-- filtering, so nothing about those needs to change for a digest to show
-- up in the stream and in search results. It's excluded from Now the same
-- way an un-promoted capture already is: nothing ever calls promote() on
-- a digest, so its queue_pos stays NULL.
ALTER TABLE captures ADD COLUMN kind TEXT NOT NULL DEFAULT 'capture';

-- Set once, when the session ends (see Store::end_session) -- NULL for a
-- still-active session. Counts every non-digest capture created between
-- the session's started_at and its ended_at, regardless of who or what
-- created it (there's no reliable signal to attribute a capture to "the
-- human" vs. "the agent's capture_add" -- both go through the same
-- Store::capture(&body, None) path).
ALTER TABLE sessions ADD COLUMN captures_during_session INTEGER;
ALTER TABLE sessions ADD COLUMN unpromoted_at_end INTEGER;
