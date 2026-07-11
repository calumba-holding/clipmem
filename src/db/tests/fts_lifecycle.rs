use super::*;

#[test]
fn forget_removes_canonical_fts_content() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    let stored = db.store_capture(&fake_snapshot(1, "forgotten private clipboard token"))?;
    assert_eq!(
        canonical_search_fts_row_counts(&db, stored.snapshot_id())?,
        (1, 1)
    );

    db.forget_snapshot(stored.snapshot_id())?
        .context("stored snapshot should be forgettable")?;

    assert_eq!(
        canonical_search_fts_row_counts(&db, stored.snapshot_id())?,
        (0, 0)
    );
    Ok(())
}
