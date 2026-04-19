use anyhow::{Context, Result};
use rusqlite::{params, OptionalExtension, TransactionBehavior};

use crate::model::{CaptureStoreResult, ClipboardItem, ClipboardSnapshot};
use crate::sensitive;

use super::{
    row_usize, usize_to_i64, CapturePolicy, CaptureSettings, CaptureSkipReason,
    CaptureStoreOutcome, Database, OcrCandidate, OcrStatusReport, PurgeReport,
    SnapshotDeletionReport,
};

impl Database {
    /// Store a captured clipboard snapshot and append a capture event for the observation.
    ///
    /// # Errors
    ///
    /// Returns an error if any transaction step fails, including snapshot, item,
    /// representation, or capture-event inserts, or the final commit.
    pub fn store_capture(&mut self, snapshot: &ClipboardSnapshot) -> Result<CaptureStoreResult> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("begin capture transaction")?;

        let inserted_snapshot_id: Option<i64> = tx
            .query_row(
                "INSERT INTO snapshots (
                    sha256,
                    snapshot_kind,
                    preview_text,
                    search_text,
                    item_count,
                    total_bytes
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(sha256) DO NOTHING
                RETURNING id",
                params![
                    snapshot.fingerprint(),
                    snapshot.snapshot_kind().as_str(),
                    snapshot.preview_text(),
                    snapshot.search_text(),
                    usize_to_i64(snapshot.item_count())?,
                    usize_to_i64(snapshot.total_bytes())?,
                ],
                |row| row.get(0),
            )
            .optional()
            .context("insert snapshot row")?;

        let snapshot_id = if let Some(id) = inserted_snapshot_id {
            for item in snapshot.items() {
                insert_item(&tx, id, item)
                    .with_context(|| format!("insert snapshot item {}", item.item_index()))?;
            }

            id
        } else {
            tx.query_row(
                "SELECT id FROM snapshots WHERE sha256 = ?1",
                [snapshot.fingerprint()],
                |row| row.get(0),
            )
            .context("lookup existing snapshot by fingerprint")?
        };

