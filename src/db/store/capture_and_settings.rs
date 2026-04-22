use super::*;

pub(in crate::db) const WEBP_UTI: &str = "org.webmproject.webp";
pub(in crate::db) const IMAGE_OPTIMIZATION_FORMAT: &str = "webp_lossless";
pub(in crate::db) const IMAGE_OPTIMIZATION_MIN_ABSOLUTE_SAVINGS: usize = 64 * 1024;
pub(in crate::db) const IMAGE_OPTIMIZATION_MIN_RELATIVE_SAVINGS_NUMERATOR: usize = 1;
pub(in crate::db) const IMAGE_OPTIMIZATION_MIN_RELATIVE_SAVINGS_DENOMINATOR: usize = 10;
pub(in crate::db) const IMAGE_OPTIMIZATION_MAX_DIMENSION: u32 = 16_384;
pub(in crate::db) const RESTORE_SUPPRESSION_WINDOW_SECONDS: i64 = 30;

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

        let inserted_event_rows = tx
            .execute(
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
        if inserted_event_rows == 0 {
            tx.commit()
                .context("commit suppressed capture transaction")?;
            anyhow::bail!("capture event suppressed by pending restore marker");
        }
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

    pub(crate) fn store_watched_capture_if_allowed(
        &mut self,
        snapshot: &ClipboardSnapshot,
    ) -> Result<CaptureStoreOutcome> {
        if self.consume_pending_restore(snapshot.fingerprint())? {
            return Ok(CaptureStoreOutcome::Skipped(
                CaptureSkipReason::RestoredSnapshot,
            ));
        }

        self.store_capture_if_allowed(snapshot)
    }

    pub(crate) fn register_pending_restore(&self, snapshot_sha256: &str) -> Result<()> {
        self.delete_expired_pending_restores()?;
        self.conn
            .execute(
                "INSERT INTO pending_restores (snapshot_sha256, created_at)
                 VALUES (?1, CURRENT_TIMESTAMP)
                 ON CONFLICT(snapshot_sha256) DO UPDATE SET created_at = CURRENT_TIMESTAMP",
                [snapshot_sha256],
            )
            .context("register pending restore")?;
        Ok(())
    }

    pub(crate) fn clear_pending_restore(&self, snapshot_sha256: &str) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM pending_restores WHERE snapshot_sha256 = ?1",
                [snapshot_sha256],
            )
            .context("clear pending restore")?;
        Ok(())
    }

    fn consume_pending_restore(&mut self, snapshot_sha256: &str) -> Result<bool> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("begin pending restore transaction")?;
        delete_expired_pending_restores(&tx)?;
        let consumed = tx
            .execute(
                "DELETE FROM pending_restores WHERE snapshot_sha256 = ?1",
                [snapshot_sha256],
            )
            .context("consume pending restore")?
            != 0;
        tx.commit().context("commit pending restore transaction")?;
        Ok(consumed)
    }

    fn delete_expired_pending_restores(&self) -> Result<()> {
        delete_expired_pending_restores(&self.conn)
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
                        WHERE last_observed_at < datetime('now', printf('-%d seconds', ?1))
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
                        (SELECT COUNT(*) FROM ocr_results WHERE status = 'pending'),
                        (SELECT COUNT(*) FROM ocr_results WHERE status = 'ready'),
                        (SELECT COUNT(*) FROM ocr_results WHERE status = 'failed'),
                        (SELECT COUNT(*) FROM ocr_results WHERE status = 'skipped'),
                        (
                            SELECT COUNT(*)
                            FROM snapshot_ocr_cache
                            WHERE ocr_text != ''
                        )
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
