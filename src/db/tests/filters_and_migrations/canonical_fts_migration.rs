use super::*;

#[test]
fn schema_v24_removes_orphan_canonical_fts_rows_and_installs_delete_trigger() -> Result<()> {
    let path = temp_db_path("v24-canonical-fts-lifecycle");
    {
        let mut db = Database::open_or_init(&path)?;
        db.store_capture(&fake_snapshot(1, "retained migration document"))?;
    }
    {
        let conn = rusqlite::Connection::open(&path)?;
        conn.execute("DROP TRIGGER snapshot_search_documents_ad", [])?;
        conn.execute(
            "INSERT INTO snapshot_search_documents_fts (rowid,native_text) VALUES (999,'forgotten secret token')",
            [],
        )?;
        conn.execute(
            "INSERT INTO snapshot_search_documents_literal_fts (rowid,haystack) VALUES (999,'forgotten secret token')",
            [],
        )?;
        conn.pragma_update(None, "user_version", 23)?;
    }

    let db = Database::open_or_init_and_migrate(&path)?;
    assert_eq!(canonical_search_fts_row_counts(&db, 999)?, (0, 0));
    let trigger_count: i64 = db.conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name='snapshot_search_documents_ad'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(trigger_count, 1);
    drop(db);
    cleanup_db(&path);
    Ok(())
}
