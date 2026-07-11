use anyhow::{Context, Result};
use rusqlite::{params, Transaction, TransactionBehavior};

use crate::db::Database;

pub(crate) struct RestoreSuppressionTransaction<'connection> {
    tx: Option<Transaction<'connection>>,
    operation_id: String,
}

impl Database {
    pub(crate) fn begin_restore_suppression(
        &mut self,
        snapshot_id: i64,
        snapshot_sha256: &str,
    ) -> Result<RestoreSuppressionTransaction<'_>> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("begin restore suppression transaction")?;
        super::capture::expire_restore_operations(&tx)?;
        let operation_id = tx
            .query_row(
                "INSERT INTO restore_operations (
                    operation_id, snapshot_id, snapshot_sha256, state, expires_at
                 ) VALUES (
                    lower(hex(randomblob(16))), ?1, ?2, 'preparing', datetime('now', '+5 seconds')
                 ) RETURNING operation_id",
                params![snapshot_id, snapshot_sha256],
                |row| row.get(0),
            )
            .context("insert restore suppression operation")?;
        Ok(RestoreSuppressionTransaction {
            tx: Some(tx),
            operation_id,
        })
    }
}

impl RestoreSuppressionTransaction<'_> {
    pub(crate) fn operation_id(&self) -> &str {
        &self.operation_id
    }

    pub(crate) fn register_generation(&self, change_count: i64) -> Result<()> {
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

    pub(crate) fn finish_written(mut self, change_count: i64) -> Result<String> {
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
        self.commit()?;
        Ok(self.operation_id)
    }

    pub(crate) fn finish_failed(mut self, failure: &str) -> Result<String> {
        self.tx()
            .execute(
                "UPDATE restore_operations
                 SET state = 'failed', failure = ?2,
                     expires_at = datetime('now', '+30 seconds')
                 WHERE operation_id = ?1",
                params![self.operation_id, failure],
            )
            .context("mark restore suppression operation failed")?;
        self.commit()?;
        Ok(self.operation_id)
    }

    fn tx(&self) -> &Transaction<'_> {
        self.tx.as_ref().expect("restore transaction is active")
    }

    fn commit(&mut self) -> Result<()> {
        self.tx
            .take()
            .expect("restore transaction is active")
            .commit()
            .context("commit restore suppression transaction")
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use anyhow::Result;

    use super::*;
    use crate::capture_service::CaptureApplicationService;
    use crate::db::{CaptureMode, CaptureOutcome};
    use crate::model::{build_item, build_representation, build_snapshot, CaptureContext};

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
            "clipmem-restore-ordering-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory)?;
        let path = directory.join("archive.sqlite3");

        let mut restore_db = Database::open_or_init(&path)?;
        let stored = restore_db.store_capture(&snapshot(1))?;
        let sha = restore_db
            .find_snapshot(stored.snapshot_id(), 1)?
            .expect("stored snapshot exists")
            .sha256()
            .to_string();
        let mut watcher_db = Database::open_read_write_current(&path)?;

        let suppression = restore_db.begin_restore_suppression(stored.snapshot_id(), &sha)?;
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
}
