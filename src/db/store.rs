use std::io::Cursor;
use std::path::Path;

use anyhow::{ensure, Context, Result};
use image::{ExtendedColorType, ImageEncoder, ImageReader, Limits};
use rusqlite::{params, OptionalExtension, TransactionBehavior};

use crate::model::{
    hash_bytes, truncate_chars, CaptureStoreResult, ClipboardItem, ClipboardSnapshot,
};
use crate::sensitive;

use super::{
    row_usize, usize_to_i64, CapturePolicy, CaptureSettings, CaptureSkipReason,
    CaptureStoreOutcome, Database, ImageOptimizationReport, OcrCandidate, OcrStatusReport,
    PurgeReport, SnapshotDeletionReport,
};

const WEBP_UTI: &str = "org.webmproject.webp";
const IMAGE_OPTIMIZATION_FORMAT: &str = "webp_lossless";
const IMAGE_OPTIMIZATION_MIN_ABSOLUTE_SAVINGS: usize = 64 * 1024;
const IMAGE_OPTIMIZATION_MIN_RELATIVE_SAVINGS_NUMERATOR: usize = 1;
const IMAGE_OPTIMIZATION_MIN_RELATIVE_SAVINGS_DENOMINATOR: usize = 10;
const IMAGE_OPTIMIZATION_MAX_DIMENSION: u32 = 16_384;

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

    pub(crate) fn optimize_images(
        &mut self,
        dry_run: bool,
        limit: usize,
        auto_compact: bool,
    ) -> Result<ImageOptimizationReport> {
        let initial_file_bytes = if database_path_is_file_backed(&self.path) {
            Some(super::storage_file_sizes(&self.path)?.total_bytes())
        } else {
            None
        };
        let candidates = load_image_optimization_candidates(&self.conn, limit)?;
        let mut report = ImageOptimizationReport {
            dry_run,
            format: IMAGE_OPTIMIZATION_FORMAT,
            scanned_rows: 0,
            compressed_rows: 0,
            skipped_rows: 0,
            conflict_count: 0,
            original_bytes: 0,
            optimized_bytes: 0,
            logical_saved_bytes: 0,
            compact_run: false,
            compact: None,
            compact_error: None,
            filesystem_saved_bytes: 0,
            filesystem_growth_bytes: 0,
            compact_recommended: false,
        };

        for candidate in candidates {
            report.scanned_rows += 1;
            let optimized = match encode_candidate_as_lossless_webp(&candidate) {
                Ok(optimized) => optimized,
                Err(reason) => {
                    report.skipped_rows += 1;
                    if !dry_run {
                        mark_image_optimization_skipped(&self.conn, &candidate, reason)?;
                    }
                    continue;
                }
            };

            let saved_bytes = candidate.byte_len.saturating_sub(optimized.bytes.len());
            if !image_optimization_is_beneficial(candidate.byte_len, optimized.bytes.len()) {
                report.skipped_rows += 1;
                if !dry_run {
                    mark_image_optimization_skipped(&self.conn, &candidate, "not_smaller")?;
                }
                continue;
            }

            if image_optimization_would_conflict(&self.conn, &candidate, &optimized)? {
                report.skipped_rows += 1;
                report.conflict_count += 1;
                if !dry_run {
                    mark_image_optimization_skipped(&self.conn, &candidate, "conflict")?;
                }
                continue;
            }

            report.compressed_rows += 1;
            report.original_bytes += candidate.byte_len;
            report.optimized_bytes += optimized.bytes.len();
            report.logical_saved_bytes += saved_bytes;
            report.compact_recommended = true;

            if !dry_run {
                replace_image_with_optimized_webp(&mut self.conn, &candidate, optimized)?;
            }
        }

        if !dry_run && auto_compact && database_path_is_file_backed(&self.path) {
            let compact_wanted = report.compact_recommended || storage_compaction_would_help(self)?;
            if compact_wanted {
                report.compact_run = true;
                match self.compact_storage(false) {
                    Ok(compact) => {
                        if let Some(initial) = initial_file_bytes {
                            report.filesystem_saved_bytes =
                                initial.saturating_sub(compact.total_after_bytes);
                            report.filesystem_growth_bytes =
                                compact.total_after_bytes.saturating_sub(initial);
                        }
                        report.compact_recommended = false;
                        report.compact = Some(compact);
                    }
                    Err(error) => {
                        report.compact_error = Some(format_compaction_error(&error));
                        report.compact_recommended = true;
                    }
                }
            } else if let Some(initial) = initial_file_bytes {
                let final_bytes = super::storage_file_sizes(&self.path)?.total_bytes();
                report.filesystem_saved_bytes = initial.saturating_sub(final_bytes);
                report.filesystem_growth_bytes = final_bytes.saturating_sub(initial);
            }
        } else if !dry_run && report.compact_recommended && !auto_compact {
            if let Some(initial) = initial_file_bytes {
                let final_bytes = super::storage_file_sizes(&self.path)?.total_bytes();
                report.filesystem_saved_bytes = initial.saturating_sub(final_bytes);
                report.filesystem_growth_bytes = final_bytes.saturating_sub(initial);
            }
        }

        Ok(report)
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

#[derive(Debug, Clone)]
struct ImageOptimizationCandidate {
    snapshot_id: i64,
    item_index: i64,
    uti: String,
    byte_len: usize,
    raw_sha256: String,
    blob_value: Vec<u8>,
}

#[derive(Debug, Clone)]
struct OptimizedImage {
    bytes: Vec<u8>,
    raw_sha256: String,
}

fn load_image_optimization_candidates(
    conn: &rusqlite::Connection,
    limit: usize,
) -> Result<Vec<ImageOptimizationCandidate>> {
    let limit = usize_to_i64(super::sanitise_limit(limit))?;
    let mut stmt = conn
        .prepare(
            r"
                SELECT snapshot_id, item_index, uti, byte_len, raw_sha256, blob_value
                FROM item_representations
                WHERE kind = 'image'
                  AND image_compression_status = 'uncompressed'
                  AND length(blob_value) > 0
                ORDER BY byte_len DESC, snapshot_id ASC, item_index ASC, uti ASC
                LIMIT ?1
            ",
        )
        .context("prepare image optimization candidate query")?;
    let rows = stmt
        .query_map([limit], |row| {
            Ok(ImageOptimizationCandidate {
                snapshot_id: row.get(0)?,
                item_index: row.get(1)?,
                uti: row.get(2)?,
                byte_len: row_usize(row, 3)?,
                raw_sha256: row.get(4)?,
                blob_value: row.get(5)?,
            })
        })
        .context("execute image optimization candidate query")?;
    super::collect_rows(rows).context("collect image optimization candidates")
}

fn database_path_is_file_backed(path: &Path) -> bool {
    path != Path::new(":memory:")
}

fn storage_compaction_would_help(db: &Database) -> Result<bool> {
    let file_sizes = super::storage_file_sizes(&db.path)?;
    let freelist_count = super::pragma_usize(&db.conn, "freelist_count")?;
    Ok(freelist_count > 0 || file_sizes.wal > 0 || file_sizes.shm > 0)
}

fn format_compaction_error(error: &anyhow::Error) -> String {
    let message = error.to_string();
    let lower = message.to_ascii_lowercase();
    if lower.contains("busy") || lower.contains("locked") {
        format!("{message}; retry after stopping capture or after the database is no longer busy")
    } else {
        message
    }
}

fn encode_candidate_as_lossless_webp(
    candidate: &ImageOptimizationCandidate,
) -> std::result::Result<OptimizedImage, &'static str> {
    if candidate_looks_animated_or_unsupported(candidate) {
        return Err("animated_or_unsupported");
    }

    let mut reader = ImageReader::new(Cursor::new(&candidate.blob_value))
        .with_guessed_format()
        .map_err(|_| "unsupported")?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(IMAGE_OPTIMIZATION_MAX_DIMENSION);
    limits.max_image_height = Some(IMAGE_OPTIMIZATION_MAX_DIMENSION);
    reader.limits(limits);
    let image = reader.decode().map_err(|_| "corrupt_or_unsupported")?;
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut bytes = Vec::new();
    image::codecs::webp::WebPEncoder::new_lossless(&mut bytes)
        .write_image(rgba.as_raw(), width, height, ExtendedColorType::Rgba8)
        .map_err(|_| "encode_failed")?;
    let raw_sha256 = hash_bytes(&bytes);
    Ok(OptimizedImage { bytes, raw_sha256 })
}

