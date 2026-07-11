use anyhow::{Context, Result};
use rusqlite::{OptionalExtension, TransactionBehavior};

use super::revision::bump_revision_tx;
use crate::db::sqlite_helpers::row_usize;
use crate::db::types::{ArchiveChangeKind, Database, PurgeReport, SnapshotDeletionReport};

impl Database {
    pub(crate) fn forget_snapshot(
        &mut self,
        snapshot_id: i64,
    ) -> Result<Option<SnapshotDeletionReport>> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("begin forget transaction")?;

        let report = load_snapshot_deletion_report(&tx, snapshot_id)?;
        if report.is_none() {
            tx.commit().context("commit empty forget transaction")?;
            return Ok(None);
        }

        tx.execute("DELETE FROM snapshots WHERE id = ?1", [snapshot_id])
            .context("delete snapshot")?;
        delete_unreferenced_ocr_results(&tx)?;
        let deleted_derivatives = delete_unreferenced_representation_derivatives(&tx)?;
        bump_deletion_revisions(&tx, deleted_derivatives)?;
        tx.commit().context("commit forget transaction")?;
        Ok(report)
    }

    pub(crate) fn purge_snapshots_older_than(
        &mut self,
        older_than_seconds: u64,
        dry_run: bool,
    ) -> Result<PurgeReport> {
        let older_than_seconds_i64 =
            i64::try_from(older_than_seconds).context("duration exceeds SQLite INTEGER range")?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("begin purge transaction")?;

        let report = load_purge_report(&tx, older_than_seconds)?;
        if !dry_run && report.snapshot_count() > 0 {
            tx.execute(
                r"
                    DELETE FROM snapshots
                    WHERE id IN (
                        SELECT snapshot_id
                        FROM snapshot_stats
                        WHERE last_observed_at < datetime('now', printf('-%d seconds', ?1))
                    )
                ",
                [older_than_seconds_i64],
            )
            .context("delete expired snapshots")?;
            delete_unreferenced_ocr_results(&tx)?;
            let deleted_derivatives = delete_unreferenced_representation_derivatives(&tx)?;
            bump_deletion_revisions(&tx, deleted_derivatives)?;
        }

        tx.commit().context("commit purge transaction")?;
        Ok(PurgeReport::new(
            older_than_seconds,
            dry_run,
            report.snapshot_count(),
            report.item_count(),
            report.representation_count(),
            report.capture_event_count(),
            report.total_bytes(),
        ))
    }

    pub(crate) fn apply_retention_policy(&mut self) -> Result<Option<PurgeReport>> {
        let retention_seconds = self.capture_settings()?.retention_seconds();
        retention_seconds
            .map(|seconds| self.purge_snapshots_older_than(seconds, false))
            .transpose()
    }
}

fn delete_unreferenced_representation_derivatives(tx: &rusqlite::Transaction<'_>) -> Result<usize> {
    tx.execute(
        r"
            DELETE FROM representation_derivatives
            WHERE NOT EXISTS (
                SELECT 1
                FROM item_representations ir
                WHERE ir.raw_sha256 = representation_derivatives.source_raw_sha256
            )
        ",
        [],
    )
    .context("delete unreferenced representation derivatives")
}

fn bump_deletion_revisions(
    tx: &rusqlite::Transaction<'_>,
    deleted_derivatives: usize,
) -> Result<()> {
    if deleted_derivatives == 0 {
        bump_revision_tx(tx, &[ArchiveChangeKind::ArchiveContent])
    } else {
        bump_revision_tx(
            tx,
            &[
                ArchiveChangeKind::ArchiveContent,
                ArchiveChangeKind::Storage,
            ],
        )
    }
}

fn delete_unreferenced_ocr_results(tx: &rusqlite::Transaction<'_>) -> Result<()> {
    tx.execute(
        r"
            DELETE FROM ocr_results
            WHERE NOT EXISTS (
                SELECT 1
                FROM item_representations ir
                WHERE ir.raw_sha256 = ocr_results.raw_sha256
            )
        ",
        [],
    )
    .context("delete unreferenced OCR results")?;
    Ok(())
}

fn load_snapshot_deletion_report(
    tx: &rusqlite::Transaction<'_>,
    snapshot_id: i64,
) -> Result<Option<SnapshotDeletionReport>> {
    tx.query_row(
        r"
            SELECT
                s.id,
                s.item_count,
                (
                    SELECT COUNT(*)
                    FROM item_representations ir
                    WHERE ir.snapshot_id = s.id
                ) AS representation_count,
                (
                    SELECT COUNT(*)
                    FROM capture_events ce
                    WHERE ce.snapshot_id = s.id
                ) AS capture_event_count,
                s.total_bytes
            FROM snapshots s
            WHERE s.id = ?1
        ",
        [snapshot_id],
        |row| {
            Ok(SnapshotDeletionReport::new(
                row.get(0)?,
                row_usize(row, 1)?,
                row_usize(row, 2)?,
                row_usize(row, 3)?,
                row_usize(row, 4)?,
            ))
        },
    )
    .optional()
    .context("load snapshot deletion report")
}

fn load_purge_report(
    tx: &rusqlite::Transaction<'_>,
    older_than_seconds: u64,
) -> Result<PurgeReport> {
    let older_than_seconds_i64 =
        i64::try_from(older_than_seconds).context("duration exceeds SQLite INTEGER range")?;
    tx.query_row(
        r"
            WITH candidates AS (
                SELECT ss.snapshot_id
                FROM snapshot_stats ss
                WHERE ss.last_observed_at < datetime('now', printf('-%d seconds', ?1))
            )
            SELECT
                COALESCE((SELECT COUNT(*) FROM candidates), 0) AS snapshot_count,
                COALESCE((
                    SELECT SUM(s.item_count)
                    FROM snapshots s
                    WHERE s.id IN (SELECT snapshot_id FROM candidates)
                ), 0) AS item_count,
                COALESCE((
                    SELECT COUNT(*)
                    FROM item_representations ir
                    WHERE ir.snapshot_id IN (SELECT snapshot_id FROM candidates)
                ), 0) AS representation_count,
                COALESCE((
                    SELECT COUNT(*)
                    FROM capture_events ce
                    WHERE ce.snapshot_id IN (SELECT snapshot_id FROM candidates)
                ), 0) AS capture_event_count,
                COALESCE((
                    SELECT SUM(s.total_bytes)
                    FROM snapshots s
                    WHERE s.id IN (SELECT snapshot_id FROM candidates)
                ), 0) AS total_bytes
        ",
        [older_than_seconds_i64],
        |row| {
            Ok(PurgeReport::new(
                older_than_seconds,
                false,
                row_usize(row, 0)?,
                row_usize(row, 1)?,
                row_usize(row, 2)?,
                row_usize(row, 3)?,
                row_usize(row, 4)?,
            ))
        },
    )
    .context("load purge report")
}