        tx.execute(
            "INSERT INTO capture_events (
                snapshot_id,
                change_count,
                frontmost_app_bundle_id,
                frontmost_app_name
            ) VALUES (?1, ?2, ?3, ?4)",
            params![
                snapshot_id,
                snapshot.change_count(),
                snapshot.frontmost_app_bundle_id(),
                snapshot.frontmost_app_name(),
            ],
        )
        .context("insert capture event row")?;
        let event_id = tx.last_insert_rowid();

        tx.commit().context("commit capture transaction")?;

        Ok(CaptureStoreResult::new(
            snapshot_id,
            event_id,
            inserted_snapshot_id.is_some(),
        ))
    }

    pub(crate) fn store_capture_if_allowed(
        &mut self,
        snapshot: &ClipboardSnapshot,
    ) -> Result<CaptureStoreOutcome> {
        let settings = self.capture_settings()?;
        if settings.api_key_filter_enabled()
            && sensitive::should_skip_snapshot_for_api_key_filter(snapshot)
        {
            return Ok(CaptureStoreOutcome::Skipped(
                CaptureSkipReason::ApiKeyFilter,
            ));
        }

        self.store_capture(snapshot)
            .map(CaptureStoreOutcome::Stored)
    }

    pub(crate) fn capture_settings(&self) -> Result<CaptureSettings> {
        self.conn
            .query_row(
                "SELECT paused, retention_seconds, api_key_filter_enabled, ocr_enabled FROM clipmem_settings WHERE id = 1",
                [],
                |row| {
                    let paused = row.get::<_, i64>(0)? != 0;
                    let retention_seconds = row
                        .get::<_, Option<i64>>(1)?
                        .map(u64::try_from)
                        .transpose()
                        .map_err(|source| {
                            rusqlite::Error::FromSqlConversionFailure(
                                1,
                                rusqlite::types::Type::Integer,
                                Box::new(source),
                            )
                        })?;
                    let api_key_filter_enabled = row.get::<_, i64>(2)? != 0;
                    let ocr_enabled = row.get::<_, i64>(3)? != 0;
                    Ok(CaptureSettings::new(
                        paused,
                        retention_seconds,
                        api_key_filter_enabled,
                        ocr_enabled,
                    ))
                },
            )
            .context("load capture settings")
    }

    pub(crate) fn capture_policy(&self) -> Result<CapturePolicy> {
        Ok(CapturePolicy::new(
            self.capture_settings()?,
            self.list_ignored_bundle_ids()?,
        ))
    }

    pub(crate) fn set_paused(&self, paused: bool) -> Result<CaptureSettings> {
        self.conn
            .execute(
                "UPDATE clipmem_settings SET paused = ?1 WHERE id = 1",
                [if paused { 1_i64 } else { 0_i64 }],
            )
            .context("update paused setting")?;
        self.capture_settings()
    }

    pub(crate) fn set_retention_seconds(
        &self,
        retention_seconds: Option<u64>,
    ) -> Result<CaptureSettings> {
        let retention_seconds = retention_seconds
            .map(i64::try_from)
            .transpose()
            .context("retention exceeds SQLite INTEGER range")?;
        self.conn
            .execute(
                "UPDATE clipmem_settings SET retention_seconds = ?1 WHERE id = 1",
                [retention_seconds],
            )
            .context("update retention setting")?;
        self.capture_settings()
    }

    pub(crate) fn set_api_key_filter_enabled(&self, enabled: bool) -> Result<CaptureSettings> {
        self.conn
            .execute(
                "UPDATE clipmem_settings SET api_key_filter_enabled = ?1 WHERE id = 1",
                [if enabled { 1_i64 } else { 0_i64 }],
            )
            .context("update api key filter setting")?;
        self.capture_settings()
    }

    pub(crate) fn set_ocr_enabled(&self, enabled: bool) -> Result<CaptureSettings> {
        self.conn
            .execute(
                "UPDATE clipmem_settings SET ocr_enabled = ?1 WHERE id = 1",
                [if enabled { 1_i64 } else { 0_i64 }],
            )
            .context("update ocr setting")?;
        self.capture_settings()
    }

    pub(crate) fn list_ignored_bundle_ids(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT bundle_id FROM ignored_bundle_ids ORDER BY bundle_id ASC")
            .context("prepare ignored bundle-id query")?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .context("execute ignored bundle-id query")?;
        super::collect_rows(rows).context("collect ignored bundle ids")
    }

    pub(crate) fn add_ignored_bundle_id(&self, bundle_id: &str) -> Result<bool> {
        let bundle_id = normalize_bundle_id(bundle_id)?;
        let changed = self
            .conn
            .execute(
                "INSERT OR IGNORE INTO ignored_bundle_ids (bundle_id) VALUES (?1)",
                [bundle_id],
            )
            .context("insert ignored bundle id")?;
        Ok(changed != 0)
    }

    pub(crate) fn remove_ignored_bundle_id(&self, bundle_id: &str) -> Result<bool> {
        let bundle_id = normalize_bundle_id(bundle_id)?;
        let changed = self
            .conn
            .execute(
                "DELETE FROM ignored_bundle_ids WHERE bundle_id = ?1",
                [bundle_id],
            )
            .context("delete ignored bundle id")?;
        Ok(changed != 0)
    }

    pub(crate) fn bundle_id_is_ignored(&self, bundle_id: Option<&str>) -> Result<bool> {
        let Some(bundle_id) = bundle_id else {
            return Ok(false);
        };
        let bundle_id = normalize_bundle_id(bundle_id)?;
        self.conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM ignored_bundle_ids WHERE bundle_id = ?1)",
                [bundle_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|value| value != 0)
            .context("check ignored bundle id")
    }

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
                        WHERE datetime(last_observed_at) < datetime('now', printf('-%d seconds', ?1))
                    )
                ",
                [older_than_seconds_i64],
            )
            .context("delete expired snapshots")?;
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

    pub(crate) fn enqueue_ocr_for_snapshot(&mut self, snapshot_id: i64) -> Result<usize> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("begin ocr enqueue transaction")?;
        let inserted = enqueue_ocr_for_snapshot_tx(&tx, snapshot_id)?;
        rebuild_snapshot_ocr_cache(&tx, snapshot_id)?;
        tx.commit().context("commit ocr enqueue transaction")?;
        Ok(inserted)
    }

    pub(crate) fn next_ocr_candidates(
        &mut self,
        limit: usize,
        snapshot_id: Option<i64>,
        retry_failed: bool,
    ) -> Result<Vec<OcrCandidate>> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("begin ocr candidate transaction")?;

        enqueue_ocr_candidates_tx(&tx, snapshot_id)?;
        if retry_failed {
            tx.execute(
                r"
                    UPDATE ocr_results
                    SET status = 'pending',
                        error = NULL,
                        updated_at = CURRENT_TIMESTAMP
                    WHERE status = 'failed'
                      AND (
                          ?1 IS NULL
                          OR EXISTS (
                              SELECT 1
                              FROM item_representations ir
                              WHERE ir.raw_sha256 = ocr_results.raw_sha256
                                AND ir.snapshot_id = ?1
                          )
                      )
                ",
                [snapshot_id],
            )
            .context("requeue failed ocr results")?;
        }

        let limit = usize_to_i64(super::sanitise_limit(limit))?;
        let mut stmt = tx
            .prepare(
                r"
                    WITH candidate_hashes AS (
                        SELECT o.raw_sha256
                        FROM ocr_results o
                        WHERE o.status = 'pending'
                          AND (
                              ?1 IS NULL
                              OR EXISTS (
                                  SELECT 1
                                  FROM item_representations ir
                                  WHERE ir.raw_sha256 = o.raw_sha256
                                    AND ir.snapshot_id = ?1
                              )
                          )
                        ORDER BY o.updated_at ASC, o.raw_sha256 ASC
                        LIMIT ?2
                    )
                    SELECT
                        c.raw_sha256,
                        (
                            SELECT ir.blob_value
                            FROM item_representations ir
                            WHERE ir.raw_sha256 = c.raw_sha256
                              AND ir.kind = 'image'
                              AND length(ir.blob_value) > 0
                            ORDER BY ir.byte_len DESC
                            LIMIT 1
                        ) AS blob_value,
                        (
                            SELECT COUNT(DISTINCT ir.snapshot_id)
                            FROM item_representations ir
                            WHERE ir.raw_sha256 = c.raw_sha256
                        ) AS snapshot_count
                    FROM candidate_hashes c
                ",
            )
            .context("prepare ocr candidate query")?;
        let rows = stmt
            .query_map(params![snapshot_id, limit], |row| {
                Ok(OcrCandidate::new(
                    row.get(0)?,
                    row.get(1)?,
                    row_usize(row, 2)?,
                ))
            })
            .context("execute ocr candidate query")?;
        let candidates = super::collect_rows(rows).context("collect ocr candidates")?;
        drop(stmt);
        tx.commit().context("commit ocr candidate transaction")?;
        Ok(candidates)
    }

    pub(crate) fn store_ocr_text(
        &mut self,
        raw_sha256: &str,
        engine: &str,
        recognition_level: &str,
        text: &str,
    ) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("begin ocr result transaction")?;
        let status = if text.trim().is_empty() {
            "skipped"
        } else {
            "ready"
        };
        tx.execute(
            r"
                UPDATE ocr_results
                SET status = ?2,
                    engine = ?3,
                    recognition_level = ?4,
                    text_value = ?5,
                    error = NULL,
                    attempt_count = attempt_count + 1,
                    updated_at = CURRENT_TIMESTAMP
                WHERE raw_sha256 = ?1
            ",
            params![raw_sha256, status, engine, recognition_level, text.trim()],
        )
        .context("store ocr text result")?;
        rebuild_snapshot_ocr_cache_for_hash(&tx, raw_sha256)?;
        tx.commit().context("commit ocr result transaction")?;
        Ok(())
    }

    pub(crate) fn store_ocr_failure(
        &mut self,
        raw_sha256: &str,
        engine: &str,
        recognition_level: &str,
        error: &str,
    ) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("begin ocr failure transaction")?;
        tx.execute(
            r"
                UPDATE ocr_results
                SET status = 'failed',
                    engine = ?2,
                    recognition_level = ?3,
                    text_value = NULL,
                    error = ?4,
                    attempt_count = attempt_count + 1,
                    updated_at = CURRENT_TIMESTAMP
                WHERE raw_sha256 = ?1
            ",
            params![raw_sha256, engine, recognition_level, error],
        )
        .context("store ocr failure result")?;
        rebuild_snapshot_ocr_cache_for_hash(&tx, raw_sha256)?;
        tx.commit().context("commit ocr failure transaction")?;
        Ok(())
    }

    pub(crate) fn ocr_status_report(&self) -> Result<OcrStatusReport> {
        self.conn
            .query_row(
                r"
                    SELECT
                        COALESCE(SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END), 0),
                        COALESCE(SUM(CASE WHEN status = 'ready' THEN 1 ELSE 0 END), 0),
                        COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0),
                        COALESCE(SUM(CASE WHEN status = 'skipped' THEN 1 ELSE 0 END), 0),
                        (
                            SELECT COUNT(*)
                            FROM snapshot_ocr_cache
                            WHERE ocr_text != ''
                        )
                    FROM ocr_results
                ",
                [],
                |row| {
                    Ok(OcrStatusReport::new(
                        row_usize(row, 0)?,
                        row_usize(row, 1)?,
                        row_usize(row, 2)?,
                        row_usize(row, 3)?,
                        row_usize(row, 4)?,
                    ))
                },
            )
            .context("load ocr status report")
    }
}

