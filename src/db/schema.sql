PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS snapshots (
    id            INTEGER PRIMARY KEY,
    sha256        TEXT NOT NULL UNIQUE,
    snapshot_kind TEXT NOT NULL,
    preview_text  TEXT NOT NULL,
    search_text   TEXT NOT NULL,
    item_count    INTEGER NOT NULL CHECK (item_count >= 0),
    total_bytes   INTEGER NOT NULL CHECK (total_bytes >= 0),
    created_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS snapshot_items (
    snapshot_id   INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    item_index    INTEGER NOT NULL CHECK (item_index >= 0),
    primary_kind  TEXT NOT NULL,
    primary_uti   TEXT,
    preview_text  TEXT NOT NULL,
    search_text   TEXT NOT NULL,
    total_bytes   INTEGER NOT NULL CHECK (total_bytes >= 0),
    PRIMARY KEY (snapshot_id, item_index)
);

CREATE TABLE IF NOT EXISTS item_representations (
    snapshot_id    INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    item_index     INTEGER NOT NULL CHECK (item_index >= 0),
    uti            TEXT NOT NULL,
    kind           TEXT NOT NULL,
    byte_len       INTEGER NOT NULL CHECK (byte_len >= 0),
    raw_sha256     TEXT NOT NULL,
    text_value     TEXT,
    blob_value     BLOB NOT NULL,
    PRIMARY KEY (snapshot_id, item_index, uti)
);

CREATE TABLE IF NOT EXISTS capture_events (
    id                     INTEGER PRIMARY KEY,
    snapshot_id            INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    observed_at            TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    change_count           INTEGER NOT NULL CHECK (change_count >= 0),
    frontmost_app_bundle_id TEXT,
    frontmost_app_name     TEXT
);

CREATE TABLE IF NOT EXISTS snapshot_stats (
    snapshot_id                 INTEGER PRIMARY KEY REFERENCES snapshots(id) ON DELETE CASCADE,
    capture_count               INTEGER NOT NULL CHECK (capture_count >= 0),
    first_observed_at           TEXT NOT NULL,
    last_observed_at            TEXT NOT NULL,
    last_event_id               INTEGER NOT NULL,
    last_frontmost_app_bundle_id TEXT,
    last_frontmost_app_name     TEXT
);

CREATE TABLE IF NOT EXISTS snapshot_projection_cache (
    snapshot_id INTEGER PRIMARY KEY REFERENCES snapshots(id) ON DELETE CASCADE,
    urls        TEXT NOT NULL DEFAULT '',
    file_urls   TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_capture_events_snapshot_id
    ON capture_events(snapshot_id);

CREATE INDEX IF NOT EXISTS idx_capture_events_snapshot_observed_id
    ON capture_events(snapshot_id, observed_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_capture_events_observed_id
    ON capture_events(observed_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_snapshot_stats_last_observed_snapshot
    ON snapshot_stats(last_observed_at DESC, snapshot_id DESC);

CREATE INDEX IF NOT EXISTS idx_snapshot_items_snapshot_id
    ON snapshot_items(snapshot_id, item_index);

CREATE VIRTUAL TABLE IF NOT EXISTS snapshots_fts USING fts5(
    search_text,
    preview_text,
    content='snapshots',
    content_rowid='id',
    tokenize='unicode61'
);

CREATE TRIGGER IF NOT EXISTS snapshots_ai AFTER INSERT ON snapshots BEGIN
    INSERT INTO snapshots_fts(rowid, search_text, preview_text)
    VALUES (new.id, new.search_text, new.preview_text);
END;

CREATE TRIGGER IF NOT EXISTS snapshots_ad AFTER DELETE ON snapshots BEGIN
    INSERT INTO snapshots_fts(snapshots_fts, rowid, search_text, preview_text)
    VALUES ('delete', old.id, old.search_text, old.preview_text);
END;

CREATE TRIGGER IF NOT EXISTS snapshots_au AFTER UPDATE ON snapshots BEGIN
    INSERT INTO snapshots_fts(snapshots_fts, rowid, search_text, preview_text)
    VALUES ('delete', old.id, old.search_text, old.preview_text);
    INSERT INTO snapshots_fts(rowid, search_text, preview_text)
    VALUES (new.id, new.search_text, new.preview_text);
END;

CREATE TRIGGER IF NOT EXISTS capture_events_ai AFTER INSERT ON capture_events BEGIN
    INSERT INTO snapshot_stats (
        snapshot_id,
        capture_count,
        first_observed_at,
        last_observed_at,
        last_event_id,
        last_frontmost_app_bundle_id,
        last_frontmost_app_name
    ) VALUES (
        new.snapshot_id,
        1,
        new.observed_at,
        new.observed_at,
        new.id,
        new.frontmost_app_bundle_id,
        new.frontmost_app_name
    )
    ON CONFLICT(snapshot_id) DO UPDATE SET
        capture_count = snapshot_stats.capture_count + 1,
        first_observed_at = MIN(snapshot_stats.first_observed_at, new.observed_at),
        last_observed_at = CASE
            WHEN new.observed_at > snapshot_stats.last_observed_at
                OR (
                    new.observed_at = snapshot_stats.last_observed_at
                    AND new.id > snapshot_stats.last_event_id
                )
                THEN new.observed_at
            ELSE snapshot_stats.last_observed_at
        END,
        last_event_id = CASE
            WHEN new.observed_at > snapshot_stats.last_observed_at
                OR (
                    new.observed_at = snapshot_stats.last_observed_at
                    AND new.id > snapshot_stats.last_event_id
                )
                THEN new.id
            ELSE snapshot_stats.last_event_id
        END,
        last_frontmost_app_bundle_id = CASE
            WHEN new.observed_at > snapshot_stats.last_observed_at
                OR (
                    new.observed_at = snapshot_stats.last_observed_at
                    AND new.id > snapshot_stats.last_event_id
                )
                THEN new.frontmost_app_bundle_id
            ELSE snapshot_stats.last_frontmost_app_bundle_id
        END,
        last_frontmost_app_name = CASE
            WHEN new.observed_at > snapshot_stats.last_observed_at
                OR (
                    new.observed_at = snapshot_stats.last_observed_at
                    AND new.id > snapshot_stats.last_event_id
                )
                THEN new.frontmost_app_name
            ELSE snapshot_stats.last_frontmost_app_name
        END;
END;

CREATE TRIGGER IF NOT EXISTS capture_events_au
AFTER UPDATE OF observed_at, frontmost_app_bundle_id, frontmost_app_name ON capture_events BEGIN
    DELETE FROM snapshot_stats WHERE snapshot_id = old.snapshot_id;
    INSERT INTO snapshot_stats (
        snapshot_id,
        capture_count,
        first_observed_at,
        last_observed_at,
        last_event_id,
        last_frontmost_app_bundle_id,
        last_frontmost_app_name
    )
    SELECT
        ce.snapshot_id,
        COUNT(*) AS capture_count,
        MIN(ce.observed_at) AS first_observed_at,
        MAX(ce.observed_at) AS last_observed_at,
        (
            SELECT latest.id
            FROM capture_events latest
            WHERE latest.snapshot_id = ce.snapshot_id
            ORDER BY latest.observed_at DESC, latest.id DESC
            LIMIT 1
        ) AS last_event_id,
        (
            SELECT latest.frontmost_app_bundle_id
            FROM capture_events latest
            WHERE latest.snapshot_id = ce.snapshot_id
            ORDER BY latest.observed_at DESC, latest.id DESC
            LIMIT 1
        ) AS last_frontmost_app_bundle_id,
        (
            SELECT latest.frontmost_app_name
            FROM capture_events latest
            WHERE latest.snapshot_id = ce.snapshot_id
            ORDER BY latest.observed_at DESC, latest.id DESC
            LIMIT 1
        ) AS last_frontmost_app_name
    FROM capture_events ce
    WHERE ce.snapshot_id = new.snapshot_id
    GROUP BY ce.snapshot_id;
END;

CREATE TRIGGER IF NOT EXISTS capture_events_ad AFTER DELETE ON capture_events BEGIN
    DELETE FROM snapshot_stats WHERE snapshot_id = old.snapshot_id;
    INSERT INTO snapshot_stats (
        snapshot_id,
        capture_count,
        first_observed_at,
        last_observed_at,
        last_event_id,
        last_frontmost_app_bundle_id,
        last_frontmost_app_name
    )
    SELECT
        ce.snapshot_id,
        COUNT(*) AS capture_count,
        MIN(ce.observed_at) AS first_observed_at,
        MAX(ce.observed_at) AS last_observed_at,
        (
            SELECT latest.id
            FROM capture_events latest
            WHERE latest.snapshot_id = ce.snapshot_id
            ORDER BY latest.observed_at DESC, latest.id DESC
            LIMIT 1
        ) AS last_event_id,
        (
            SELECT latest.frontmost_app_bundle_id
            FROM capture_events latest
            WHERE latest.snapshot_id = ce.snapshot_id
            ORDER BY latest.observed_at DESC, latest.id DESC
            LIMIT 1
        ) AS last_frontmost_app_bundle_id,
        (
            SELECT latest.frontmost_app_name
            FROM capture_events latest
            WHERE latest.snapshot_id = ce.snapshot_id
            ORDER BY latest.observed_at DESC, latest.id DESC
            LIMIT 1
        ) AS last_frontmost_app_name
    FROM capture_events ce
    WHERE ce.snapshot_id = old.snapshot_id
    GROUP BY ce.snapshot_id;
END;

CREATE TRIGGER IF NOT EXISTS item_representations_ai AFTER INSERT ON item_representations BEGIN
    INSERT INTO snapshot_projection_cache (snapshot_id, urls, file_urls)
    VALUES (new.snapshot_id, '', '')
    ON CONFLICT(snapshot_id) DO NOTHING;
    UPDATE snapshot_projection_cache
    SET
        urls = COALESCE((
            SELECT GROUP_CONCAT(text_value, char(31))
            FROM (
                SELECT DISTINCT text_value
                FROM item_representations
                WHERE snapshot_id = new.snapshot_id
                  AND kind = 'url'
                  AND text_value IS NOT NULL
                  AND text_value != ''
                ORDER BY text_value
            )
        ), ''),
        file_urls = COALESCE((
            SELECT GROUP_CONCAT(text_value, char(31))
            FROM (
                SELECT DISTINCT text_value
                FROM item_representations
                WHERE snapshot_id = new.snapshot_id
                  AND kind = 'file_url'
                  AND text_value IS NOT NULL
                  AND text_value != ''
                ORDER BY text_value
            )
        ), '')
    WHERE snapshot_id = new.snapshot_id;
END;

CREATE TRIGGER IF NOT EXISTS item_representations_au AFTER UPDATE ON item_representations BEGIN
    INSERT INTO snapshot_projection_cache (snapshot_id, urls, file_urls)
    VALUES (new.snapshot_id, '', '')
    ON CONFLICT(snapshot_id) DO NOTHING;
    UPDATE snapshot_projection_cache
    SET
        urls = COALESCE((
            SELECT GROUP_CONCAT(text_value, char(31))
            FROM (
                SELECT DISTINCT text_value
                FROM item_representations
                WHERE snapshot_id = old.snapshot_id
                  AND kind = 'url'
                  AND text_value IS NOT NULL
                  AND text_value != ''
                ORDER BY text_value
            )
        ), ''),
        file_urls = COALESCE((
            SELECT GROUP_CONCAT(text_value, char(31))
            FROM (
                SELECT DISTINCT text_value
                FROM item_representations
                WHERE snapshot_id = old.snapshot_id
                  AND kind = 'file_url'
                  AND text_value IS NOT NULL
                  AND text_value != ''
                ORDER BY text_value
            )
        ), '')
    WHERE snapshot_id = old.snapshot_id;
    UPDATE snapshot_projection_cache
    SET
        urls = COALESCE((
            SELECT GROUP_CONCAT(text_value, char(31))
            FROM (
                SELECT DISTINCT text_value
                FROM item_representations
                WHERE snapshot_id = new.snapshot_id
                  AND kind = 'url'
                  AND text_value IS NOT NULL
                  AND text_value != ''
                ORDER BY text_value
            )
        ), ''),
        file_urls = COALESCE((
            SELECT GROUP_CONCAT(text_value, char(31))
            FROM (
                SELECT DISTINCT text_value
                FROM item_representations
                WHERE snapshot_id = new.snapshot_id
                  AND kind = 'file_url'
                  AND text_value IS NOT NULL
                  AND text_value != ''
                ORDER BY text_value
            )
        ), '')
    WHERE snapshot_id = new.snapshot_id;
END;

CREATE TRIGGER IF NOT EXISTS item_representations_ad AFTER DELETE ON item_representations BEGIN
    UPDATE snapshot_projection_cache
    SET
        urls = COALESCE((
            SELECT GROUP_CONCAT(text_value, char(31))
            FROM (
                SELECT DISTINCT text_value
                FROM item_representations
                WHERE snapshot_id = old.snapshot_id
                  AND kind = 'url'
                  AND text_value IS NOT NULL
                  AND text_value != ''
                ORDER BY text_value
            )
        ), ''),
        file_urls = COALESCE((
            SELECT GROUP_CONCAT(text_value, char(31))
            FROM (
                SELECT DISTINCT text_value
                FROM item_representations
                WHERE snapshot_id = old.snapshot_id
                  AND kind = 'file_url'
                  AND text_value IS NOT NULL
                  AND text_value != ''
                ORDER BY text_value
            )
        ), '')
    WHERE snapshot_id = old.snapshot_id;
END;
