use super::*;

#[test]
fn purge_uses_last_observed_at_and_dry_run_does_not_delete() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    let old = db.store_capture(&fake_snapshot(1, "old clipboard entry"))?;
    let fresh = db.store_capture(&fake_snapshot(2, "fresh clipboard entry"))?;
    set_event_observed_at(&db, old.event_id(), "2000-01-01 00:00:00")?;
    db.conn.execute(
        "UPDATE snapshots SET created_at = '2000-01-01 00:00:00' WHERE id = ?1",
        [fresh.snapshot_id()],
    )?;
    let dry_run = db.purge_snapshots_older_than(30 * 24 * 60 * 60, true)?;
    assert!(dry_run.dry_run());
    assert_eq!(dry_run.snapshot_count(), 1);
    assert_eq!(dry_run.item_count(), 1);
    assert_eq!(dry_run.representation_count(), 1);
    assert_eq!(dry_run.capture_event_count(), 1);
    assert!(db.find_snapshot(old.snapshot_id(), 10)?.is_some());
    assert!(db.find_snapshot(fresh.snapshot_id(), 10)?.is_some());
    let deleted = db.purge_snapshots_older_than(30 * 24 * 60 * 60, false)?;
    assert!(!deleted.dry_run());
    assert_eq!(deleted.snapshot_count(), 1);
    assert!(db.find_snapshot(old.snapshot_id(), 10)?.is_none());
    assert!(db.find_snapshot(fresh.snapshot_id(), 10)?.is_some());
    assert!(db
        .search_auto("old clipboard", 10, &unfiltered())?
        .hits()
        .is_empty());
    assert_eq!(
        db.search_auto("fresh clipboard", 10, &unfiltered())?
            .hits()
            .len(),
        1
    );
    Ok(())
}

#[test]
fn purge_snapshots_older_than_deletes_orphaned_ocr_results() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    let old = db.store_capture(&image_snapshot(
        1,
        vec![("public.png", b"old-image-bytes".to_vec())],
    ))?;
    let fresh = db.store_capture(&image_snapshot(
        2,
        vec![("public.png", b"fresh-image-bytes".to_vec())],
    ))?;
    for snapshot_id in [old.snapshot_id(), fresh.snapshot_id()] {
        db.enqueue_ocr_for_snapshot(snapshot_id)?;
    }
    for candidate in db.next_ocr_candidates(25, None, false)? {
        db.store_ocr_text(candidate.raw_sha256(), "fake", "fast", "Indexed image text")?;
    }
    set_event_observed_at(&db, old.event_id(), "2000-01-01 00:00:00")?;
    let dry_run = db.purge_snapshots_older_than(30 * 24 * 60 * 60, true)?;
    assert!(dry_run.dry_run());
    assert_eq!(ocr_result_count(&db)?, 2);
    let deleted = db.purge_snapshots_older_than(30 * 24 * 60 * 60, false)?;
    assert!(!deleted.dry_run());
    assert_eq!(deleted.snapshot_count(), 1);
    assert!(db.find_snapshot(old.snapshot_id(), 10)?.is_none());
    assert!(db.find_snapshot(fresh.snapshot_id(), 10)?.is_some());
    assert_eq!(ocr_result_count(&db)?, 1);
    Ok(())
}

fn ocr_result_count(db: &Database) -> Result<i64> {
    Ok(db
        .conn
        .query_row("SELECT COUNT(*) FROM ocr_results", [], |row| row.get(0))?)
}

#[test]
fn search_like_treats_underscore_as_literal() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    db.store_capture(&fake_snapshot(1, "configXtest"))?;
    db.store_capture(&fake_snapshot(2, "config test"))?;
    db.store_capture(&fake_snapshot(3, "config_test"))?;
    let hits = db.search_literal("config_test", 10, &unfiltered())?;
    let previews: Vec<_> = hits
        .hits()
        .iter()
        .map(crate::model::SearchHit::preview_text)
        .collect();
    assert_eq!(previews, vec!["config_test"]);
    Ok(())
}

#[test]
fn search_like_treats_escape_character_as_literal() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    db.store_capture(&fake_snapshot(1, r"logs\2024\archive"))?;
    db.store_capture(&fake_snapshot(2, "logs/2024/archive"))?;
    let hits = db.search_literal(r"logs\2024", 10, &unfiltered())?;
    let previews: Vec<_> = hits
        .hits()
        .iter()
        .map(crate::model::SearchHit::preview_text)
        .collect();
    assert_eq!(previews, vec![r"logs\2024\archive"]);
    Ok(())
}