fn candidate_looks_animated_or_unsupported(candidate: &ImageOptimizationCandidate) -> bool {
    let uti = candidate.uti.to_ascii_lowercase();
    if uti.contains("gif") || uti.contains("webp") {
        return true;
    }

    let bytes = candidate.blob_value.as_slice();
    bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") || webp_has_animation_flag(bytes)
}

fn webp_has_animation_flag(bytes: &[u8]) -> bool {
    bytes.len() >= 21
        && &bytes[0..4] == b"RIFF"
        && &bytes[8..12] == b"WEBP"
        && &bytes[12..16] == b"VP8X"
        && (bytes[20] & 0b0000_0010) != 0
}

fn image_optimization_is_beneficial(original_len: usize, optimized_len: usize) -> bool {
    if optimized_len >= original_len {
        return false;
    }
    let saved = original_len - optimized_len;
    saved >= IMAGE_OPTIMIZATION_MIN_ABSOLUTE_SAVINGS
        && saved * IMAGE_OPTIMIZATION_MIN_RELATIVE_SAVINGS_DENOMINATOR
            >= original_len * IMAGE_OPTIMIZATION_MIN_RELATIVE_SAVINGS_NUMERATOR
}

fn mark_image_optimization_skipped(
    conn: &rusqlite::Connection,
    candidate: &ImageOptimizationCandidate,
    reason: &str,
) -> Result<()> {
    conn.execute(
        r"
            UPDATE item_representations
            SET image_compression_status = 'skipped',
                image_compression_reason = ?4
            WHERE snapshot_id = ?1
              AND item_index = ?2
              AND uti = ?3
              AND image_compression_status = 'uncompressed'
        ",
        params![
            candidate.snapshot_id,
            candidate.item_index,
            &candidate.uti,
            reason
        ],
    )
    .context("mark image optimization skipped")?;
    Ok(())
}