fn enqueue_ocr_candidates_tx(
    tx: &rusqlite::Transaction<'_>,
    snapshot_id: Option<i64>,
) -> Result<usize> {
    tx.execute(
        r"
            INSERT INTO ocr_results (raw_sha256, status)
            SELECT DISTINCT ir.raw_sha256, 'pending'
            FROM item_representations ir
            WHERE ir.kind = 'image'
              AND length(ir.blob_value) > 0
              AND (?1 IS NULL OR ir.snapshot_id = ?1)
            ON CONFLICT(raw_sha256) DO NOTHING
        ",
        [snapshot_id],
    )
    .context("enqueue ocr candidates")
}

fn enqueue_ocr_for_snapshot_tx(tx: &rusqlite::Transaction<'_>, snapshot_id: i64) -> Result<usize> {
    enqueue_ocr_candidates_tx(tx, Some(snapshot_id))
}

fn rebuild_snapshot_ocr_cache_for_hash(
    tx: &rusqlite::Transaction<'_>,
    raw_sha256: &str,
) -> Result<()> {
    let mut stmt = tx
        .prepare(
            r"
                SELECT DISTINCT snapshot_id
                FROM item_representations
                WHERE raw_sha256 = ?1
            ",
        )
        .context("prepare affected ocr snapshot query")?;
    let rows = stmt
        .query_map([raw_sha256], |row| row.get::<_, i64>(0))
        .context("execute affected ocr snapshot query")?;
    let snapshot_ids = super::collect_rows(rows).context("collect affected ocr snapshots")?;
    drop(stmt);
    for snapshot_id in snapshot_ids {
        rebuild_snapshot_ocr_cache(tx, snapshot_id)?;
    }
    Ok(())
}

