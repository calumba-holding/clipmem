use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clipmem::db::Database;
use clipmem::model::{
    ClipboardItem, ClipboardKind, ClipboardRepresentation, ClipboardSnapshot, SnapshotKind,
};
use sha2::{Digest, Sha256};

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

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn text_snapshot(change_count: i64, text: &str) -> ClipboardSnapshot {
    let raw_bytes = text.as_bytes().to_vec();
    let raw_sha256 = sha256_hex(&raw_bytes);

    let representation = ClipboardRepresentation::new(
        "public.utf8-plain-text".to_string(),
        ClipboardKind::PlainText,
        raw_sha256.clone(),
        Some(text.to_string()),
        raw_bytes,
    );
    let item = ClipboardItem::new(
        0,
        ClipboardKind::PlainText,
        Some("public.utf8-plain-text".to_string()),
        text.to_string(),
        text.to_string(),
        vec![representation],
    );

    ClipboardSnapshot::new(
        change_count,
        Some("Editor".to_string()),
        Some("com.example.Editor".to_string()),
        raw_sha256,
        SnapshotKind::PlainText,
        text.to_string(),
        text.to_string(),
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
    let hits = db.search_auto("git", 10)?;
    let details = db
        .find_snapshot(stored.snapshot_id, 10)?
        .context("expected stored snapshot details")?;

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].snapshot_id, stored.snapshot_id);
    assert_eq!(details.capture_count, 1);
    assert_eq!(details.preview_text, snapshot.preview_text);
    assert_eq!(details.items.len(), 1);
    assert_eq!(details.items[0].primary_kind, ClipboardKind::PlainText);
    assert_eq!(
        details.items[0].representations[0].kind,
        ClipboardKind::PlainText
    );
    assert_eq!(
        details.items[0].representations[0].text_value.as_deref(),
        Some("git clone https://example.com/repo")
    );

    cleanup_db(&path);
    Ok(())
}