fn image_optimization_would_conflict(
    conn: &rusqlite::Connection,
    candidate: &ImageOptimizationCandidate,
    optimized: &OptimizedImage,
) -> Result<bool> {
    let uti_conflict: i64 = conn
        .query_row(
            r"
                SELECT EXISTS(
                    SELECT 1
                    FROM item_representations
                    WHERE snapshot_id = ?1
                      AND item_index = ?2
                      AND uti = ?3
                      AND uti != ?4
                )
            ",
            params![
                candidate.snapshot_id,
                candidate.item_index,
                WEBP_UTI,
                &candidate.uti
            ],
            |row| row.get(0),
        )
        .context("check WebP representation conflict")?;
    if uti_conflict != 0 {
        return Ok(true);
    }

    let fingerprint =
        snapshot_fingerprint_with_replacement(conn, candidate, WEBP_UTI, &optimized.bytes)?;
    let snapshot_conflict: i64 = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM snapshots WHERE sha256 = ?1 AND id != ?2)",
            params![fingerprint, candidate.snapshot_id],
            |row| row.get(0),
        )
        .context("check optimized snapshot fingerprint conflict")?;
    Ok(snapshot_conflict != 0)
}

fn replace_image_with_optimized_webp(
    conn: &mut rusqlite::Connection,
    candidate: &ImageOptimizationCandidate,
    optimized: OptimizedImage,
) -> Result<()> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .context("begin image optimization transaction")?;

    let updated = tx
        .execute(
            r"
            UPDATE item_representations
            SET uti = ?4,
                byte_len = ?5,
                raw_sha256 = ?6,
                blob_value = ?7,
                image_compression_status = 'compressed',
                image_compression_format = ?8,
                image_compressed_at = CURRENT_TIMESTAMP,
                image_original_byte_len = ?9,
                image_original_raw_sha256 = ?10,
                image_compression_reason = NULL
            WHERE snapshot_id = ?1
              AND item_index = ?2
              AND uti = ?3
              AND image_compression_status = 'uncompressed'
        ",
            params![
                candidate.snapshot_id,
                candidate.item_index,
                &candidate.uti,
                WEBP_UTI,
                usize_to_i64(optimized.bytes.len())?,
                &optimized.raw_sha256,
                &optimized.bytes,
                IMAGE_OPTIMIZATION_FORMAT,
                usize_to_i64(candidate.byte_len)?,
                &candidate.raw_sha256
            ],
        )
        .context("replace image representation with lossless WebP")?;
    ensure!(
        updated == 1,
        "image optimization target changed before replacement; retry optimize-images"
    );

    copy_ocr_result_for_optimized_image(&tx, &candidate.raw_sha256, &optimized.raw_sha256)?;
    rebuild_snapshot_item_summary(&tx, candidate.snapshot_id, candidate.item_index)?;
    rebuild_snapshot_summary(&tx, candidate.snapshot_id)?;
    let fingerprint = recompute_snapshot_fingerprint(&tx, candidate.snapshot_id)?;
    tx.execute(
        "UPDATE snapshots SET sha256 = ?2 WHERE id = ?1",
        params![candidate.snapshot_id, fingerprint],
    )
    .context("update optimized snapshot fingerprint")?;
    rebuild_snapshot_ocr_cache(&tx, candidate.snapshot_id)?;

    tx.commit()
        .context("commit image optimization transaction")?;
    Ok(())
}

