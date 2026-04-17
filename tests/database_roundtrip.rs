use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clipmem::archive::{Database, SearchMode};
use clipmem::capture::{
    build_item, build_representation, build_snapshot, CaptureContext, ClipboardKind,
    ClipboardSnapshot, SnapshotKind,
};

fn temp_db_path(test_name: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();

    std::env::temp_dir()
        .join("clipmem-tests")
        .join(format!("{test_name}-{}-{timestamp}.sqlite3", process::id()))
}

fn cleanup_db(path: &Path) {
    for suffix in ["", "-shm", "-wal"] {
        let candidate = if suffix.is_empty() {
            path.to_path_buf()
        } else {
            PathBuf::from(format!("{}{suffix}", path.display()))
        };
        let _ = fs::remove_file(candidate);
    }

    if let Some(parent) = path.parent() {
        let _ = fs::remove_dir(parent);
    }
}

fn text_snapshot(change_count: i64, text: &str) -> ClipboardSnapshot {
    let representation =
        build_representation(
            "public.utf8-plain-text".to_string(),
            Some(text.to_string()),
            text.as_bytes().to_vec(),
        );
    let item = build_item(0, vec![representation]);

    build_snapshot(
        CaptureContext::new(change_count)
            .with_frontmost_app_name("Editor")
            .with_frontmost_app_bundle_id("com.example.Editor"),
        vec![item],
    )
}

#[test]
fn public_database_api_round_trips_a_stored_snapshot() -> Result<()> {
    let path = temp_db_path("database-roundtrip");
    let mut db = Database::open_or_init(&path)?;
    let snapshot = text_snapshot(1, "git clone https://example.com/repo");

    let stored = db.store_capture(&snapshot)?;
    drop(db);

    let db = Database::open_existing(&path)?;
    let results = db.search_auto("git", 10)?;
    let details = db
        .find_snapshot(stored.snapshot_id(), 10)?
        .context("expected stored snapshot details")?;

    assert_eq!(results.mode_used(), SearchMode::Fts);
    assert_eq!(results.hits().len(), 1);
    assert_eq!(results.hits()[0].snapshot_id(), stored.snapshot_id());
    assert_eq!(details.capture_count(), 1);
    assert_eq!(details.preview_text(), snapshot.preview_text());
    assert_eq!(details.snapshot_kind(), SnapshotKind::PlainText);
    assert_eq!(details.items().len(), 1);
    assert_eq!(details.items()[0].primary_kind(), ClipboardKind::PlainText);
    assert_eq!(
        details.items()[0].representations()[0].kind(),
        ClipboardKind::PlainText
    );
    assert_eq!(
        details.items()[0].representations()[0].text_value(),
        Some("git clone https://example.com/repo")
    );
    assert_eq!(details.total_bytes(), snapshot.total_bytes());
    assert_eq!(details.item_count(), snapshot.item_count());

    cleanup_db(&path);
    Ok(())
}
