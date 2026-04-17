use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;

use anyhow::{Context, Result};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::model::{
    build_item, build_representation, build_snapshot, CaptureContext, ClipboardSnapshot,
};
use rusqlite::params;

use super::{
    configure_connection, explain_query_plan, Database, RetrievalFilters, RetrievalKind,
    SearchMode, TimelineSort, SCHEMA,
};

fn temp_db_path(test_name: &str) -> std::path::PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();

    std::env::temp_dir()
        .join("clipmem-db-tests")
        .join(format!("{test_name}-{}-{timestamp}.sqlite3", process::id()))
}

fn cleanup_db(path: &std::path::Path) {
    for suffix in ["", "-shm", "-wal"] {
        let candidate = if suffix.is_empty() {
            path.to_path_buf()
        } else {
            std::path::PathBuf::from(format!("{}{suffix}", path.display()))
        };
        let _ = std::fs::remove_file(candidate);
    }

    if let Some(parent) = path.parent() {
        let _ = std::fs::remove_dir(parent);
    }
}

fn fake_snapshot(change_count: i64, text: &str) -> ClipboardSnapshot {
    let representation = build_representation(
        "public.utf8-plain-text".to_string(),
        None,
        text.as_bytes().to_vec(),
    );

    let item = build_item(0, vec![representation]);
    build_snapshot(
        CaptureContext::new(change_count)
            .with_frontmost_app_name("Test App")
            .with_frontmost_app_bundle_id("com.example.test"),
        vec![item],
    )
}

fn set_event_observed_at(db: &Database, event_id: i64, observed_at: &str) -> Result<()> {
    db.conn.execute(
        "UPDATE capture_events SET observed_at = ?1 WHERE id = ?2",
        [observed_at, &event_id.to_string()],
    )?;
    Ok(())
}

fn unfiltered() -> RetrievalFilters {
    RetrievalFilters::default()
}

fn seed_large_archive(db: &mut Database, snapshot_count: usize, event_count: usize) -> Result<()> {
    let tx = db
        .conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

    {
        let mut insert_snapshot = tx.prepare(
            "INSERT INTO snapshots (
                sha256,
                snapshot_kind,
                preview_text,
                search_text,
                item_count,
                total_bytes,
                created_at
            ) VALUES (?1, 'plain_text', ?2, ?3, 1, 96, ?4)",
        )?;
        let mut insert_item = tx.prepare(
            "INSERT INTO snapshot_items (
                snapshot_id,
                item_index,
                primary_kind,
                primary_uti,
                preview_text,
                search_text,
                total_bytes
            ) VALUES (?1, 0, 'plain_text', 'public.utf8-plain-text', ?2, ?3, 96)",
        )?;
        let mut insert_text_representation = tx.prepare(
            "INSERT INTO item_representations (
                snapshot_id,
                item_index,
                uti,
                kind,
                byte_len,
                raw_sha256,
                text_value,
                blob_value
            ) VALUES (?1, 0, 'public.utf8-plain-text', 'plain_text', 96, ?2, ?3, ?4)",
        )?;
        let mut insert_url_representation = tx.prepare(
            "INSERT INTO item_representations (
                snapshot_id,
                item_index,
                uti,
                kind,
                byte_len,
                raw_sha256,
                text_value,
                blob_value
            ) VALUES (?1, 0, 'public.url', 'url', 48, ?2, ?3, ?4)",
        )?;

        for snapshot_index in 0..snapshot_count {
            let snapshot_number = snapshot_index + 1;
            let preview_text = format!("git status output sample {snapshot_number}");
            let search_text =
                format!("{preview_text} with repo path /tmp/repo/{snapshot_number}/Cargo.toml");
            let url = format!("https://example.com/{snapshot_number}");
            let created_at = format!(
                "2026-04-17 {:02}:{:02}:{:02}",
                12usize.saturating_sub((snapshot_index / 3600) % 12),
                (snapshot_index / 60) % 60,
                snapshot_index % 60
            );
            let fingerprint = format!("{:064x}", snapshot_number);

            insert_snapshot
                .execute(params![fingerprint, preview_text, search_text, created_at,])?;

            let snapshot_id = tx.last_insert_rowid();
            insert_item.execute(params![snapshot_id, preview_text, search_text])?;

            let text_bytes = search_text.as_bytes().to_vec();
            insert_text_representation.execute(params![
                snapshot_id,
                format!("{:064x}", 100_000 + snapshot_number),
                search_text,
                text_bytes,
            ])?;

            let url_bytes = url.as_bytes().to_vec();
            insert_url_representation.execute(params![
                snapshot_id,
                format!("{:064x}", 200_000 + snapshot_number),
                url,
                url_bytes,
            ])?;
        }
    }

    {
        let mut insert_event = tx.prepare(
            "INSERT INTO capture_events (
                snapshot_id,
                observed_at,
                change_count,
                frontmost_app_bundle_id,
                frontmost_app_name
            ) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;

        for event_index in 0..event_count {
            let snapshot_id = ((event_index % snapshot_count) + 1) as i64;
            let observed_at = format!(
                "2026-04-17 {:02}:{:02}:{:02}",
                12usize.saturating_sub((event_index / 3600) % 12),
                (event_index / 60) % 60,
                event_index % 60
            );
            let (bundle_id, app_name) = if event_index % 3 == 0 {
                ("com.apple.Terminal", "Terminal")
            } else {
                ("com.apple.Safari", "Safari")
            };

            insert_event.execute(params![
                snapshot_id,
                observed_at,
                event_index as i64 + 1,
                bundle_id,
                app_name,
            ])?;
        }
    }

    tx.execute_batch("ANALYZE")?;
    tx.commit()?;
    Ok(())
}

