use anyhow::{Context, Result};

use crate::db::{CaptureMode, CaptureOutcome, Database};
use crate::model::ClipboardSnapshot;

/// Single application boundary for capture policy and transactional storage.
pub(crate) struct CaptureApplicationService<'a> {
    db: &'a mut Database,
}

impl<'a> CaptureApplicationService<'a> {
    pub(crate) fn new(db: &'a mut Database) -> Self {
        Self { db }
    }

    pub(crate) fn capture(
        &mut self,
        snapshot: &ClipboardSnapshot,
        mode: CaptureMode,
    ) -> Result<CaptureOutcome> {
        self.db
            .capture_with_policy(snapshot, mode)
            .context("capture application transaction failed")
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use std::sync::{Arc, Barrier};

    use super::CaptureApplicationService;
    use crate::db::{CaptureMode, CaptureOutcome, Database};
    use crate::model::{build_item, build_representation, build_snapshot, CaptureContext};

    fn snapshot(change_count: i64, bundle_id: &str) -> crate::model::ClipboardSnapshot {
        build_snapshot(
            CaptureContext::new(change_count).with_frontmost_app_bundle_id(bundle_id),
            vec![build_item(
                0,
                vec![build_representation(
                    "public.utf8-plain-text".into(),
                    None,
                    b"hello".to_vec(),
                )],
            )],
        )
    }

    #[test]
    fn all_modes_enforce_pause_and_ignored_app_policy() -> Result<()> {
        for mode in [
            CaptureMode::Watch,
            CaptureMode::Manual,
            CaptureMode::SetupSeed,
        ] {
            let mut db = Database::open_in_memory()?;
            db.set_paused(true)?;
            let outcome = CaptureApplicationService::new(&mut db)
                .capture(&snapshot(1, "com.example.app"), mode)?;
            assert!(matches!(outcome, CaptureOutcome::SkippedPaused));

            db.set_paused(false)?;
            db.add_ignored_bundle_id("com.example.app")?;
            let outcome = CaptureApplicationService::new(&mut db)
                .capture(&snapshot(2, "com.example.app"), mode)?;
            assert!(matches!(outcome, CaptureOutcome::SkippedIgnoredApp { .. }));
        }
        Ok(())
    }

    #[test]
    fn all_modes_enforce_sensitive_filter_and_apply_ocr_policy() -> Result<()> {
        for mode in [
            CaptureMode::Watch,
            CaptureMode::Manual,
            CaptureMode::SetupSeed,
        ] {
            let mut db = Database::open_in_memory()?;
            db.set_api_key_filter_enabled(true)?;
            let sensitive = build_snapshot(
                CaptureContext::new(1),
                vec![build_item(
                    0,
                    vec![build_representation(
                        "public.utf8-plain-text".into(),
                        None,
                        b"Authorization: Bearer 8JfA-2mQpV_4tLz9XnR6cH0wKdS7yBu3".to_vec(),
                    )],
                )],
            );
            let outcome = CaptureApplicationService::new(&mut db).capture(&sensitive, mode)?;
            assert!(matches!(outcome, CaptureOutcome::SkippedSensitive));

            db.set_api_key_filter_enabled(false)?;
            db.set_ocr_enabled(true)?;
            let outcome = CaptureApplicationService::new(&mut db)
                .capture(&snapshot(2, "com.example.app"), mode)?;
            assert!(matches!(
                outcome,
                CaptureOutcome::Stored {
                    enqueue_ocr: true,
                    ..
                }
            ));
        }
        Ok(())
    }

    #[test]
    fn two_watcher_contenders_are_suppressed_but_next_generation_stores() -> Result<()> {
        let directory = std::env::temp_dir().join(format!(
            "clipmem-restore-race-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory)?;
        let path = directory.join("archive.sqlite3");
        let mut setup = Database::open_or_init(&path)?;
        let original = setup.store_capture(&snapshot(1, "com.example.app"))?;
        let sha = setup
            .find_snapshot(original.snapshot_id(), 1)?
            .unwrap()
            .sha256()
            .to_string();
        let operation = setup.begin_restore_operation(original.snapshot_id(), &sha, 2)?;
        setup.mark_restore_written(&operation, 2)?;
        drop(setup);

        let barrier = Arc::new(Barrier::new(3));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let path = path.clone();
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || -> Result<CaptureOutcome> {
                let mut db = Database::open_read_write_current(&path)?;
                barrier.wait();
                CaptureApplicationService::new(&mut db)
                    .capture(&snapshot(2, "com.example.app"), CaptureMode::Watch)
            }));
        }
        barrier.wait();
        for handle in handles {
            assert!(matches!(
                handle.join().expect("watcher thread should not panic")?,
                CaptureOutcome::SuppressedRestore { .. }
            ));
        }

        let mut db = Database::open_read_write_current(&path)?;
        let next = CaptureApplicationService::new(&mut db)
            .capture(&snapshot(3, "com.example.app"), CaptureMode::Watch)?;
        assert!(matches!(next, CaptureOutcome::ObservedExisting { .. }));
        let event_count: i64 =
            db.conn
                .query_row("SELECT COUNT(*) FROM capture_events", [], |row| row.get(0))?;
        assert_eq!(event_count, 2);
        drop(db);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("sqlite3-wal"));
        let _ = std::fs::remove_file(path.with_extension("sqlite3-shm"));
        let _ = std::fs::remove_dir(directory);
        Ok(())
    }
}
