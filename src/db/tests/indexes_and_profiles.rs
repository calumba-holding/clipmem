use super::*;
use rusqlite::named_params;

#[test]
fn concurrent_store_capture_reuses_snapshot_and_appends_events() -> Result<()> {
    let path = temp_db_path("concurrent-store");
    let barrier = Arc::new(Barrier::new(2));
    let first_barrier = Arc::clone(&barrier);
    let second_barrier = Arc::clone(&barrier);
    let first_path = path.clone();
    let second_path = path.clone();

    drop(Database::open_or_init(&path)?);

    let first = thread::spawn(move || -> Result<_> {
        let mut db = Database::open_existing(&first_path)?;
        let snapshot = fake_snapshot(1, "git status");
        first_barrier.wait();
        db.store_capture(&snapshot)
    });
    let second = thread::spawn(move || -> Result<_> {
        let mut db = Database::open_existing(&second_path)?;
        let snapshot = fake_snapshot(2, "git status");
        second_barrier.wait();
        db.store_capture(&snapshot)
    });

    let first_store = first.join().expect("first store thread should join")?;
    let second_store = second.join().expect("second store thread should join")?;
    let db = Database::open_existing(&path)?;

    let snapshot_count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM snapshots", [], |row| row.get(0))?;
    let event_count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM capture_events", [], |row| row.get(0))?;

    assert_eq!(snapshot_count, 1);
    assert_eq!(event_count, 2);
    assert_eq!(first_store.snapshot_id(), second_store.snapshot_id());
    assert_ne!(
        first_store.inserted_new_snapshot(),
        second_store.inserted_new_snapshot()
    );

    cleanup_db(&path);
    Ok(())
}

#[test]
fn latest_event_query_uses_compound_capture_events_index() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    db.store_capture(&fake_snapshot(1, "git status"))?;
    db.store_capture(&fake_snapshot(2, "git status"))?;

    let plan = explain_query_plan(
        &db.conn,
        "SELECT id, observed_at FROM capture_events WHERE snapshot_id = ?1 ORDER BY observed_at DESC, id DESC LIMIT ?2",
        &[&1_i64, &10_i64],
    )?;

    assert!(plan
        .iter()
        .all(|detail| !detail.contains("USE TEMP B-TREE FOR ORDER BY")));
    assert!(plan
        .iter()
        .any(|detail| detail.contains("idx_capture_events_snapshot_observed_id")));
    Ok(())
}

#[test]
fn duplicate_capture_event_cache_preserves_distinct_apps() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    let terminal = fake_snapshot_with_app(1, "shared payload", "Terminal", "com.apple.Terminal");
    let safari = fake_snapshot_with_app(2, "shared payload", "Safari", "com.apple.Safari");
    let terminal_again =
        fake_snapshot_with_app(3, "shared payload", "Terminal", "com.apple.Terminal");

    let stored = db.store_capture(&terminal)?;
    db.store_capture(&safari)?;
    db.store_capture(&terminal_again)?;

    let (app_names_lower, bundle_ids_lower): (String, String) = db.conn.query_row(
        "SELECT app_names_lower, bundle_ids_lower
         FROM snapshot_event_filter_cache
         WHERE snapshot_id = ?1",
        [stored.snapshot_id()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    assert_eq!(app_names_lower, "safari\u{1f}terminal");
    assert_eq!(bundle_ids_lower, "com.apple.safari\u{1f}com.apple.terminal");
    Ok(())
}

#[test]
fn ocr_candidate_scan_uses_image_candidate_index() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    db.store_capture(&image_snapshot(1, vec![("public.png", vec![1, 2, 3, 4])]))?;

    let plan = explain_query_plan(
        &db.conn,
        "SELECT DISTINCT ir.raw_sha256
         FROM item_representations ir
         WHERE ir.kind = 'image'
           AND length(ir.blob_value) > 0",
        &[],
    )?;

    assert!(plan
        .iter()
        .any(|detail| detail.contains("idx_item_representations_image_candidates")));
    Ok(())
}