#[test]
fn duplicate_snapshots_share_content_row() -> Result<()> {
    let mut db = Database::open_in_memory()?;

    let first = fake_snapshot(1, "git clone https://example.com/repo");
    let second = fake_snapshot(2, "git clone https://example.com/repo");

    let first_store = db.store_capture(&first)?;
    let second_store = db.store_capture(&second)?;

    assert!(first_store.inserted_new_snapshot());
    assert!(!second_store.inserted_new_snapshot());
    assert_eq!(first_store.snapshot_id(), second_store.snapshot_id());

    let results = db.search_auto("git", 10, &unfiltered())?;
    assert_eq!(results.mode_used(), SearchMode::Fts);
    assert_eq!(results.hits().len(), 1);
    assert_eq!(results.hits()[0].capture_count(), 2);

    let details = db
        .find_snapshot(first_store.snapshot_id(), 10)?
        .context("expected stored snapshot details")?;
    assert_eq!(details.capture_count(), 2);
    assert_eq!(details.items().len(), 1);

    Ok(())
}

#[test]
fn open_existing_does_not_create_missing_database_paths() {
    let path = temp_db_path("open-existing-missing");

    let result = Database::open_existing(&path);

    assert!(result.is_err());
    assert!(!path.exists());
}

#[test]
fn search_like_treats_percent_as_literal() -> Result<()> {
    let mut db = Database::open_in_memory()?;

    db.store_capture(&fake_snapshot(1, "Discount: 50 percent off"))?;
    db.store_capture(&fake_snapshot(2, "Discount: 50%"))?;

    let hits = db.search_literal("50%", 10, &unfiltered())?;
    let previews: Vec<_> = hits
        .hits()
        .iter()
        .map(crate::model::SearchHit::preview_text)
        .collect();

    assert_eq!(previews, vec!["Discount: 50%"]);
    Ok(())
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

#[test]
fn search_literal_matches_file_url_paths() -> Result<()> {
    let mut db = Database::open_in_memory()?;

    let snapshot = build_snapshot(
        CaptureContext::new(1)
            .with_frontmost_app_name("Finder")
            .with_frontmost_app_bundle_id("com.apple.finder"),
        vec![build_item(
            0,
            vec![build_representation(
                "public.file-url".to_string(),
                Some("file:///tmp/repo/42/Cargo.toml".to_string()),
                b"file:///tmp/repo/42/Cargo.toml".to_vec(),
            )],
        )],
    );
    db.store_capture(&snapshot)?;

    let hits = db.search_literal("/tmp/repo/42/Cargo.toml", 10, &unfiltered())?;

    assert_eq!(hits.hits().len(), 1);
    assert_eq!(
        hits.hits()[0].preview_text(),
        "file:///tmp/repo/42/Cargo.toml"
    );
    Ok(())
}

#[test]
fn search_percent_literal_returns_only_exact_matches() -> Result<()> {
    let mut db = Database::open_in_memory()?;

    db.store_capture(&fake_snapshot(1, "Discount: 50 percent off"))?;
    db.store_capture(&fake_snapshot(2, "Discount: 50%"))?;

    let results = db.search_auto("50%", 10, &unfiltered())?;
    let previews: Vec<_> = results
        .hits()
        .iter()
        .map(crate::model::SearchHit::preview_text)
        .collect();

    assert_eq!(results.mode_used(), SearchMode::Literal);
    assert_eq!(previews, vec!["Discount: 50%"]);
    Ok(())
}

#[test]
fn search_underscore_literal_returns_only_exact_matches() -> Result<()> {
    let mut db = Database::open_in_memory()?;

    db.store_capture(&fake_snapshot(1, "configXtest"))?;
    db.store_capture(&fake_snapshot(2, "config test"))?;
    db.store_capture(&fake_snapshot(3, "config_test"))?;

    let results = db.search_auto("config_test", 10, &unfiltered())?;
    let previews: Vec<_> = results
        .hits()
        .iter()
        .map(crate::model::SearchHit::preview_text)
        .collect();

    assert_eq!(results.mode_used(), SearchMode::Literal);
    assert_eq!(previews, vec!["config_test"]);
    Ok(())
}

#[test]
fn search_propagates_non_syntax_fts_failures() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    db.store_capture(&fake_snapshot(1, "git clone https://example.com/repo"))?;
    db.conn.execute_batch("DROP TABLE snapshots_fts;")?;

    assert!(db.search_auto("git", 10, &unfiltered()).is_err());
    Ok(())
}

