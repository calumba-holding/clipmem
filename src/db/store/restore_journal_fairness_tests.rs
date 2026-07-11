use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;

use super::*;

static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

fn test_directory(label: &str) -> PathBuf {
    let suffix = NEXT_PATH.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "clipmem-restore-{label}-{}-{suffix}",
        std::process::id()
    ))
}

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
    let mut expired_markers = Vec::new();
    for generation in [1, 65, 961] {
        let path = marker_path(&directory, generation);
        fs::write(
            &path,
            format!(
                "{MARKER_VERSION}\noperation_id={}\nexpires_unix=0\ngeneration={generation}\n",
                "b".repeat(32)
            ),
        )?;
        expired_markers.push(path);
    }
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

    fs::write(
        cursor_directory(&directory).join("cleanup-cursor"),
        format!("{CLEANUP_WINDOWS_PER_SHARD}\n"),
    )?;
    for _ in 0..CLEANUP_WINDOWS_PER_SHARD {
        cleanup_recovery_directory(&directory);
    }
    fs::write(
        cursor_directory(&directory).join("cleanup-cursor"),
        format!("{}\n", 42 * CLEANUP_WINDOWS_PER_SHARD),
    )?;
    for _ in 0..CLEANUP_WINDOWS_PER_SHARD {
        cleanup_recovery_directory(&directory);
    }

    assert!(expired_markers.iter().all(|path| !path.exists()));
    assert!(!expired_journal.exists());
    assert!(!stale_temp.exists());
    assert!(directory.join("preserved-0").exists());
    fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn corrupt_cursor_recovers_and_stale_cursor_temp_is_cleaned() -> Result<()> {
    let directory = test_directory("cursor");
    fs::create_dir_all(&directory)?;
    preflight_shards(&directory)?;
    let cursor = cursor_directory(&directory);
    fs::write(cursor.join("cleanup-cursor"), b"corrupt\n")?;
    let stale = cursor.join("cleanup-cursor.tmp-0-1-0");
    fs::write(&stale, b"partial")?;

    cleanup_recovery_directory(&directory);

    assert!(!stale.exists());
    assert_eq!(fs::read_to_string(cursor.join("cleanup-cursor"))?, "1\n");
    fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn failed_publication_removes_create_new_temporary_file() -> Result<()> {
    let directory = test_directory("publish-failure");
    fs::create_dir_all(&directory)?;
    let destination = directory.join("destination.journal");
    let temporary = directory.join("temporary");

    atomic_persist_with(&destination, b"content", [temporary.clone()], |_, _| {
        Err(std::io::Error::other("injected publish failure"))
    })
    .expect_err("injected publication failure is reported");

    assert!(!temporary.exists());
    assert!(!destination.exists());
    fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn stale_temporary_name_collision_retries_next_candidate() -> Result<()> {
    let directory = test_directory("collision");
    fs::create_dir_all(&directory)?;
    let destination = directory.join("destination.journal");
    let collision = directory.join("collision");
    let retry = directory.join("retry");
    fs::write(&collision, b"stale")?;

    atomic_persist_with(
        &destination,
        b"published",
        [collision.clone(), retry.clone()],
        |temporary, destination| fs::rename(temporary, destination),
    )?;

    assert_eq!(fs::read(&collision)?, b"stale");
    assert!(!retry.exists());
    assert_eq!(fs::read(&destination)?, b"published");
    fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn startup_cleanup_removes_owned_expired_journal_and_stale_temp_only() -> Result<()> {
    let directory = test_directory("expired-owned");
    fs::create_dir_all(&directory)?;
    preflight_shards(&directory)?;
    let operation_id = "a".repeat(32);
    let journal = journal_path(&directory, &operation_id);
    fs::write(
        &journal,
        format!("{JOURNAL_VERSION}\noperation_id={operation_id}\nexpires_unix=0\n"),
    )?;
    let temporary = journal
        .parent()
        .unwrap()
        .join(format!("{operation_id}.tmp-0-123-0"));
    fs::write(&temporary, b"partial")?;
    let unrelated = journal.parent().unwrap().join("notes.tmp-0-123-0");
    fs::write(&unrelated, b"keep")?;
    fs::write(
        cursor_directory(&directory).join("cleanup-cursor"),
        b"682\n",
    )?;

    cleanup_recovery_directory(&directory);

    assert!(!journal.exists());
    assert!(!temporary.exists());
    assert!(unrelated.exists());
    fs::remove_dir_all(directory)?;
    Ok(())
}