fn copy_ocr_result_for_optimized_image(
    tx: &rusqlite::Transaction<'_>,
    old_hash: &str,
    new_hash: &str,
) -> Result<()> {
    tx.execute(
        r"
            INSERT OR IGNORE INTO ocr_results (
                raw_sha256,
                status,
                engine,
                recognition_level,
                text_value,
                error,
                attempt_count,
                created_at,
                updated_at
            )
            SELECT
                ?2,
                status,
                engine,
                recognition_level,
                text_value,
                error,
                attempt_count,
                created_at,
                CURRENT_TIMESTAMP
            FROM ocr_results
            WHERE raw_sha256 = ?1
        ",
        params![old_hash, new_hash],
    )
    .context("copy OCR result for optimized image")?;
    Ok(())
}

fn rebuild_snapshot_item_summary(
    tx: &rusqlite::Transaction<'_>,
    snapshot_id: i64,
    item_index: i64,
) -> Result<()> {
    let total_bytes: usize = tx
        .query_row(
            r"
                SELECT COALESCE(SUM(byte_len), 0)
                FROM item_representations
                WHERE snapshot_id = ?1 AND item_index = ?2
            ",
            params![snapshot_id, item_index],
            |row| row_usize(row, 0),
        )
        .context("recompute snapshot item byte count")?;
    let primary = tx
        .query_row(
            r"
                SELECT kind, uti, byte_len
                FROM item_representations
                WHERE snapshot_id = ?1 AND item_index = ?2
                ORDER BY
                    CASE kind
                        WHEN 'plain_text' THEN 0
                        WHEN 'url' THEN 1
                        WHEN 'file_url' THEN 2
                        WHEN 'html' THEN 3
                        WHEN 'json' THEN 4
                        WHEN 'xml' THEN 5
                        WHEN 'rtf' THEN 6
                        WHEN 'pdf' THEN 7
                        WHEN 'image' THEN 8
                        WHEN 'binary' THEN 9
                        ELSE 10
                    END,
                    CASE WHEN text_value IS NULL OR text_value = '' THEN 1 ELSE 0 END,
                    uti ASC
                LIMIT 1
            ",
            params![snapshot_id, item_index],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row_usize(row, 2)?,
                ))
            },
        )
        .optional()
        .context("load optimized item primary representation")?;
    let search_text: String = tx
        .query_row(
            "SELECT search_text FROM snapshot_items WHERE snapshot_id = ?1 AND item_index = ?2",
            params![snapshot_id, item_index],
            |row| row.get(0),
        )
        .context("load optimized item search text")?;
    let (primary_kind, primary_uti, preview_text) =
        if let Some((kind, uti, primary_byte_len)) = primary {
            let preview_text = if search_text.is_empty() {
                truncate_chars(&format!("[{kind} · {primary_byte_len} bytes · {uti}]"), 200)
            } else {
                search_text.clone()
            };
            (kind, Some(uti), preview_text)
        } else {
            (
                "empty".to_string(),
                None,
                "[empty clipboard item]".to_string(),
            )
        };

    tx.execute(
        r"
            UPDATE snapshot_items
            SET primary_kind = ?3,
                primary_uti = ?4,
                preview_text = ?5,
                total_bytes = ?6
            WHERE snapshot_id = ?1 AND item_index = ?2
        ",
        params![
            snapshot_id,
            item_index,
            primary_kind,
            primary_uti,
            preview_text,
            usize_to_i64(total_bytes)?
        ],
    )
    .context("update optimized snapshot item summary")?;
    Ok(())
}

