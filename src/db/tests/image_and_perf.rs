use super::*;

#[test]
fn new_image_captures_start_uncompressed() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    let snapshot = image_snapshot(1, vec![("public.png", b"not actually a png".to_vec())]);
    let stored = db.store_capture(&snapshot)?;

    let status: String = db.conn.query_row(
        "SELECT image_compression_status FROM item_representations WHERE snapshot_id = ?1",
        [stored.snapshot_id()],
        |row| row.get(0),
    )?;

    assert_eq!(status, "uncompressed");
    Ok(())
}

#[test]
fn lossless_webp_optimization_rewrites_image_once_and_preserves_pixels() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    let original = lossless_test_tiff()?;
    let original_pixels = decode_rgba(&original)?;
    let snapshot = image_snapshot(1, vec![("public.tiff", original.clone())]);
    let stored = db.store_capture(&snapshot)?;
    let original_snapshot_sha: String = db.conn.query_row(
        "SELECT sha256 FROM snapshots WHERE id = ?1",
        [stored.snapshot_id()],
        |row| row.get(0),
    )?;

    let representation_count_before: i64 =
        db.conn
            .query_row("SELECT COUNT(*) FROM item_representations", [], |row| {
                row.get(0)
            })?;

    let report = db.optimize_images(false, Some(25), true)?;

    assert_eq!(report.scanned_rows, 1);
    assert_eq!(report.compressed_rows, 1);
    assert_eq!(report.skipped_rows, 0);
    assert_eq!(report.logical_saved_bytes, 0);
    assert!(!report.compact_run);

    let representation_count_after: i64 =
        db.conn
            .query_row("SELECT COUNT(*) FROM item_representations", [], |row| {
                row.get(0)
            })?;
    assert_eq!(representation_count_after, representation_count_before);

    let (uti, status, format, original_len, original_hash, byte_len, raw_hash, blob): RepresentationCompressionRow = db.conn.query_row(
        r"
            SELECT
                uti,
                image_compression_status,
                image_compression_format,
                image_original_byte_len,
                image_original_raw_sha256,
                byte_len,
                raw_sha256,
                blob_value
            FROM item_representations
            WHERE snapshot_id = ?1
        ",
        [stored.snapshot_id()],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
            ))
        },
    )?;
    let optimized_snapshot_sha: String = db.conn.query_row(
        "SELECT sha256 FROM snapshots WHERE id = ?1",
        [stored.snapshot_id()],
        |row| row.get(0),
    )?;
    let total_bytes: i64 = db.conn.query_row(
        "SELECT total_bytes FROM snapshots WHERE id = ?1",
        [stored.snapshot_id()],
        |row| row.get(0),
    )?;

    assert_eq!(uti, "public.tiff");
    assert_eq!(status, "uncompressed");
    assert_eq!(format, None);
    assert_eq!(original_len, None);
    assert_eq!(original_hash, None);
    assert_eq!(byte_len as usize, blob.len());
    assert_eq!(raw_hash, crate::model::hash_bytes(&original));
    assert_eq!(optimized_snapshot_sha, original_snapshot_sha);
    assert_eq!(total_bytes, byte_len);
    assert_eq!(decode_rgba(&blob)?, original_pixels);

    let second_report = db.optimize_images(false, Some(25), true)?;
    assert_eq!(second_report.scanned_rows, 0);
    assert_eq!(second_report.compressed_rows, 0);
    Ok(())
}

#[test]
fn image_optimization_without_limit_processes_every_current_candidate() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    let original = lossless_test_tiff()?;
    let representations = (0..30).map(|_| ("public.tiff", original.clone())).collect();
    let snapshot = image_snapshot(1, representations);
    db.store_capture(&snapshot)?;

    let report = db.optimize_images(false, None, true)?;

    assert_eq!(report.scanned_rows, 1);
    assert_eq!(report.compressed_rows, 1);
    assert_eq!(report.skipped_rows, 0);

    let (compressed, uncompressed): (i64, i64) = db.conn.query_row(
        r"
            SELECT
                SUM(CASE WHEN image_compression_status = 'compressed' THEN 1 ELSE 0 END),
                SUM(CASE WHEN image_compression_status = 'uncompressed' THEN 1 ELSE 0 END)
            FROM item_representations
        ",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    assert_eq!(compressed, 0);
    assert_eq!(uncompressed, 30);
    Ok(())
}

#[test]
fn explicit_image_optimization_limit_still_caps_scanned_rows() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    let original = lossless_test_tiff()?;
    let representations = (0..30).map(|_| ("public.tiff", original.clone())).collect();
    let snapshot = image_snapshot(1, representations);
    db.store_capture(&snapshot)?;

    let report = db.optimize_images(false, Some(25), true)?;

    assert_eq!(report.scanned_rows, 1);
    assert_eq!(report.compressed_rows, 1);

    let uncompressed: i64 = db.conn.query_row(
        "SELECT COUNT(*) FROM item_representations WHERE image_compression_status = 'uncompressed'",
        [],
        |row| row.get(0),
    )?;

    assert_eq!(uncompressed, 30);
    Ok(())
}