#[test]
fn schema_keeps_fts_index_and_triggers() {
    assert!(SCHEMA.contains("CREATE VIRTUAL TABLE IF NOT EXISTS snapshots_fts"));
    assert!(SCHEMA.contains("CREATE TRIGGER IF NOT EXISTS snapshots_ai"));
    assert!(SCHEMA.contains("CREATE TRIGGER IF NOT EXISTS snapshots_ad"));
    assert!(SCHEMA.contains("CREATE TRIGGER IF NOT EXISTS snapshots_au"));
    assert!(SCHEMA.contains("idx_capture_events_snapshot_observed_id"));
    assert!(SCHEMA.contains("idx_capture_events_observed_id"));
}

#[test]
fn timeline_returns_real_events_in_stable_descending_order() -> Result<()> {
    let mut db = Database::open_in_memory()?;

    let first = db.store_capture(&fake_snapshot(1, "git status"))?;
    let second = db.store_capture(&fake_snapshot(2, "git status"))?;
    let third = db.store_capture(&fake_snapshot(3, "cargo test"))?;

    set_event_observed_at(&db, first.event_id(), "2026-04-16 10:00:00")?;
    set_event_observed_at(&db, second.event_id(), "2026-04-16 10:00:00")?;
    set_event_observed_at(&db, third.event_id(), "2026-04-16 11:00:00")?;

    let page = db.timeline_page(10, &unfiltered(), TimelineSort::Desc, None)?;
    let ids = page
        .items()
        .iter()
        .map(|event| event.event_id())
        .collect::<Vec<_>>();

    assert_eq!(
        ids,
        vec![third.event_id(), second.event_id(), first.event_id()]
    );
    assert_eq!(page.items()[1].snapshot_id(), first.snapshot_id());
    assert_eq!(page.items()[1].change_count(), 2);
    Ok(())
}