fn rebuild_snapshot_summary(tx: &rusqlite::Transaction<'_>, snapshot_id: i64) -> Result<()> {
    let mut stmt = tx
        .prepare(
            r"
                SELECT primary_kind, preview_text, search_text, total_bytes
                FROM snapshot_items
                WHERE snapshot_id = ?1
                ORDER BY item_index ASC
            ",
        )
        .context("prepare optimized snapshot summary query")?;
    let rows = stmt
        .query_map([snapshot_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row_usize(row, 3)?,
            ))
        })
        .context("execute optimized snapshot summary query")?;
    let items = super::collect_rows(rows).context("collect optimized snapshot summary rows")?;
    drop(stmt);

    let item_count = items.len();
    let total_bytes = items.iter().map(|(_, _, _, bytes)| *bytes).sum::<usize>();
    let preview_parts = items
        .iter()
        .map(|(_, preview, _, _)| preview.trim())
        .filter(|preview| !preview.is_empty())
        .collect::<Vec<_>>();
    let preview_text = if preview_parts.is_empty() {
        "[empty clipboard]".to_string()
    } else {
        truncate_chars(&preview_parts.join(" | "), 280)
    };
    let search_text = items
        .iter()
        .map(|(_, _, search, _)| search.trim())
        .filter(|search| !search.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    let snapshot_kind = snapshot_kind_from_item_rows(&items);

    tx.execute(
        r"
            UPDATE snapshots
            SET snapshot_kind = ?2,
                preview_text = ?3,
                search_text = ?4,
                item_count = ?5,
                total_bytes = ?6
            WHERE id = ?1
        ",
        params![
            snapshot_id,
            snapshot_kind,
            preview_text,
            search_text,
            usize_to_i64(item_count)?,
            usize_to_i64(total_bytes)?
        ],
    )
    .context("update optimized snapshot summary")?;
    Ok(())
}

fn snapshot_kind_from_item_rows(items: &[(String, String, String, usize)]) -> String {
    if items.is_empty() {
        return "empty".to_string();
    }

    let first = &items[0].0;
    if items.iter().all(|(kind, _, _, _)| kind == first) {
        first.clone()
    } else {
        "mixed".to_string()
    }
}

fn snapshot_fingerprint_with_replacement(
    conn: &rusqlite::Connection,
    candidate: &ImageOptimizationCandidate,
    replacement_uti: &str,
    replacement_bytes: &[u8],
) -> Result<String> {
    recompute_snapshot_fingerprint_with(conn, candidate.snapshot_id, |item_index, uti, bytes| {
        if item_index == candidate.item_index && uti == candidate.uti {
            Some((replacement_uti.to_string(), replacement_bytes.to_vec()))
        } else {
            Some((uti.to_string(), bytes.to_vec()))
        }
    })
}

fn recompute_snapshot_fingerprint(conn: &rusqlite::Connection, snapshot_id: i64) -> Result<String> {
    recompute_snapshot_fingerprint_with(conn, snapshot_id, |_, uti, bytes| {
        Some((uti.to_string(), bytes.to_vec()))
    })
}

fn recompute_snapshot_fingerprint_with<F>(
    conn: &rusqlite::Connection,
    snapshot_id: i64,
    mut representation: F,
) -> Result<String>
where
    F: FnMut(i64, &str, &[u8]) -> Option<(String, Vec<u8>)>,
{
    use sha2::{Digest, Sha256};

    let mut item_stmt = conn
        .prepare(
            "SELECT item_index FROM snapshot_items WHERE snapshot_id = ?1 ORDER BY item_index ASC",
        )
        .context("prepare snapshot fingerprint item query")?;
    let item_rows = item_stmt
        .query_map([snapshot_id], |row| row.get::<_, i64>(0))
        .context("execute snapshot fingerprint item query")?;
    let item_indices =
        super::collect_rows(item_rows).context("collect snapshot fingerprint items")?;
    drop(item_stmt);

    let mut hasher = Sha256::new();
    hasher.update(b"clipmem/v1");
    for item_index in item_indices {
        hasher.update((item_index as u64).to_be_bytes());
        let mut rep_stmt = conn
            .prepare(
                r"
                    SELECT uti, blob_value
                    FROM item_representations
                    WHERE snapshot_id = ?1 AND item_index = ?2
                    ORDER BY uti ASC
                ",
            )
            .context("prepare snapshot fingerprint representation query")?;
        let rep_rows = rep_stmt
            .query_map(params![snapshot_id, item_index], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .context("execute snapshot fingerprint representation query")?;
        let mut reps = super::collect_rows(rep_rows)
            .context("collect snapshot fingerprint representations")?;
        drop(rep_stmt);
        reps = reps
            .into_iter()
            .filter_map(|(uti, bytes)| representation(item_index, &uti, &bytes))
            .collect();
        reps.sort_by(|(left_uti, _), (right_uti, _)| left_uti.cmp(right_uti));

        for (uti, bytes) in reps {
            hasher.update((uti.len() as u64).to_be_bytes());
            hasher.update(uti.as_bytes());
            hasher.update((bytes.len() as u64).to_be_bytes());
            hasher.update(bytes);
        }
    }

    Ok(hex::encode(hasher.finalize()))
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
                blob_value,
                image_compression_status
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'uncompressed')",
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
