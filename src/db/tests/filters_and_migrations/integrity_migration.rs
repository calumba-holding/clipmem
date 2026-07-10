use super::*;

#[test]
fn schema_v20_rejects_orphan_representations_and_cascades_item_deletion() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    let stored = db.store_capture(&fake_snapshot(1, "integrity"))?;
    let orphan = db.conn.execute(
        "INSERT INTO item_representations (snapshot_id,item_index,uti,kind,byte_len,raw_sha256,blob_value) VALUES (?1,99,'public.data','binary',1,'00',x'00')",
        [stored.snapshot_id()],
    );
    assert!(orphan.is_err());
    db.conn.execute(
        "DELETE FROM snapshot_items WHERE snapshot_id=?1 AND item_index=0",
        [stored.snapshot_id()],
    )?;
    let remaining: i64 = db.conn.query_row(
        "SELECT COUNT(*) FROM item_representations WHERE snapshot_id=?1",
        [stored.snapshot_id()],
        |row| row.get(0),
    )?;
    assert_eq!(remaining, 0);
    Ok(())
}

#[test]
fn schema_v20_migration_aborts_with_exact_orphan_report() -> Result<()> {
    let path = temp_db_path("v20-orphan-preflight");
    let snapshot_id = {
        let mut db = Database::open_or_init(&path)?;
        db.store_capture(&fake_snapshot(1, "orphan fixture"))?
            .snapshot_id()
    };
    {
        let conn = rusqlite::Connection::open(&path)?;
        conn.pragma_update(None, "foreign_keys", false)?;
        conn.execute_batch(
            "CREATE TABLE orphan_copy AS SELECT * FROM item_representations WHERE 0;",
        )?;
        conn.execute(
            "INSERT INTO orphan_copy (snapshot_id,item_index,uti,kind,byte_len,raw_sha256,blob_value,image_compression_status) VALUES (?1,99,'public.data','binary',1,'00',x'00','uncompressed')",
            [snapshot_id],
        )?;
        conn.execute(
            "INSERT INTO item_representations SELECT * FROM orphan_copy",
            [],
        )?;
        conn.execute_batch("DROP TABLE orphan_copy; PRAGMA user_version=19;")?;
    }
    let error = match Database::open_or_init_and_migrate(&path) {
        Ok(_) => panic!("orphan migration should fail"),
        Err(error) => error,
    };
    assert!(format!("{error:#}")
        .contains("schema v20 integrity preflight found 1 item representation row(s)"));
    cleanup_db(&path);
    Ok(())
}