#[test]
fn purge_candidate_scan_uses_snapshot_stats_time_index() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    db.store_capture(&fake_snapshot(1, "expired"))?;

    let plan = explain_query_plan(
        &db.conn,
        "SELECT ss.snapshot_id
         FROM snapshot_stats ss
         WHERE ss.last_observed_at < datetime('now', printf('-%d seconds', ?1))",
        &[&86_400_i64],
    )?;

    assert!(plan
        .iter()
        .any(|detail| detail.contains("idx_snapshot_stats_last_observed_snapshot")));
    Ok(())
}

#[test]
fn recent_since_only_filter_uses_snapshot_stats_time_index() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    let old = db.store_capture(&fake_snapshot(1, "old"))?;
    let recent = db.store_capture(&fake_snapshot(2, "recent"))?;

    set_event_observed_at(&db, old.event_id(), "2026-04-19 08:00:00")?;
    set_event_observed_at(&db, recent.event_id(), "2026-04-21 08:00:00")?;
    db.conn.execute(
        "UPDATE snapshot_stats SET first_observed_at = '2026-04-19 08:00:00', last_observed_at = '2026-04-19 08:00:00' WHERE snapshot_id = ?1",
        [old.snapshot_id()],
    )?;
    db.conn.execute(
        "UPDATE snapshot_stats SET first_observed_at = '2026-04-21 08:00:00', last_observed_at = '2026-04-21 08:00:00' WHERE snapshot_id = ?1",
        [recent.snapshot_id()],
    )?;

    let filters = RetrievalFilters::default().with_since(Some("2026-04-20T00:00:00Z".to_string()));
    let page = db.recent_page(20, &filters, None)?;

    assert_eq!(page.items().len(), 1);
    assert_eq!(page.items()[0].snapshot_id(), recent.snapshot_id());

    let mut stmt = db.conn.prepare(
        "EXPLAIN QUERY PLAN
         SELECT ss.snapshot_id
         FROM snapshot_stats ss
         WHERE ss.last_observed_at >= datetime(:since)
         ORDER BY ss.last_observed_at DESC, ss.snapshot_id DESC
         LIMIT :limit",
    )?;
    let rows = stmt.query_map(
        named_params! {
            ":since": "2026-04-20T00:00:00Z",
            ":limit": 20_i64,
        },
        |row| row.get::<_, String>(3),
    )?;
    let plan = collect_rows(rows)?;

    assert!(plan
        .iter()
        .any(|detail| detail.contains("idx_snapshot_stats_last_observed_snapshot")));
    Ok(())
}

#[test]
fn ocr_status_counts_use_status_and_text_indexes() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    let stored = db.store_capture(&image_snapshot(1, vec![("public.png", vec![1, 2, 3, 4])]))?;
    db.enqueue_ocr_for_snapshot(stored.snapshot_id())?;

    let status_plan = explain_query_plan(
        &db.conn,
        "SELECT COUNT(*) FROM ocr_results WHERE status = 'pending'",
        &[],
    )?;
    let text_plan = explain_query_plan(
        &db.conn,
        "SELECT COUNT(*) FROM snapshot_ocr_cache WHERE ocr_text != ''",
        &[],
    )?;

    assert!(status_plan
        .iter()
        .any(|detail| detail.contains("idx_ocr_results_status")));
    assert!(text_plan
        .iter()
        .any(|detail| detail.contains("idx_snapshot_ocr_cache_text_present")));
    Ok(())
}

#[test]
fn pending_ocr_candidate_query_uses_pending_queue_index() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    db.store_capture(&image_snapshot(1, vec![("public.png", vec![1, 2, 3, 4])]))?;
    db.next_ocr_candidates(25, None, false)?;

    let plan = explain_query_plan(
        &db.conn,
        "SELECT o.raw_sha256
         FROM ocr_results o
         WHERE o.status = 'pending'
         ORDER BY o.updated_at ASC, o.raw_sha256 ASC
         LIMIT ?1",
        &[&25_i64],
    )?;

    assert!(plan
        .iter()
        .any(|detail| detail.contains("idx_ocr_results_pending_queue")));
    assert!(plan
        .iter()
        .all(|detail| !detail.contains("USE TEMP B-TREE FOR ORDER BY")));
    Ok(())
}