#[test]
fn timeline_paging_respects_sort_and_cursor_boundaries() -> Result<()> {
    let mut db = Database::open_in_memory()?;

    let first = db.store_capture(&fake_snapshot(1, "alpha"))?;
    let second = db.store_capture(&fake_snapshot(2, "beta"))?;
    let third = db.store_capture(&fake_snapshot(3, "gamma"))?;

    set_event_observed_at(&db, first.event_id(), "2026-04-16 09:00:00")?;
    set_event_observed_at(&db, second.event_id(), "2026-04-16 10:00:00")?;
    set_event_observed_at(&db, third.event_id(), "2026-04-16 11:00:00")?;

    let first_page = db.timeline_page(2, &unfiltered(), TimelineSort::Asc, None)?;
    assert!(first_page.has_more());
    let cursor = super::TimelineCursorState::new(
        first_page
            .items()
            .last()
            .expect("timeline page should not be empty")
            .observed_at()
            .to_string(),
        first_page
            .items()
            .last()
            .expect("timeline page should not be empty")
            .event_id(),
    );

    let second_page = db.timeline_page(2, &unfiltered(), TimelineSort::Asc, Some(&cursor))?;
    let ids = second_page
        .items()
        .iter()
        .map(|event| event.event_id())
        .collect::<Vec<_>>();

    assert!(!second_page.has_more());
    assert_eq!(ids, vec![third.event_id()]);
    Ok(())
}

#[test]
fn shared_filters_constrain_search_recent_and_timeline_consistently() -> Result<()> {
    let mut db = Database::open_in_memory()?;

    let rich = build_snapshot(
        CaptureContext::new(1)
            .with_frontmost_app_name("Terminal")
            .with_frontmost_app_bundle_id("com.apple.Terminal"),
        vec![build_item(
            0,
            vec![
                build_representation(
                    "public.utf8-plain-text".to_string(),
                    Some("git clone https://example.com/repo".to_string()),
                    b"git clone https://example.com/repo".to_vec(),
                ),
                build_representation(
                    "public.url".to_string(),
                    Some("https://example.com/repo".to_string()),
                    b"https://example.com/repo".to_vec(),
                ),
            ],
        )],
    );
    let preview = fake_snapshot(2, "meeting notes");

    let first = db.store_capture(&rich)?;
    let second = db.store_capture(&rich)?;
    let _third = db.store_capture(&preview)?;

    set_event_observed_at(&db, first.event_id(), "2026-04-16 09:00:00")?;
    set_event_observed_at(&db, second.event_id(), "2026-04-16 10:00:00")?;

    let filters = RetrievalFilters::new(
        None,
        None,
        None,
        Some("terminal".to_string()),
        None,
        Some(RetrievalKind::Url),
        false,
        true,
        false,
        false,
        false,
        Some(20),
        None,
    );

    let search = db.search_auto("example.com", 10, &filters)?;
    let recent = db.recent(10, &filters)?;
    let timeline = db.timeline_page(10, &filters, TimelineSort::Desc, None)?;

    assert_eq!(search.hits().len(), 1);
    assert_eq!(recent.len(), 1);
    assert_eq!(timeline.items().len(), 2);
    assert_eq!(search.hits()[0].snapshot_id(), first.snapshot_id());
    assert_eq!(recent[0].snapshot_id(), first.snapshot_id());
    assert!(timeline
        .items()
        .iter()
        .all(|event| event.snapshot_id() == first.snapshot_id()));
    Ok(())
}

#[test]
fn configure_connection_enables_foreign_keys() -> Result<()> {
    let conn = rusqlite::Connection::open_in_memory()?;
    configure_connection(&conn)?;

    let foreign_keys_enabled: i64 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
    assert_eq!(foreign_keys_enabled, 1);
    Ok(())
}

#[test]
fn search_auto_falls_back_for_url_queries() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    db.store_capture(&fake_snapshot(1, "git clone https://example.com/repo"))?;

    let results = db.search_auto("https://example.com/repo", 10, &unfiltered())?;

    assert_eq!(results.mode_used(), SearchMode::Literal);
    assert_eq!(results.hits().len(), 1);
    assert_eq!(
        results.hits()[0].preview_text(),
        "git clone https://example.com/repo"
    );
    Ok(())
}

#[test]
fn search_auto_falls_back_for_colon_queries() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    db.store_capture(&fake_snapshot(1, "foo:bar"))?;

    let results = db.search_auto("foo:bar", 10, &unfiltered())?;

    assert_eq!(results.mode_used(), SearchMode::Literal);
    assert_eq!(results.hits().len(), 1);
    assert_eq!(results.hits()[0].preview_text(), "foo:bar");
    Ok(())
}

