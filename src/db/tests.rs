use anyhow::{Context, Result};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::clipboard::{build_item, build_snapshot, hash_bytes};
use crate::model::{ClipboardKind, ClipboardRepresentation, ClipboardSnapshot};

use super::Database;

fn temp_db_path(test_name: &str) -> std::path::PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();

    std::env::temp_dir()
        .join("clipmem-db-tests")
        .join(format!("{test_name}-{}-{timestamp}.sqlite3", process::id()))
}

fn fake_snapshot(change_count: i64, text: &str) -> ClipboardSnapshot {
    let representation = ClipboardRepresentation::new(
        "public.utf8-plain-text".to_string(),
        ClipboardKind::PlainText,
        hash_bytes(text.as_bytes()),
        Some(text.to_string()),
        text.as_bytes().to_vec(),
    );

    let item = build_item(0, vec![representation]);
    build_snapshot(
        change_count,
        Some("Test App".to_string()),
        Some("com.example.test".to_string()),
        vec![item],
    )
}

#[test]
fn duplicate_snapshots_share_content_row() -> Result<()> {
    let mut db = Database::open_in_memory()?;

    let first = fake_snapshot(1, "git clone https://example.com/repo");
    let second = fake_snapshot(2, "git clone https://example.com/repo");

    let first_store = db.store_capture(&first)?;
    let second_store = db.store_capture(&second)?;

    assert!(first_store.inserted_new_snapshot);
    assert!(!second_store.inserted_new_snapshot);
    assert_eq!(first_store.snapshot_id, second_store.snapshot_id);

    let hits = db.search_auto("git", 10)?;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].capture_count, 2);

    let details = db
        .find_snapshot(first_store.snapshot_id, 10)?
        .context("expected stored snapshot details")?;
    assert_eq!(details.capture_count, 2);
    assert_eq!(details.items.len(), 1);

    Ok(())
}

#[test]
fn open_existing_does_not_create_missing_database_paths() {
    let path = temp_db_path("open-existing-missing");
    let parent = path.parent().expect("temporary path should have a parent");

    let result = Database::open_existing(&path);

    assert!(result.is_err());
    assert!(!parent.exists());
}

#[test]
fn search_like_treats_percent_as_literal() -> Result<()> {
    let mut db = Database::open_in_memory()?;

    db.store_capture(&fake_snapshot(1, "Discount: 50 percent off"))?;
    db.store_capture(&fake_snapshot(2, "Discount: 50%"))?;

    let hits = db.search_literal("50%", 10)?;
    let previews: Vec<_> = hits.iter().map(|hit| hit.preview_text.as_str()).collect();

    assert_eq!(previews, vec!["Discount: 50%"]);
    Ok(())
}

#[test]
fn search_like_treats_underscore_as_literal() -> Result<()> {
    let mut db = Database::open_in_memory()?;

    db.store_capture(&fake_snapshot(1, "configXtest"))?;
    db.store_capture(&fake_snapshot(2, "config test"))?;
    db.store_capture(&fake_snapshot(3, "config_test"))?;

    let hits = db.search_literal("config_test", 10)?;
    let previews: Vec<_> = hits.iter().map(|hit| hit.preview_text.as_str()).collect();

    assert_eq!(previews, vec!["config_test"]);
    Ok(())
}

#[test]
fn search_like_treats_escape_character_as_literal() -> Result<()> {
    let mut db = Database::open_in_memory()?;

    db.store_capture(&fake_snapshot(1, r"logs\2024\archive"))?;
    db.store_capture(&fake_snapshot(2, "logs/2024/archive"))?;

    let hits = db.search_literal(r"logs\2024", 10)?;
    let previews: Vec<_> = hits.iter().map(|hit| hit.preview_text.as_str()).collect();

    assert_eq!(previews, vec![r"logs\2024\archive"]);
    Ok(())
}

#[test]
fn search_percent_literal_returns_only_exact_matches() -> Result<()> {
    let mut db = Database::open_in_memory()?;

    db.store_capture(&fake_snapshot(1, "Discount: 50 percent off"))?;
    db.store_capture(&fake_snapshot(2, "Discount: 50%"))?;

    let hits = db.search_auto("50%", 10)?;
    let previews: Vec<_> = hits.iter().map(|hit| hit.preview_text.as_str()).collect();

    assert_eq!(previews, vec!["Discount: 50%"]);
    Ok(())
}

#[test]
fn search_underscore_literal_returns_only_exact_matches() -> Result<()> {
    let mut db = Database::open_in_memory()?;

    db.store_capture(&fake_snapshot(1, "configXtest"))?;
    db.store_capture(&fake_snapshot(2, "config test"))?;
    db.store_capture(&fake_snapshot(3, "config_test"))?;

    let hits = db.search_auto("config_test", 10)?;
    let previews: Vec<_> = hits.iter().map(|hit| hit.preview_text.as_str()).collect();

    assert_eq!(previews, vec!["config_test"]);
    Ok(())
}

#[test]
fn search_propagates_non_syntax_fts_failures() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    db.store_capture(&fake_snapshot(1, "git clone https://example.com/repo"))?;
    db.conn.execute_batch("DROP TABLE snapshots_fts;")?;

    assert!(db.search_auto("git", 10).is_err());
    Ok(())
}