#[test]
fn unfiltered_stats_leaderboards_use_ordering_indexes() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    db.store_capture(&fake_snapshot(1, "small"))?;
    let large = db.store_capture(&fake_snapshot(2, "large"))?;
    db.conn.execute(
        "UPDATE snapshots SET total_bytes = 4096 WHERE id = ?1",
        [large.snapshot_id()],
    )?;

    let largest_plan = explain_query_plan(
        &db.conn,
        "SELECT s.id, s.total_bytes
         FROM snapshots s
         JOIN snapshot_stats ss ON ss.snapshot_id = s.id
         ORDER BY s.total_bytes DESC, s.id ASC
         LIMIT ?1",
        &[&5_i64],
    )?;
    let most_captured_plan = explain_query_plan(
        &db.conn,
        "SELECT s.id, ss.capture_count
         FROM snapshots s
         JOIN snapshot_stats ss ON ss.snapshot_id = s.id
         ORDER BY ss.capture_count DESC, s.id ASC
         LIMIT ?1",
        &[&5_i64],
    )?;

    assert!(largest_plan
        .iter()
        .any(|detail| detail.contains("idx_snapshots_total_bytes")));
    assert!(largest_plan
        .iter()
        .all(|detail| !detail.contains("USE TEMP B-TREE FOR ORDER BY")));
    assert!(most_captured_plan
        .iter()
        .any(|detail| detail.contains("idx_snapshot_stats_capture_count")));
    assert!(most_captured_plan
        .iter()
        .all(|detail| !detail.contains("USE TEMP B-TREE FOR ORDER BY")));
    Ok(())
}

#[test]
fn image_optimization_candidate_query_uses_queue_index() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    db.store_capture(&image_snapshot(1, vec![("public.png", vec![1, 2, 3, 4])]))?;

    let plan = explain_query_plan(
        &db.conn,
        "SELECT snapshot_id, item_index, uti, byte_len, raw_sha256, blob_value
         FROM item_representations
         WHERE kind = 'image'
           AND image_compression_status = 'uncompressed'
           AND length(blob_value) > 0
         ORDER BY byte_len DESC, snapshot_id ASC, item_index ASC, uti ASC
         LIMIT ?1",
        &[&25_i64],
    )?;

    assert!(plan
        .iter()
        .any(|detail| detail.contains("idx_item_representations_image_optimization_queue")));
    assert!(plan
        .iter()
        .all(|detail| !detail.contains("USE TEMP B-TREE FOR ORDER BY")));
    Ok(())
}

#[test]
#[ignore = "profiling harness for pending OCR candidate discovery"]
fn profile_pending_ocr_candidate_query() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    seed_pending_ocr_candidate_archive(&mut db, 50_000)?;
    let sql = r"
        SELECT o.raw_sha256
        FROM ocr_results o
        WHERE o.status = 'pending'
        ORDER BY o.updated_at ASC, o.raw_sha256 ASC
        LIMIT ?1
    ";

    db.conn
        .execute("DROP INDEX IF EXISTS idx_ocr_results_pending_queue", [])?;
    db.conn.execute_batch("ANALYZE")?;
    let before = median_profile_run(7, || {
        let mut stmt = db.conn.prepare(sql)?;
        let rows = stmt.query_map([25_i64], |row| row.get::<_, String>(0))?;
        collect_rows(rows).map(|rows| rows.len())
    })?;

    db.conn.execute(
        "CREATE INDEX idx_ocr_results_pending_queue
         ON ocr_results(status, updated_at ASC, raw_sha256 ASC)",
        [],
    )?;
    db.conn.execute_batch("ANALYZE")?;
    let after = median_profile_run(7, || {
        let mut stmt = db.conn.prepare(sql)?;
        let rows = stmt.query_map([25_i64], |row| row.get::<_, String>(0))?;
        collect_rows(rows).map(|rows| rows.len())
    })?;

    eprintln!("pending_ocr_candidates_before={before:?} after={after:?}");
    Ok(())
}

