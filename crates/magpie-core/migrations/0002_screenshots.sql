-- Screenshots join the full-text index: captures_fts gains an ocr_text
-- column, sourced from blobs.ocr_text rather than captures itself, since
-- OCR happens asynchronously after the capture row already exists.
--
-- FTS5's external-content 'delete' command requires the exact values
-- currently held in the index (it uses them to locate the postings to
-- remove, not a lookup), so every trigger below re-derives both columns
-- rather than touching just the one that changed -- a partial column-list
-- insert implicitly zeroes the omitted column, which would silently drop
-- half the index on the next write.
--
-- captures_fts_ad moves from AFTER to BEFORE DELETE for the same reason:
-- blobs.capture_id cascades on delete, so by AFTER DELETE the row this
-- trigger needs to read would already be gone. BEFORE DELETE runs strictly
-- before the cascade, so the blob is guaranteed still present.

DROP TRIGGER captures_fts_ai;
DROP TRIGGER captures_fts_ad;
DROP TRIGGER captures_fts_au;
DROP TABLE captures_fts;

CREATE VIRTUAL TABLE captures_fts USING fts5(
    body,
    ocr_text,
    content = 'captures',
    content_rowid = 'id'
);

INSERT INTO captures_fts(rowid, body, ocr_text)
    SELECT id, body, COALESCE(
        (SELECT group_concat(ocr_text, ' ') FROM blobs WHERE capture_id = captures.id),
        ''
    )
    FROM captures;

CREATE TRIGGER captures_fts_ai AFTER INSERT ON captures BEGIN
    INSERT INTO captures_fts(rowid, body, ocr_text) VALUES (new.id, new.body, '');
END;

CREATE TRIGGER captures_fts_ad BEFORE DELETE ON captures BEGIN
    INSERT INTO captures_fts(captures_fts, rowid, body, ocr_text) VALUES (
        'delete', old.id, old.body,
        COALESCE((SELECT group_concat(ocr_text, ' ') FROM blobs WHERE capture_id = old.id), '')
    );
END;

CREATE TRIGGER captures_fts_au AFTER UPDATE OF body ON captures BEGIN
    INSERT INTO captures_fts(captures_fts, rowid, body, ocr_text) VALUES (
        'delete', old.id, old.body,
        COALESCE((SELECT group_concat(ocr_text, ' ') FROM blobs WHERE capture_id = old.id), '')
    );
    INSERT INTO captures_fts(rowid, body, ocr_text) VALUES (
        new.id, new.body,
        COALESCE((SELECT group_concat(ocr_text, ' ') FROM blobs WHERE capture_id = new.id), '')
    );
END;

-- Fires once OCR finishes and writes the result onto its blob row (blobs
-- are inserted with ocr_text NULL; recognition runs after the capture is
-- already visible, so this is the only place that text enters the index).
CREATE TRIGGER blobs_fts_ocr_au AFTER UPDATE OF ocr_text ON blobs BEGIN
    INSERT INTO captures_fts(captures_fts, rowid, body, ocr_text) VALUES (
        'delete', old.capture_id,
        (SELECT body FROM captures WHERE id = old.capture_id), old.ocr_text
    );
    INSERT INTO captures_fts(rowid, body, ocr_text) VALUES (
        new.capture_id,
        (SELECT body FROM captures WHERE id = new.capture_id), new.ocr_text
    );
END;
