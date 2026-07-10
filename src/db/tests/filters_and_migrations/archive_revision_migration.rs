use super::*;

#[test]
fn schema_version_18_adds_archive_revisions_table() -> Result<()> {
    let path = temp_db_path("archive-revisions-migration");
    let parent = path.parent().expect("temporary path should have a parent");
    std::fs::create_dir_all(parent)?;

    let conn = rusqlite::Connection::open(&path)?;
    conn.execute_batch(
        r"
        CREATE TABLE clipmem_settings (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            paused INTEGER NOT NULL DEFAULT 0 CHECK (paused IN (0, 1)),
            retention_seconds INTEGER CHECK (retention_seconds IS NULL OR retention_seconds >= 0),
            api_key_filter_enabled INTEGER NOT NULL DEFAULT 0 CHECK (api_key_filter_enabled IN (0, 1)),
            ocr_enabled INTEGER NOT NULL DEFAULT 0 CHECK (ocr_enabled IN (0, 1)),
            representation_cache_deferred INTEGER NOT NULL DEFAULT 0 CHECK (representation_cache_deferred IN (0, 1))
        );
        INSERT INTO clipmem_settings (id) VALUES (1);
        PRAGMA user_version = 17;
    ",
    )?;
    drop(conn);

    let db = Database::open_or_init_and_migrate(&path)?;
    let version: i64 = db
        .conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let revision = db.archive_revision()?;

    assert_eq!(version, CURRENT_SCHEMA_VERSION);
    assert_eq!(revision.revision(), 0);
    assert_eq!(revision.last_change_kind(), "initialized");

    cleanup_db(&path);
    Ok(())
}
