use anyhow::Result;
use rusqlite::Connection;

pub(super) const SCHEMA: &str = r"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS snapshots (
    id            INTEGER PRIMARY KEY,
    sha256        TEXT NOT NULL UNIQUE,
    snapshot_kind TEXT NOT NULL,
    preview_text  TEXT NOT NULL,
    search_text   TEXT NOT NULL,
    item_count    INTEGER NOT NULL,
    total_bytes   INTEGER NOT NULL,
    created_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS snapshot_items (
    snapshot_id   INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    item_index    INTEGER NOT NULL,
    primary_kind  TEXT NOT NULL,
    primary_uti   TEXT,
    preview_text  TEXT NOT NULL,
    search_text   TEXT NOT NULL,
    total_bytes   INTEGER NOT NULL,
    PRIMARY KEY (snapshot_id, item_index)
);

CREATE TABLE IF NOT EXISTS item_representations (
    snapshot_id    INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    item_index     INTEGER NOT NULL,
    uti            TEXT NOT NULL,
    classification TEXT NOT NULL,
    is_text        INTEGER NOT NULL CHECK (is_text IN (0, 1)),
    byte_len       INTEGER NOT NULL,
    raw_sha256     TEXT NOT NULL,
    text_value     TEXT,
    blob_value     BLOB NOT NULL,
    PRIMARY KEY (snapshot_id, item_index, uti)
);

CREATE TABLE IF NOT EXISTS capture_events (
    id                     INTEGER PRIMARY KEY,
    snapshot_id            INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    observed_at            TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    change_count           INTEGER NOT NULL,
    frontmost_app_bundle_id TEXT,
    frontmost_app_name     TEXT
);

CREATE INDEX IF NOT EXISTS idx_capture_events_snapshot_id
    ON capture_events(snapshot_id);

CREATE INDEX IF NOT EXISTS idx_capture_events_observed_at
    ON capture_events(observed_at DESC);

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
";

pub(super) fn configure_connection(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use rusqlite::Connection;

    use super::{configure_connection, SCHEMA};

    #[test]
    fn schema_keeps_fts_index_and_triggers() {
        assert!(SCHEMA.contains("CREATE VIRTUAL TABLE IF NOT EXISTS snapshots_fts"));
        assert!(SCHEMA.contains("CREATE TRIGGER IF NOT EXISTS snapshots_ai"));
        assert!(SCHEMA.contains("CREATE TRIGGER IF NOT EXISTS snapshots_ad"));
        assert!(SCHEMA.contains("CREATE TRIGGER IF NOT EXISTS snapshots_au"));
    }

    #[test]
    fn configure_connection_enables_foreign_keys() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        configure_connection(&conn)?;

        let foreign_keys_enabled: i64 =
            conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
        assert_eq!(foreign_keys_enabled, 1);
        Ok(())
    }
}
