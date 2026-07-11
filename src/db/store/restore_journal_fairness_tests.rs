use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;

use super::*;

static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

#[test]
fn unrelated_root_pressure_cannot_starve_owned_shards_across_runs() -> Result<()> {
    let suffix = NEXT_PATH.fetch_add(1, Ordering::Relaxed);
    let db_path = std::env::temp_dir().join(format!(
        "clipmem-restore-fairness-{}-{suffix}.db",
        std::process::id()
    ));
    let directory = journal_directory(&db_path);
    fs::create_dir_all(&directory)?;
    preflight_shards(&directory)?;
    for index in 0..(MAX_STARTUP_INSPECTIONS + 50) {
        fs::write(directory.join(format!("preserved-{index}")), b"unrelated")?;
    }
    let expired_marker = marker_path(&directory, 1);
    fs::write(
        &expired_marker,
        format!(
            "{MARKER_VERSION}\noperation_id={}\nexpires_unix=0\ngeneration=1\n",
            "b".repeat(32)
        ),
    )?;
    let operation_id = "a".repeat(32);
    let expired_journal = journal_path(&directory, &operation_id);
    fs::write(
        &expired_journal,
        format!("{JOURNAL_VERSION}\noperation_id={operation_id}\nexpires_unix=0\n"),
    )?;
    let stale_temp = expired_journal
        .parent()
        .unwrap()
        .join(format!("{operation_id}.tmp-0-1-0"));
    fs::write(&stale_temp, b"partial")?;

    for _ in 0..RECOVERY_SHARDS {
        cleanup_recovery_directory(&directory);
    }

    assert!(!expired_marker.exists());
    assert!(!expired_journal.exists());
    assert!(!stale_temp.exists());
    assert!(directory.join("preserved-0").exists());
    fs::remove_dir_all(directory)?;
    Ok(())
}