#[test]
fn image_optimization_preserves_literal_search_by_app_identity() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    let original = lossless_test_tiff()?;
    let text_rep = build_representation(
        "public.utf8-plain-text".to_string(),
        None,
        b"unique image optimization text".to_vec(),
    );
    let image_rep = build_representation("public.tiff".to_string(), None, original);
    let snapshot = build_snapshot(
        CaptureContext::new(1)
            .with_frontmost_app_name("Safari")
            .with_frontmost_app_bundle_id("com.apple.Safari"),
        vec![build_item(0, vec![text_rep, image_rep])],
    );

    db.store_capture(&snapshot)?;
    assert_eq!(
        db.search_literal("Safari", 10, &unfiltered())?.hits().len(),
        1
    );
    assert_eq!(
        db.search_literal("com.apple.Safari", 10, &unfiltered())?
            .hits()
            .len(),
        1
    );

    let report = db.optimize_images(false, Some(25), true)?;

    assert_eq!(report.compressed_rows, 1);
    assert_eq!(
        db.search_literal("Safari", 10, &unfiltered())?.hits().len(),
        1
    );
    assert_eq!(
        db.search_literal("com.apple.Safari", 10, &unfiltered())?
            .hits()
            .len(),
        1
    );
    Ok(())
}

#[test]
fn file_backed_image_optimization_compacts_without_adding_rows() -> Result<()> {
    let path = temp_db_path("image-optimization-compacts");
    let original = lossless_test_tiff()?;
    let snapshot = image_snapshot(1, vec![("public.tiff", original)]);
    let mut db = Database::open_or_init(&path)?;
    let stored = db.store_capture(&snapshot)?;
    let size_before = super::storage_file_sizes(&path)?.total_bytes();
    let row_count_before: i64 =
        db.conn
            .query_row("SELECT COUNT(*) FROM item_representations", [], |row| {
                row.get(0)
            })?;

    let report = db.optimize_images(false, Some(25), true)?;

    let row_count_after: i64 =
        db.conn
            .query_row("SELECT COUNT(*) FROM item_representations", [], |row| {
                row.get(0)
            })?;
    let size_after = super::storage_file_sizes(&path)?.total_bytes();
    let status: String = db.conn.query_row(
        "SELECT image_compression_status FROM item_representations WHERE snapshot_id = ?1",
        [stored.snapshot_id()],
        |row| row.get(0),
    )?;

    assert_eq!(report.scanned_rows, 1);
    assert_eq!(report.compressed_rows, 1);
    assert!(report.compact_run);
    assert!(report.compact.is_some());
    assert!(!report.compact_recommended);
    assert_eq!(row_count_after, row_count_before);
    assert_eq!(status, "uncompressed");
    assert!(size_after >= size_before);

    cleanup_db(&path);
    Ok(())
}

#[test]
fn no_compact_optimization_leaves_reclaimable_pages_for_later_compaction() -> Result<()> {
    let path = temp_db_path("image-optimization-no-compact");
    let original = lossless_test_tiff()?;
    let snapshot = image_snapshot(1, vec![("public.tiff", original)]);
    let mut db = Database::open_or_init(&path)?;
    db.store_capture(&snapshot)?;

    let first = db.optimize_images(false, Some(25), false)?;
    let freelist_after_rewrite: i64 = db
        .conn
        .query_row("PRAGMA freelist_count", [], |row| row.get(0))?;
    let second = db.optimize_images(false, Some(25), true)?;

    assert_eq!(first.compressed_rows, 1);
    assert!(!first.compact_run);
    assert!(!first.compact_recommended);
    assert_eq!(freelist_after_rewrite, 0);
    assert_eq!(second.scanned_rows, 0);
    assert_eq!(second.compressed_rows, 0);
    assert!(second.compact_run);
    assert!(second.compact.is_some());
    assert!(!second.compact_recommended);

    cleanup_db(&path);
    Ok(())
}

#[test]
fn corrupt_image_rows_are_marked_skipped_once() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    let snapshot = image_snapshot(1, vec![("public.png", b"not actually a png".to_vec())]);
    let stored = db.store_capture(&snapshot)?;

    let report = db.optimize_images(false, Some(25), true)?;
    assert_eq!(report.scanned_rows, 1);
    assert_eq!(report.compressed_rows, 0);
    assert_eq!(report.skipped_rows, 1);

    let (status, reason): (String, Option<String>) = db.conn.query_row(
        "SELECT status, reason FROM representation_derivatives WHERE source_snapshot_id = ?1",
        [stored.snapshot_id()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(status, "skipped");
    assert_eq!(reason.as_deref(), Some("corrupt_or_unsupported"));

    let second_report = db.optimize_images(false, Some(25), true)?;
    assert_eq!(second_report.scanned_rows, 0);
    Ok(())
}

#[test]
fn repeated_explicit_migrate_open_is_idempotent() -> Result<()> {
    let path = temp_db_path("legacy-idempotent");
    let parent = path.parent().expect("temporary path should have a parent");
    std::fs::create_dir_all(parent)?;

    let conn = rusqlite::Connection::open(&path)?;
    conn.execute_batch(
        r"
        CREATE TABLE snapshots (
            id INTEGER PRIMARY KEY,
            sha256 TEXT NOT NULL UNIQUE,
            snapshot_kind TEXT NOT NULL,
            preview_text TEXT NOT NULL,
            search_text TEXT NOT NULL,
            item_count INTEGER NOT NULL,
            total_bytes INTEGER NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE capture_events (
            id INTEGER PRIMARY KEY,
            snapshot_id INTEGER NOT NULL,
            observed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            change_count INTEGER NOT NULL,
            frontmost_app_bundle_id TEXT,
            frontmost_app_name TEXT
        );
        PRAGMA user_version = 0;
    ",
    )?;
    drop(conn);

    drop(Database::open_or_init_and_migrate(&path)?);
    let db = Database::open_or_init_and_migrate(&path)?;
    let version: i64 = db
        .conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))?;

    assert_eq!(version, CURRENT_SCHEMA_VERSION);

    std::fs::remove_file(&path)?;
    Ok(())
}