#[test]
fn search_auto_falls_back_for_bundle_id_queries() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    db.store_capture(&fake_snapshot(1, "com.apple.Terminal"))?;

    let results = db.search_auto("com.apple.Terminal", 10, &unfiltered())?;

    assert_eq!(results.mode_used(), SearchMode::Literal);
    assert_eq!(results.hits().len(), 1);
    assert_eq!(results.hits()[0].preview_text(), "com.apple.Terminal");
    Ok(())
}

#[test]
fn search_auto_falls_back_for_slashy_path_queries() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    db.store_capture(&fake_snapshot(1, "logs/2024/archive"))?;

    let results = db.search_auto("logs/2024", 10, &unfiltered())?;

    assert_eq!(results.mode_used(), SearchMode::Literal);
    assert_eq!(results.hits().len(), 1);
    assert_eq!(results.hits()[0].preview_text(), "logs/2024/archive");
    Ok(())
}

#[test]
fn search_auto_handles_leading_colon_queries_without_error() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    db.store_capture(&fake_snapshot(1, "git clone https://example.com/repo"))?;

    let results = db.search_auto(":leading", 10, &unfiltered())?;

    assert_eq!(results.mode_used(), SearchMode::Literal);
    assert!(results.hits().is_empty());
    Ok(())
}

#[test]
fn search_auto_keeps_valid_quoted_fts_queries_in_fts_mode() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    db.store_capture(&fake_snapshot(1, "launchctl bootstrap"))?;

    let results = db.search_auto("\"launchctl\" AND bootstrap", 10, &unfiltered())?;

    assert_eq!(results.mode_used(), SearchMode::Fts);
    assert_eq!(results.hits().len(), 1);
    Ok(())
}

#[test]
fn open_existing_migrates_legacy_database_and_rebuilds_fts() -> Result<()> {
    let path = temp_db_path("legacy-migration");
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
        INSERT INTO snapshots (
            id, sha256, snapshot_kind, preview_text, search_text, item_count, total_bytes, created_at
        ) VALUES (
            1, 'legacy-sha', 'plain_text', 'git status', 'git status', 1, 10, '2026-04-16 10:00:00'
        );
        INSERT INTO capture_events (
            id, snapshot_id, observed_at, change_count, frontmost_app_bundle_id, frontmost_app_name
        ) VALUES (
            1, 1, '2026-04-16 10:00:00', 1, 'com.example.test', 'Test App'
        );
        PRAGMA user_version = 0;
    ",
    )?;
    drop(conn);

    let db = Database::open_existing(&path)?;
    let version: i64 = db
        .conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let results = db.search_auto("git", 10, &unfiltered())?;

    assert_eq!(version, 6);
    assert_eq!(results.mode_used(), SearchMode::Fts);
    assert_eq!(results.hits().len(), 1);

    std::fs::remove_file(&path)?;
    Ok(())
}

#[test]
fn repeated_open_existing_is_idempotent_after_migration() -> Result<()> {
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

    drop(Database::open_existing(&path)?);
    let db = Database::open_existing(&path)?;
    let version: i64 = db
        .conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))?;

    assert_eq!(version, 6);

    std::fs::remove_file(&path)?;
    Ok(())
}

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
#[ignore = "profiling harness for large retrieval workloads"]
fn profile_large_retrieval_queries() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    seed_large_archive(&mut db, 5_000, 100_000)?;
    let app_filtered = RetrievalFilters::new(
        None,
        None,
        None,
        Some("terminal".to_string()),
        None,
        None,
        false,
        false,
        false,
        false,
        false,
        None,
        None,
    );
    let bundle_filtered = RetrievalFilters::new(
        None,
        None,
        None,
        None,
        Some("com.apple.Terminal".to_string()),
        None,
        false,
        false,
        false,
        false,
        false,
        None,
        None,
    );
    let recent_hours = RetrievalFilters::new(
        None,
        None,
        Some(24),
        None,
        None,
        None,
        false,
        false,
        false,
        false,
        false,
        None,
        None,
    );

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
        "open_existing_first={:?} open_existing_second={:?}",
        first_open_elapsed, second_open_elapsed
    );

    cleanup_db(&path);
    Ok(())
}
