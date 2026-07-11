use anyhow::{Context, Result};
use rusqlite::{params, Transaction, TransactionBehavior};

use crate::db::Database;

use super::restore_journal::RestoreRecoveryJournal;

pub(crate) struct RestoreSuppressionTransaction<'connection> {
    tx: Option<Transaction<'connection>>,
    operation_id: String,
    journal: Option<RestoreRecoveryJournal>,
}

impl Database {
    pub(crate) fn begin_restore_suppression(
        &mut self,
        snapshot_id: i64,
        snapshot_sha256: &str,
    ) -> Result<RestoreSuppressionTransaction<'_>> {
        let operation_id: String = self
            .conn
            .query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))
            .context("create restore suppression operation id")?;
        let journal = RestoreRecoveryJournal::create(&self.path, operation_id.clone())?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("begin restore suppression transaction")?;
        super::capture::expire_restore_operations(&tx)?;
        tx.execute(
            "INSERT INTO restore_operations (
                    operation_id, snapshot_id, snapshot_sha256, state, expires_at
                 ) VALUES (
                    ?1, ?2, ?3, 'preparing', datetime('now', '+5 seconds')
                 )",
            params![operation_id, snapshot_id, snapshot_sha256],
        )
        .context("insert restore suppression operation")?;
        Ok(RestoreSuppressionTransaction {
            tx: Some(tx),
            operation_id,
            journal: Some(journal),
        })
    }
}