fn rebuild_snapshot_ocr_cache(tx: &rusqlite::Transaction<'_>, snapshot_id: i64) -> Result<()> {
    tx.execute(
        r"
            INSERT INTO snapshot_ocr_cache (snapshot_id, ocr_text, status, updated_at)
            SELECT
                s.id,
                COALESCE((
                        SELECT GROUP_CONCAT(text_value, char(10) || char(10))
                    FROM (
                        SELECT DISTINCT o.text_value AS text_value
                        FROM item_representations ir
                        JOIN ocr_results o ON o.raw_sha256 = ir.raw_sha256
                        WHERE ir.snapshot_id = s.id
                          AND o.status = 'ready'
                          AND o.text_value IS NOT NULL
                          AND o.text_value != ''
                        ORDER BY o.text_value
                    )
                ), '') AS ocr_text,
                CASE
                    WHEN EXISTS (
                        SELECT 1
                        FROM item_representations ir
                        JOIN ocr_results o ON o.raw_sha256 = ir.raw_sha256
                        WHERE ir.snapshot_id = s.id
                          AND o.status = 'ready'
                          AND o.text_value IS NOT NULL
                          AND o.text_value != ''
                    ) THEN 'ready'
                    WHEN EXISTS (
                        SELECT 1
                        FROM item_representations ir
                        JOIN ocr_results o ON o.raw_sha256 = ir.raw_sha256
                        WHERE ir.snapshot_id = s.id
                          AND o.status = 'pending'
                    ) THEN 'pending'
                    WHEN EXISTS (
                        SELECT 1
                        FROM item_representations ir
                        JOIN ocr_results o ON o.raw_sha256 = ir.raw_sha256
                        WHERE ir.snapshot_id = s.id
                          AND o.status = 'failed'
                    ) THEN 'failed'
                    ELSE 'skipped'
                END AS status,
                CURRENT_TIMESTAMP
            FROM snapshots s
            WHERE s.id = ?1
            ON CONFLICT(snapshot_id) DO UPDATE SET
                ocr_text = excluded.ocr_text,
                status = excluded.status,
                updated_at = excluded.updated_at
        ",
        [snapshot_id],
    )
    .context("rebuild snapshot ocr cache")?;
    Ok(())
}

