use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use clipmem::db::Database;
use clipmem::model::ClipboardSnapshot;
use clipmem::testing::{build_item, build_representation, build_snapshot, CaptureContext};
use serde_json::Value;

fn temp_db_path(test_name: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();

    std::env::temp_dir()
        .join("clipmem-cli-tests")
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
    let item = build_item(
        0,
        vec![build_representation(
            "public.utf8-plain-text".to_string(),
            None,
            text.as_bytes().to_vec(),
        )],
    );

    build_snapshot(
        CaptureContext::new(change_count)
            .with_frontmost_app_name("Terminal")
            .with_frontmost_app_bundle_id("com.apple.Terminal"),
        vec![item],
    )
}

fn seed_database(path: &Path, snapshots: &[ClipboardSnapshot]) -> Result<Vec<i64>> {
    let mut db = Database::open_or_init(path)?;
    let mut ids = Vec::new();

    for snapshot in snapshots {
        let stored = db.store_capture(snapshot)?;
        ids.push(stored.snapshot_id());
    }

    Ok(ids)
}

fn run_cli(args: &[&str]) -> process::Output {
    Command::new(env!("CARGO_BIN_EXE_clipmem"))
        .args(args)
        .output()
        .expect("clipmem binary should execute")
}

#[test]
fn search_command_prints_literal_results() -> Result<()> {
    let path = temp_db_path("search-text");
    seed_database(
        &path,
        &[
            text_snapshot(1, "Discount: 50 percent off"),
            text_snapshot(2, "Discount: 50%"),
        ],
    )?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "search",
        "--mode",
        "literal",
        "50%",
    ]);
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");

    assert!(output.status.success());
    assert!(stdout.contains("preview: Discount: 50%"));
    assert!(!stdout.contains("Discount: 50 percent off"));

    cleanup_db(&path);
    Ok(())
}

#[test]
fn recent_command_emits_json_hits() -> Result<()> {
    let path = temp_db_path("recent-json");
    let ids = seed_database(&path, &[text_snapshot(1, "git status")])?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "recent",
        "--json",
    ]);
    let hits: Vec<Value> =
        serde_json::from_slice(&output.stdout).expect("recent JSON output should parse");

    assert!(output.status.success());
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["snapshot_id"].as_i64(), Some(ids[0]));
    assert_eq!(hits[0]["preview_text"].as_str(), Some("git status"));

    cleanup_db(&path);
    Ok(())
}

#[test]
fn get_command_prints_snapshot_details() -> Result<()> {
    let path = temp_db_path("get-text");
    let ids = seed_database(
        &path,
        &[text_snapshot(1, "git clone https://example.com/repo")],
    )?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "get",
        &ids[0].to_string(),
    ]);
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");

    assert!(output.status.success());
    assert!(stdout.contains(&format!("snapshot {}", ids[0])));
    assert!(stdout.contains("item 0 · kind=plain_text"));
    assert!(stdout.contains("text: git clone https://example.com/repo"));

    cleanup_db(&path);
    Ok(())
}

#[test]
fn doctor_command_reports_database_capabilities() {
    let path = temp_db_path("doctor-text");
    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "doctor",
    ]);
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");

    assert!(output.status.success());
    assert!(stdout.contains("database:"));
    assert!(stdout.contains("sqlite version:"));
    assert!(stdout.contains("fts5 temp table creation works:"));

    cleanup_db(&path);
}

#[test]
fn recent_command_rejects_zero_limit() {
    let path = temp_db_path("recent-zero-limit");
    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "recent",
        "--limit",
        "0",
    ]);
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");

    assert!(!output.status.success());
    assert!(stderr.contains('0'));

    cleanup_db(&path);
}
