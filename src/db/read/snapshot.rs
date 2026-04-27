use anyhow::{Context, Result};
use rusqlite::{params, OptionalExtension};

use crate::db::core::{collect_rows, row_enum, row_usize, sanitise_limit, usize_to_i64};
use crate::db::types::Database;
use crate::model::{
    CaptureEvent, ClipboardItem, ClipboardRepresentation, DoctorReport, FlattenedTextProjection,
    SnapshotDetails,
};

impl Database {
    /// Load one snapshot with its recent events and stored item representations.
    ///
    /// # Errors
    ///
    /// Returns an error if any lookup query fails.
    pub fn find_snapshot(
        &self,
        snapshot_id: i64,
        event_limit: usize,
    ) -> Result<Option<SnapshotDetails>> {
        let Some(mut details) = self.load_snapshot_summary(snapshot_id)? else {
            return Ok(None);
        };

        details
            .set_recent_events(self.load_recent_events(snapshot_id, sanitise_limit(event_limit))?);
        details.set_items(self.load_snapshot_items(snapshot_id)?);

        Ok(Some(details))
    }

    pub(crate) fn snapshot_projection(
        &self,
        snapshot_id: i64,
    ) -> Result<Option<FlattenedTextProjection>> {
        if !self.snapshot_exists(snapshot_id)? {
            return Ok(None);
        }

        let items = self.load_snapshot_items(snapshot_id)?;
        let ocr = self
            .conn
            .query_row(
                "SELECT ocr_text, status FROM snapshot_ocr_cache WHERE snapshot_id = ?1",
                [snapshot_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .optional()
            .context("load snapshot ocr projection")?
            .unwrap_or((None, None));
        Ok(Some(
            FlattenedTextProjection::from_items(&items).with_ocr(ocr.0, ocr.1),
        ))
    }

    fn snapshot_exists(&self, snapshot_id: i64) -> Result<bool> {
        let exists = self
            .conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM snapshots WHERE id = ?1)",
                [snapshot_id],
                |row| row.get::<_, i64>(0),
            )
            .context("check snapshot exists for projection")?;
        Ok(exists != 0)
    }

    /// Collect `SQLite` and FTS5 diagnostics for the current archive database.
    ///
    /// # Errors
    ///
    /// Returns an error if any diagnostic query fails.
    pub fn doctor(&self) -> Result<DoctorReport> {
        let sqlite_version: String = self
            .conn
            .query_row("SELECT sqlite_version()", [], |row| row.get(0))
            .context("read SQLite version")?;

        let journal_mode: String = self
            .conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .context("read journal mode")?;

        let mut compile_stmt = self
            .conn
            .prepare("PRAGMA compile_options")
            .context("prepare compile options query")?;
        let compile_rows = compile_stmt
            .query_map([], |row| row.get::<_, String>(0))
            .context("execute compile options query")?;
        let compile_options = collect_rows(compile_rows).context("collect compile options")?;

        let fts5_compile_option_present = compile_options
            .iter()
            .any(|opt| opt == "ENABLE_FTS5" || opt == "SQLITE_ENABLE_FTS5");

        self.conn
            .execute_batch(
                "CREATE VIRTUAL TABLE temp.__clipmem_fts_check USING fts5(x);
                 DROP TABLE temp.__clipmem_fts_check;",
            )
            .context("probe FTS5 temp virtual table creation")?;
        let fts5_create_virtual_table_ok = true;

        Ok(DoctorReport::new(
            self.path.display().to_string(),
            sqlite_version,
            journal_mode,
            fts5_compile_option_present,
            fts5_create_virtual_table_ok,
            compile_options,
        ))
    }