impl RestoreSuppressionTransaction<'_> {
    pub(crate) fn operation_id(&self) -> &str {
        &self.operation_id
    }

    pub(crate) fn register_generation(&mut self, change_count: i64) -> Result<()> {
        self.journal
            .as_mut()
            .expect("restore recovery journal is active")
            .register_generation(change_count)?;
        self.tx()
            .execute(
                "INSERT OR IGNORE INTO restore_operation_generations (
                    operation_id, change_count
                 ) VALUES (?1, ?2)",
                params![self.operation_id, change_count],
            )
            .context("register restore-created pasteboard generation")?;
        Ok(())
    }

    pub(crate) fn finish_written(self, change_count: i64) -> Result<String> {
        self.finish_written_with_commit(change_count, |tx| Ok(tx.commit()?))
    }

    fn finish_written_with_commit<F>(mut self, change_count: i64, commit: F) -> Result<String>
    where
        F: FnOnce(Transaction<'_>) -> Result<()>,
    {
        self.register_generation(change_count)?;
        self.tx()
            .execute(
                "UPDATE restore_operations
                 SET state = 'written', result_change_count = ?2,
                     expires_at = datetime('now', '+30 seconds')
                 WHERE operation_id = ?1",
                params![self.operation_id, change_count],
            )
            .context("mark restore suppression operation written")?;
        self.commit_with(commit)?;
        Ok(self.operation_id)
    }

    pub(crate) fn finish_failed(self, failure: &str) -> Result<String> {
        self.finish_failed_with_commit(failure, |tx| Ok(tx.commit()?))
    }

    fn finish_failed_with_commit<F>(mut self, failure: &str, commit: F) -> Result<String>
    where
        F: FnOnce(Transaction<'_>) -> Result<()>,
    {
        self.tx()
            .execute(
                "UPDATE restore_operations
                 SET state = 'failed', failure = ?2,
                     expires_at = datetime('now', '+30 seconds')
                 WHERE operation_id = ?1",
                params![self.operation_id, failure],
            )
            .context("mark restore suppression operation failed")?;
        self.commit_with(commit)?;
        Ok(self.operation_id)
    }

    fn tx(&self) -> &Transaction<'_> {
        self.tx.as_ref().expect("restore transaction is active")
    }

    fn commit_with<F>(&mut self, commit: F) -> Result<()>
    where
        F: FnOnce(Transaction<'_>) -> Result<()>,
    {
        let tx = self.tx.take().expect("restore transaction is active");
        commit(tx).context("commit restore suppression transaction")?;
        self.journal
            .take()
            .expect("restore recovery journal is active")
            .remove()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    use anyhow::Result;

    use super::*;
    use crate::capture_service::CaptureApplicationService;
    use crate::db::{CaptureMode, CaptureOutcome};
    use crate::model::{build_item, build_representation, build_snapshot, CaptureContext};

    static TEST_ID: AtomicU64 = AtomicU64::new(1);

    fn snapshot(change_count: i64) -> crate::model::ClipboardSnapshot {
        build_snapshot(
            CaptureContext::new(change_count),
            vec![build_item(
                0,
                vec![build_representation(
                    "public.utf8-plain-text".into(),
                    None,
                    b"restored".to_vec(),
                )],
            )],
        )
    }

    #[test]
    fn watcher_waits_until_exact_generation_is_durably_suppressible() -> Result<()> {
        let directory = std::env::temp_dir().join(format!(
            "clipmem-restore-ordering-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos(),
            TEST_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory)?;
        let path = directory.join("archive.sqlite3");

        let mut restore_db = Database::open_or_init(&path)?;
        let stored = restore_db.store_capture(&snapshot(1))?;
        let snapshot_id = stored.snapshot_id();
        let sha = restore_db
            .find_snapshot(snapshot_id, 1)?
            .expect("stored snapshot exists")
            .sha256()
            .to_string();
        let mut watcher_db = Database::open_read_write_current(&path)?;

        let mut suppression = restore_db.begin_restore_suppression(stored.snapshot_id(), &sha)?;
        suppression.register_generation(2)?;

        let (started_sender, started_receiver) = mpsc::channel();
        let (sender, receiver) = mpsc::channel();
        let watcher = std::thread::spawn(move || {
            started_sender
                .send(())
                .expect("start receiver remains available");
            let outcome = CaptureApplicationService::new(&mut watcher_db)
                .capture(&snapshot(2), CaptureMode::Watch);
            sender.send(outcome).expect("receiver remains available");
        });

        started_receiver.recv_timeout(Duration::from_secs(1))?;
        assert!(matches!(
            receiver.recv_timeout(Duration::from_millis(100)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        suppression.finish_written(2)?;

        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(2))??,
            CaptureOutcome::SuppressedRestore { .. }
        ));
        watcher.join().expect("watcher does not panic");

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("sqlite3-wal"));
        let _ = std::fs::remove_file(path.with_extension("sqlite3-shm"));
        let _ = std::fs::remove_dir(directory);
        Ok(())
    }

    #[test]
    fn written_commit_failure_falls_back_to_durable_generation_journal() -> Result<()> {
        let (directory, mut restore_db, mut watcher_db, snapshot_id, sha) = test_databases()?;
        let mut suppression = restore_db.begin_restore_suppression(snapshot_id, &sha)?;
        suppression.register_generation(2)?;

        let (started_sender, started_receiver) = mpsc::channel();
        let (sender, receiver) = mpsc::channel();
        let watcher = std::thread::spawn(move || {
            started_sender
                .send(())
                .expect("start receiver remains available");
            let outcome = CaptureApplicationService::new(&mut watcher_db)
                .capture(&snapshot(2), CaptureMode::Watch);
            sender.send(outcome).expect("receiver remains available");
        });
        started_receiver.recv_timeout(Duration::from_secs(1))?;
        assert!(matches!(
            receiver.recv_timeout(Duration::from_millis(100)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));

        let error = suppression
            .finish_written_with_commit(2, |_tx| anyhow::bail!("injected commit failure"))
            .expect_err("commit failure is reported");
        assert!(format!("{error:#}").contains("injected commit failure"));
        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(2))??,
            CaptureOutcome::SuppressedRestore { .. }
        ));
        watcher.join().expect("watcher does not panic");

        let operation_count: i64 =
            restore_db
                .conn
                .query_row("SELECT COUNT(*) FROM restore_operations", [], |row| {
                    row.get(0)
                })?;
        assert_eq!(operation_count, 0, "SQLite transaction rolled back");
        let path = directory.join("archive.sqlite3");
        let mut external_db = Database::open_read_write_current(&path)?;
        assert!(matches!(
            CaptureApplicationService::new(&mut external_db)
                .capture(&snapshot(3), CaptureMode::Watch)?,
            CaptureOutcome::ObservedExisting { .. }
        ));
        drop((restore_db, external_db));
        std::fs::remove_dir_all(directory)?;
        Ok(())
    }

    #[test]
    fn failed_rollback_commit_failure_preserves_both_exact_generations() -> Result<()> {
        let (directory, mut restore_db, mut watcher_db, snapshot_id, sha) = test_databases()?;
        let mut suppression = restore_db.begin_restore_suppression(snapshot_id, &sha)?;
        suppression.register_generation(2)?;
        suppression.register_generation(3)?;
        suppression
            .finish_failed_with_commit("write failed", |_tx| {
                anyhow::bail!("injected failed-state commit failure")
            })
            .expect_err("failed-state commit failure is reported");

        for generation in [2, 3] {
            assert!(matches!(
                CaptureApplicationService::new(&mut watcher_db)
                    .capture(&snapshot(generation), CaptureMode::Watch)?,
                CaptureOutcome::SuppressedRestore { .. }
            ));
        }
        assert!(matches!(
            CaptureApplicationService::new(&mut watcher_db)
                .capture(&snapshot(4), CaptureMode::Watch)?,
            CaptureOutcome::ObservedExisting { .. }
        ));
        drop((restore_db, watcher_db));
        std::fs::remove_dir_all(directory)?;
        Ok(())
    }

    fn test_databases() -> Result<(PathBuf, Database, Database, i64, String)> {
        let directory = std::env::temp_dir().join(format!(
            "clipmem-restore-commit-fault-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos(),
            TEST_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory)?;
        let path = directory.join("archive.sqlite3");
        let mut restore_db = Database::open_or_init(&path)?;
        let stored = restore_db.store_capture(&snapshot(1))?;
        let snapshot_id = stored.snapshot_id();
        let sha = restore_db
            .find_snapshot(snapshot_id, 1)?
            .expect("stored snapshot exists")
            .sha256()
            .to_string();
        let watcher_db = Database::open_read_write_current(&path)?;
        Ok((directory, restore_db, watcher_db, snapshot_id, sha))
    }
}