fn insert_item(
    tx: &rusqlite::Transaction<'_>,
    snapshot_id: i64,
    item: &ClipboardItem,
) -> Result<()> {
    tx.execute(
        "INSERT INTO snapshot_items (
            snapshot_id,
            item_index,
            primary_kind,
            primary_uti,
            preview_text,
            search_text,
            total_bytes
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            snapshot_id,
            usize_to_i64(item.item_index())?,
            item.primary_kind().as_str(),
            item.primary_uti(),
            item.preview_text(),
            item.search_text(),
            usize_to_i64(item.total_bytes())?,
        ],
    )
    .with_context(|| format!("insert snapshot_items row for item {}", item.item_index()))?;

    for rep in item.representations() {
        tx.execute(
            "INSERT INTO item_representations (
                snapshot_id,
                item_index,
                uti,
                kind,
                byte_len,
                raw_sha256,
                text_value,
                blob_value
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                snapshot_id,
                usize_to_i64(item.item_index())?,
                rep.uti(),
                rep.kind().as_str(),
                usize_to_i64(rep.byte_len())?,
                rep.raw_sha256(),
                rep.text_value(),
                rep.raw_bytes(),
            ],
        )
        .with_context(|| {
            format!(
                "insert item_representations row for item {} and uti {}",
                item.item_index(),
                rep.uti()
            )
        })?;
    }

    Ok(())
}

fn normalize_bundle_id(bundle_id: &str) -> Result<String> {
    let normalized = bundle_id.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        anyhow::bail!("bundle id cannot be empty");
    }
    Ok(normalized)
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
                WHERE datetime(ss.last_observed_at) < datetime('now', printf('-%d seconds', ?1))
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

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::Database;
    use crate::model::{build_item, build_representation, build_snapshot, CaptureContext};

    fn fake_snapshot(change_count: i64, text: &str) -> crate::model::ClipboardSnapshot {
        build_snapshot(
            CaptureContext::new(change_count).with_frontmost_app_name("Editor"),
            vec![build_item(
                0,
                vec![build_representation(
                    "public.utf8-plain-text".to_string(),
                    None,
                    text.as_bytes().to_vec(),
                )],
            )],
        )
    }

    #[test]
    fn empty_snapshots_store_without_placeholder_rows() -> Result<()> {
        let mut db = Database::open_in_memory()?;
        let snapshot = build_snapshot(CaptureContext::new(9), Vec::new());

        let store = db.store_capture(&snapshot)?;
        let stored_item_count: i64 = db.conn.query_row(
            "SELECT item_count FROM snapshots WHERE id = ?1",
            [store.snapshot_id()],
            |row| row.get(0),
        )?;
        let stored_row_count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM snapshot_items WHERE snapshot_id = ?1",
            [store.snapshot_id()],
            |row| row.get(0),
        )?;

        assert_eq!(stored_item_count, 0);
        assert_eq!(stored_row_count, 0);
        Ok(())
    }

    #[test]
    fn duplicate_snapshots_reuse_content_row_and_append_events() -> Result<()> {
        let mut db = Database::open_in_memory()?;
        let first = fake_snapshot(1, "git status");
        let second = fake_snapshot(2, "git status");

        let first_store = db.store_capture(&first)?;
        let second_store = db.store_capture(&second)?;
        let event_count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM capture_events WHERE snapshot_id = ?1",
            [first_store.snapshot_id()],
            |row| row.get(0),
        )?;

        assert!(first_store.inserted_new_snapshot());
        assert!(!second_store.inserted_new_snapshot());
        assert_eq!(first_store.snapshot_id(), second_store.snapshot_id());
        assert_eq!(event_count, 2);
        Ok(())
    }
}