    /// Load one raw representation payload by snapshot id, item index, and UTI.
    ///
    /// # Errors
    ///
    /// Returns an error if the lookup query fails.
    pub fn find_representation_bytes(
        &self,
        snapshot_id: i64,
        item_index: usize,
        uti: &str,
    ) -> Result<Option<ClipboardRepresentation>> {
        let item_index = usize_to_i64(item_index)?;

        rusqlite::OptionalExtension::optional(self.conn.query_row(
            r"
                SELECT uti, kind, raw_sha256, text_value, blob_value
                FROM item_representations
                WHERE snapshot_id = ?1 AND item_index = ?2 AND uti = ?3
            ",
            params![snapshot_id, item_index, uti],
            |row| {
                Ok(ClipboardRepresentation::new(
                    row.get(0)?,
                    row_enum(row, 1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        ))
        .context("load representation bytes")
    }

    fn load_snapshot_summary(&self, snapshot_id: i64) -> Result<Option<SnapshotDetails>> {
        let summary_sql = r"
            SELECT
                s.id,
                s.sha256,
                s.snapshot_kind,
                s.preview_text,
                s.search_text,
                s.item_count,
                s.total_bytes,
                s.created_at,
                CASE
                    WHEN ss.snapshot_id IS NULL THEN (
                        SELECT COUNT(*)
                        FROM capture_events ce
                        WHERE ce.snapshot_id = s.id
                    )
                    ELSE ss.capture_count
                END AS capture_count,
                CASE
                    WHEN ss.snapshot_id IS NULL THEN (
                        SELECT MIN(ce.observed_at)
                        FROM capture_events ce
                        WHERE ce.snapshot_id = s.id
                    )
                    ELSE ss.first_observed_at
                END AS first_observed_at,
                CASE
                    WHEN ss.snapshot_id IS NULL THEN (
                        SELECT MAX(ce.observed_at)
                        FROM capture_events ce
                        WHERE ce.snapshot_id = s.id
                    )
                    ELSE ss.last_observed_at
                END AS last_observed_at,
                CASE
                    WHEN ss.snapshot_id IS NULL THEN (
                        SELECT ce.frontmost_app_name
                        FROM capture_events ce
                        WHERE ce.snapshot_id = s.id
                        ORDER BY ce.observed_at DESC, ce.id DESC
                        LIMIT 1
                    )
                    ELSE ss.last_frontmost_app_name
                END AS last_frontmost_app_name,
                CASE
                    WHEN ss.snapshot_id IS NULL THEN (
                        SELECT ce.frontmost_app_bundle_id
                        FROM capture_events ce
                        WHERE ce.snapshot_id = s.id
                        ORDER BY ce.observed_at DESC, ce.id DESC
                        LIMIT 1
                    )
                    ELSE ss.last_frontmost_app_bundle_id
                END AS last_frontmost_app_bundle_id,
                soc.ocr_text,
                soc.status AS ocr_status
            FROM snapshots s
            LEFT JOIN snapshot_stats ss ON ss.snapshot_id = s.id
            LEFT JOIN snapshot_ocr_cache soc ON soc.snapshot_id = s.id
            WHERE s.id = ?1
        ";

        rusqlite::OptionalExtension::optional(self.conn.query_row(
            summary_sql,
            [snapshot_id],
            |row| {
                Ok(SnapshotDetails::new(
                    row.get(0)?,
                    row.get(1)?,
                    row_enum(row, 2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row_usize(row, 5)?,
                    row_usize(row, 6)?,
                    row.get(7)?,
                    row_usize(row, 8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                    row.get(12)?,
                    row.get(13)?,
                    row.get(14)?,
                    Vec::new(),
                    Vec::new(),
                ))
            },
        ))
        .context("load snapshot summary")
    }

    fn load_recent_events(
        &self,
        snapshot_id: i64,
        event_limit: usize,
    ) -> Result<Vec<CaptureEvent>> {
        let limit = usize_to_i64(event_limit)?;
        let mut event_stmt = self
            .conn
            .prepare(
                r"
                SELECT id, observed_at, change_count, frontmost_app_name, frontmost_app_bundle_id
                FROM capture_events
                WHERE snapshot_id = ?1
                ORDER BY observed_at DESC, id DESC
                LIMIT ?2
            ",
            )
            .context("prepare snapshot events query")?;
        let event_rows = event_stmt
            .query_map(params![snapshot_id, limit], |row| {
                Ok(CaptureEvent::new(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .context("execute snapshot events query")?;

        collect_rows(event_rows).context("collect snapshot events")
    }

    fn load_snapshot_items(&self, snapshot_id: i64) -> Result<Vec<ClipboardItem>> {
        let mut item_stmt = self
            .conn
            .prepare(
                r"
                SELECT item_index, primary_kind, primary_uti, preview_text, search_text, total_bytes
                FROM snapshot_items
                WHERE snapshot_id = ?1
                ORDER BY item_index ASC
            ",
            )
            .context("prepare snapshot items query")?;

        let item_rows = item_stmt
            .query_map([snapshot_id], |row| {
                Ok(ClipboardItem::new(
                    row_usize(row, 0)?,
                    row_enum(row, 1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    Vec::new(),
                ))
            })
            .context("execute snapshot items query")?;
        let mut items = collect_rows(item_rows).context("collect snapshot items")?;

        let mut rep_stmt = self
            .conn
            .prepare(
                r"
                SELECT
                    item_index,
                    uti,
                    kind,
                    raw_sha256,
                    text_value,
                    blob_value
                FROM item_representations
                WHERE snapshot_id = ?1
                ORDER BY item_index ASC, uti ASC
            ",
            )
            .context("prepare representation query")?;

        let rep_rows = rep_stmt
            .query_map([snapshot_id], |row| {
                Ok((
                    row_usize(row, 0)?,
                    ClipboardRepresentation::new(
                        row.get(1)?,
                        row_enum(row, 2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ),
                ))
            })
            .context("execute snapshot representations query")?;
        let mut representations_by_item = (0..items.len()).map(|_| Vec::new()).collect::<Vec<_>>();
        let mut item_position = 0;
        for (item_index, representation) in
            collect_rows(rep_rows).context("collect snapshot representations")?
        {
            while item_position < items.len() && items[item_position].item_index() < item_index {
                item_position += 1;
            }
            if item_position < items.len() && items[item_position].item_index() == item_index {
                representations_by_item[item_position].push(representation);
            }
        }

        for (item, representations) in items.iter_mut().zip(representations_by_item) {
            item.set_representations(representations);
        }

        Ok(items)
    }
}

#[cfg(test)]
mod profile_tests {
    use std::time::{Duration, Instant};

    use anyhow::{Context, Result};
    use rusqlite::params;

    use crate::db::core::{collect_rows, row_enum, row_usize, usize_to_i64};
    use crate::db::types::Database;
    use crate::model::{
        build_item, build_representation, build_snapshot, CaptureContext, ClipboardItem,
        ClipboardRepresentation, ClipboardSnapshot,
    };

    #[test]
    #[ignore = "profiling harness for snapshot detail item hydration"]
    fn profile_snapshot_item_hydration() -> Result<()> {
        let mut db = Database::open_in_memory()?;
        let stored = db.store_capture(&many_item_snapshot(1_000, 2))?;

        let before = median_duration(7, 2_000, || {
            let items = load_snapshot_items_n_plus_one_for_profile(&db, stored.snapshot_id())?;
            Ok(representation_count(&items))
        })?;
        let after = median_duration(7, 2_000, || {
            let items = db.load_snapshot_items(stored.snapshot_id())?;
            Ok(representation_count(&items))
        })?;

        eprintln!(
            "snapshot_items_n_plus_one_before={before:?} snapshot_items_grouped_after={after:?}"
        );
        Ok(())
    }

    fn median_duration(
        runs: usize,
        expected_count: usize,
        mut f: impl FnMut() -> Result<usize>,
    ) -> Result<Duration> {
        let mut samples = Vec::with_capacity(runs);
        for _ in 0..runs {
            let started = Instant::now();
            let count = f()?;
            assert_eq!(count, expected_count);
            samples.push(started.elapsed());
        }
        samples.sort();
        Ok(samples[samples.len() / 2])
    }

    fn many_item_snapshot(item_count: usize, representations_per_item: usize) -> ClipboardSnapshot {
        let items = (0..item_count)
            .map(|item_index| {
                let representations = (0..representations_per_item)
                    .map(|representation_index| {
                        let text = format!(
                            "snapshot item {item_index} representation {representation_index}"
                        );
                        build_representation(
                            format!("public.utf8-plain-text.{representation_index}"),
                            Some(text.clone()),
                            text.into_bytes(),
                        )
                    })
                    .collect();
                build_item(item_index, representations)
            })
            .collect();

        build_snapshot(
            CaptureContext::new(1)
                .with_frontmost_app_name("Hydration Bench")
                .with_frontmost_app_bundle_id("com.example.HydrationBench"),
            items,
        )
    }

    fn representation_count(items: &[ClipboardItem]) -> usize {
        items.iter().map(|item| item.representations().len()).sum()
    }

    fn load_snapshot_items_n_plus_one_for_profile(
        db: &Database,
        snapshot_id: i64,
    ) -> Result<Vec<ClipboardItem>> {
        let mut item_stmt = db
            .conn
            .prepare(
                r"
                SELECT item_index, primary_kind, primary_uti, preview_text, search_text, total_bytes
                FROM snapshot_items
                WHERE snapshot_id = ?1
                ORDER BY item_index ASC
            ",
            )
            .context("prepare snapshot items profile query")?;

        let item_rows = item_stmt
            .query_map([snapshot_id], |row| {
                Ok(ClipboardItem::new(
                    row_usize(row, 0)?,
                    row_enum(row, 1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    Vec::new(),
                ))
            })
            .context("execute snapshot items profile query")?;
        let mut items = collect_rows(item_rows).context("collect snapshot profile items")?;

        let mut rep_stmt = db
            .conn
            .prepare(
                r"
                SELECT
                    uti,
                    kind,
                    raw_sha256,
                    text_value,
                    blob_value
                FROM item_representations
                WHERE snapshot_id = ?1 AND item_index = ?2
                ORDER BY uti ASC
            ",
            )
            .context("prepare representation profile query")?;

        for item in &mut items {
            let rep_rows = rep_stmt
                .query_map(
                    params![snapshot_id, usize_to_i64(item.item_index())?],
                    |row| {
                        Ok(ClipboardRepresentation::new(
                            row.get(0)?,
                            row_enum(row, 1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    },
                )
                .with_context(|| {
                    format!(
                        "execute representation profile query for item {}",
                        item.item_index()
                    )
                })?;
            item.set_representations(collect_rows(rep_rows).with_context(|| {
                format!(
                    "collect profile representations for item {}",
                    item.item_index()
                )
            })?);
        }

        Ok(items)
    }
}