#[test]
#[ignore = "profiling harness for unfiltered stats leaderboard ordering"]
fn profile_unfiltered_stats_leaderboards() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    seed_stats_leaderboard_archive(&mut db, 50_000)?;
    let largest_sql = r"
        SELECT s.id, s.total_bytes
        FROM snapshots s
        JOIN snapshot_stats ss ON ss.snapshot_id = s.id
        ORDER BY s.total_bytes DESC, s.id ASC
        LIMIT ?1
    ";
    let most_captured_sql = r"
        SELECT s.id, ss.capture_count
        FROM snapshots s
        JOIN snapshot_stats ss ON ss.snapshot_id = s.id
        ORDER BY ss.capture_count DESC, s.id ASC
        LIMIT ?1
    ";

    db.conn
        .execute("DROP INDEX IF EXISTS idx_snapshots_total_bytes", [])?;
    db.conn
        .execute("DROP INDEX IF EXISTS idx_snapshot_stats_capture_count", [])?;
    db.conn.execute_batch("ANALYZE")?;
    let largest_before = median_profile_run_expected(7, 5, || {
        let mut stmt = db.conn.prepare(largest_sql)?;
        let rows = stmt.query_map([5_i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        collect_rows(rows).map(|rows| rows.len())
    })?;
    let most_captured_before = median_profile_run_expected(7, 5, || {
        let mut stmt = db.conn.prepare(most_captured_sql)?;
        let rows = stmt.query_map([5_i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        collect_rows(rows).map(|rows| rows.len())
    })?;

    db.conn.execute(
        "CREATE INDEX idx_snapshots_total_bytes ON snapshots(total_bytes DESC, id ASC)",
        [],
    )?;
    db.conn.execute(
        "CREATE INDEX idx_snapshot_stats_capture_count
         ON snapshot_stats(capture_count DESC, snapshot_id ASC)",
        [],
    )?;
    db.conn.execute_batch("ANALYZE")?;
    let largest_after = median_profile_run_expected(7, 5, || {
        let mut stmt = db.conn.prepare(largest_sql)?;
        let rows = stmt.query_map([5_i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        collect_rows(rows).map(|rows| rows.len())
    })?;
    let most_captured_after = median_profile_run_expected(7, 5, || {
        let mut stmt = db.conn.prepare(most_captured_sql)?;
        let rows = stmt.query_map([5_i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        collect_rows(rows).map(|rows| rows.len())
    })?;

    eprintln!(
        "largest_before={largest_before:?} largest_after={largest_after:?} most_captured_before={most_captured_before:?} most_captured_after={most_captured_after:?}"
    );
    Ok(())
}

#[test]
#[ignore = "profiling harness for large retrieval workloads"]
fn profile_large_retrieval_queries() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    seed_large_archive(&mut db, 5_000, 100_000)?;
    let app_filtered = RetrievalFilters::default().with_app(Some("terminal".to_string()));
    let bundle_filtered =
        RetrievalFilters::default().with_bundle_id(Some("com.apple.Terminal".to_string()));
    let recent_hours = RetrievalFilters::default().with_hours(Some(24));

    let started = Instant::now();
    let recent = db.recent_page(20, &unfiltered(), None)?;
    let recent_elapsed = started.elapsed();

    let started = Instant::now();
    let search_fts = db.search_fts("git", 20, &unfiltered())?;
    let search_fts_elapsed = started.elapsed();

    let started = Instant::now();
    let search_literal = db.search_literal("/tmp/repo/42/Cargo.toml", 20, &unfiltered())?;
    let search_literal_elapsed = started.elapsed();

    let started = Instant::now();
    let timeline = db.timeline_page(20, &unfiltered(), TimelineSort::Desc, None)?;
    let timeline_elapsed = started.elapsed();

    let started = Instant::now();
    let recent_app = db.recent_page(20, &app_filtered, None)?;
    let recent_app_elapsed = started.elapsed();

    let started = Instant::now();
    let recent_bundle = db.recent_page(20, &bundle_filtered, None)?;
    let recent_bundle_elapsed = started.elapsed();

    let started = Instant::now();
    let recent_24h = db.recent_page(20, &recent_hours, None)?;
    let recent_24h_elapsed = started.elapsed();

    let started = Instant::now();
    let search_fts_app = db.search_fts("git", 20, &app_filtered)?;
    let search_fts_app_elapsed = started.elapsed();

    eprintln!(
        "recent={:?} search_fts={:?} search_literal={:?} timeline={:?} recent_app={:?} recent_bundle={:?} recent_24h={:?} search_fts_app={:?} counts=({}, {}, {}, {}, {}, {}, {}, {})",
        recent_elapsed,
        search_fts_elapsed,
        search_literal_elapsed,
        timeline_elapsed,
        recent_app_elapsed,
        recent_bundle_elapsed,
        recent_24h_elapsed,
        search_fts_app_elapsed,
        recent.items().len(),
        search_fts.hits().len(),
        search_literal.hits().len(),
        timeline.items().len(),
        recent_app.items().len(),
        recent_bundle.items().len(),
        recent_24h.items().len(),
        search_fts_app.hits().len()
    );

    Ok(())
}

#[test]
#[ignore = "profiling harness for image optimization candidate discovery"]
fn profile_image_optimization_candidate_query() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    seed_image_optimization_candidate_archive(&mut db, 50_000)?;
    let sql = r"
        SELECT snapshot_id, item_index, uti, byte_len, raw_sha256, blob_value
        FROM item_representations
        WHERE kind = 'image'
          AND image_compression_status = 'uncompressed'
          AND length(blob_value) > 0
        ORDER BY byte_len DESC, snapshot_id ASC, item_index ASC, uti ASC
        LIMIT ?1
    ";

    db.conn.execute(
        "DROP INDEX IF EXISTS idx_item_representations_image_optimization_queue",
        [],
    )?;
    db.conn.execute_batch("ANALYZE")?;
    let before = median_profile_run(7, || {
        let mut stmt = db.conn.prepare(sql)?;
        let rows = stmt.query_map([25_i64], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Vec<u8>>(5)?,
            ))
        })?;
        collect_rows(rows).map(|rows| rows.len())
    })?;

    db.conn.execute(
        "CREATE INDEX idx_item_representations_image_optimization_queue
         ON item_representations(
             image_compression_status,
             byte_len DESC,
             snapshot_id ASC,
             item_index ASC,
             uti ASC
         )
         WHERE kind = 'image' AND length(blob_value) > 0",
        [],
    )?;
    db.conn.execute_batch("ANALYZE")?;
    let after = median_profile_run(7, || {
        let mut stmt = db.conn.prepare(sql)?;
        let rows = stmt.query_map([25_i64], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Vec<u8>>(5)?,
            ))
        })?;
        collect_rows(rows).map(|rows| rows.len())
    })?;

    eprintln!("image_optimization_candidates_before={before:?} after={after:?}");
    Ok(())
}

#[test]
#[ignore = "profiling harness for repeated open_existing startup cost"]
fn profile_open_existing_on_large_archive() -> Result<()> {
    let path = temp_db_path("open-existing-profile");
    let mut db = Database::open_or_init(&path)?;
    seed_large_archive(&mut db, 5_000, 100_000)?;
    drop(db);

    let started = Instant::now();
    let db = Database::open_existing(&path)?;
    let first_open_elapsed = started.elapsed();
    drop(db);

    let started = Instant::now();
    let db = Database::open_existing(&path)?;
    let second_open_elapsed = started.elapsed();
    drop(db);

    eprintln!(
        "open_existing_first={first_open_elapsed:?} open_existing_second={second_open_elapsed:?}"
    );

    cleanup_db(&path);
    Ok(())
}
