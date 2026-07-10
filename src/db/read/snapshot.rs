use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use rusqlite::{params, OptionalExtension};

use crate::db::core::clamp_result_limit;
use crate::db::sqlite_helpers::{collect_rows, row_enum, row_usize, usize_to_i64};
use crate::db::types::{
    Database, RepresentationManifest, RepresentationPayload, RestorePayloadPlan,
    SnapshotItemManifest, SnapshotMetadata,
};
use crate::model::{
    CaptureEvent, ClipboardItem, ClipboardRepresentation, DoctorReport, FlattenedTextProjection,
    SnapshotDetails,
};

impl Database {
    /// Load snapshot metadata, event summaries, and representation manifests without payload BLOBs.
    pub fn find_snapshot_metadata(
        &self,
        snapshot_id: i64,
        event_limit: usize,
    ) -> Result<Option<SnapshotMetadata>> {
        let Some(summary) = self.load_snapshot_summary(snapshot_id)? else {
            return Ok(None);
        };
        let events = self.load_recent_events(snapshot_id, clamp_result_limit(event_limit))?;
        let items = self.load_snapshot_manifests(snapshot_id)?;
        let projection = projection_from_manifests(&items).with_ocr(
            summary.ocr_text().map(ToOwned::to_owned),
            summary.ocr_status().map(ToOwned::to_owned),
        );
        Ok(Some(SnapshotMetadata::from_summary(
            &summary,
            &projection,
            events,
            items,
        )))
    }

    /// Load canonical/legacy snapshot documents in set-based batches without payload BLOBs.
    pub fn find_snapshot_documents(
        &self,
        snapshot_ids: &[i64],
    ) -> Result<HashMap<i64, FlattenedTextProjection>> {
        const IDS_PER_QUERY: usize = 400;
        let mut seen = HashSet::new();
        let unique_ids = snapshot_ids
            .iter()
            .copied()
            .filter(|id| seen.insert(*id))
            .collect::<Vec<_>>();
        let mut items_by_snapshot: HashMap<i64, Vec<SnapshotItemManifest>> = HashMap::new();
        let mut ocr_by_snapshot: HashMap<i64, (Option<String>, Option<String>)> = HashMap::new();

        for ids in unique_ids.chunks(IDS_PER_QUERY) {
            let placeholders = std::iter::repeat_n("?", ids.len())
                .collect::<Vec<_>>()
                .join(",");
            let item_sql = format!(
                "SELECT si.snapshot_id, si.item_index, si.primary_kind, si.primary_uti, si.preview_text, si.search_text, si.total_bytes, soc.ocr_text, soc.status
                 FROM snapshot_items si
                 LEFT JOIN snapshot_ocr_cache soc ON soc.snapshot_id = si.snapshot_id
                 WHERE si.snapshot_id IN ({placeholders})
                 ORDER BY si.snapshot_id, si.item_index"
            );
            let mut item_stmt = self
                .conn
                .prepare(&item_sql)
                .context("prepare bulk snapshot document items")?;
            let rows = item_stmt
                .query_map(rusqlite::params_from_iter(ids.iter()), |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        SnapshotItemManifest {
                            item_index: row_usize(row, 1)?,
                            primary_kind: row_enum(row, 2)?,
                            primary_uti: row.get(3)?,
                            preview_text: row.get(4)?,
                            search_text: row.get(5)?,
                            total_bytes: row_usize(row, 6)?,
                            representations: Vec::new(),
                        },
                        (row.get(7)?, row.get(8)?),
                    ))
                })
                .context("execute bulk snapshot document items")?;
            for (snapshot_id, item, ocr) in collect_rows(rows)? {
                items_by_snapshot.entry(snapshot_id).or_default().push(item);
                ocr_by_snapshot.entry(snapshot_id).or_insert(ocr);
            }

            let rep_sql = format!(
                "SELECT snapshot_id, item_index, uti, kind, byte_len, raw_sha256, text_value
                 FROM item_representations
                 WHERE snapshot_id IN ({placeholders})
                 ORDER BY snapshot_id, item_index, uti"
            );
            let mut rep_stmt = self
                .conn
                .prepare(&rep_sql)
                .context("prepare bulk snapshot document manifests")?;
            let rows = rep_stmt
                .query_map(rusqlite::params_from_iter(ids.iter()), |row| {
                    let text_value = row.get::<_, Option<String>>(6)?;
                    Ok((
                        row.get::<_, i64>(0)?,
                        row_usize(row, 1)?,
                        RepresentationManifest {
                            uti: row.get(2)?,
                            kind: row_enum(row, 3)?,
                            is_text: text_value.is_some(),
                            byte_len: row_usize(row, 4)?,
                            raw_sha256: row.get(5)?,
                            text_value,
                        },
                    ))
                })
                .context("execute bulk snapshot document manifests")?;
            for (snapshot_id, item_index, manifest) in collect_rows(rows)? {
                if let Some(item) = items_by_snapshot
                    .get_mut(&snapshot_id)
                    .and_then(|items| items.iter_mut().find(|item| item.item_index == item_index))
                {
                    item.representations.push(manifest);
                }
            }
        }

        Ok(items_by_snapshot
            .into_iter()
            .map(|(snapshot_id, items)| {
                let (ocr_text, ocr_status) =
                    ocr_by_snapshot.remove(&snapshot_id).unwrap_or((None, None));
                (
                    snapshot_id,
                    projection_from_manifests(&items).with_ocr(ocr_text, ocr_status),
                )
            })
            .collect())
    }

    pub fn find_representation_manifest(
        &self,
        snapshot_id: i64,
        item_index: usize,
        uti: &str,
    ) -> Result<Option<RepresentationManifest>> {
        let item_index = usize_to_i64(item_index)?;
        self.conn
            .query_row(
                "SELECT uti, kind, byte_len, raw_sha256, text_value FROM item_representations WHERE snapshot_id = ?1 AND item_index = ?2 AND uti = ?3",
                params![snapshot_id, item_index, uti],
                |row| {
                    let text_value = row.get::<_, Option<String>>(4)?;
                    Ok(RepresentationManifest {
                        uti: row.get(0)?,
                        kind: row_enum(row, 1)?,
                        is_text: text_value.is_some(),
                        byte_len: row_usize(row, 2)?,
                        raw_sha256: row.get(3)?,
                        text_value,
                    })
                },
            )
            .optional()
            .context("load representation manifest")
    }

    /// Read exactly one representation payload and its manifest.
    pub fn read_representation_payload(
        &self,
        snapshot_id: i64,
        item_index: usize,
        uti: &str,
    ) -> Result<Option<RepresentationPayload>> {
        let item_index = usize_to_i64(item_index)?;
        self.conn
            .query_row(
                "SELECT uti, kind, byte_len, raw_sha256, text_value, blob_value FROM item_representations WHERE snapshot_id = ?1 AND item_index = ?2 AND uti = ?3",
                params![snapshot_id, item_index, uti],
                |row| {
                    let text_value = row.get::<_, Option<String>>(4)?;
                    Ok(RepresentationPayload {
                        manifest: RepresentationManifest {
                            uti: row.get(0)?,
                            kind: row_enum(row, 1)?,
                            is_text: text_value.is_some(),
                            byte_len: row_usize(row, 2)?,
                            raw_sha256: row.get(3)?,
                            text_value,
                        },
                        bytes: row.get(5)?,
                    })
                },
            )
            .optional()
            .context("load representation payload")
    }

    /// Intentionally hydrate every source representation required for restore exactly once.
    pub fn load_restore_payload(&self, snapshot_id: i64) -> Result<Option<RestorePayloadPlan>> {
        let sha256 = self
            .conn
            .query_row(
                "SELECT sha256 FROM snapshots WHERE id = ?1",
                [snapshot_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .context("load restore snapshot identity")?;
        let Some(snapshot_sha256) = sha256 else {
            return Ok(None);
        };
        Ok(Some(RestorePayloadPlan {
            snapshot_sha256,
            items: self.load_snapshot_items(snapshot_id)?,
        }))
    }

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

        details.set_recent_events(
            self.load_recent_events(snapshot_id, clamp_result_limit(event_limit))?,
        );
        details.set_items(self.load_snapshot_items(snapshot_id)?);

        Ok(Some(details))
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
        Ok(self
            .read_representation_payload(snapshot_id, item_index, uti)?
            .map(|payload| {
                ClipboardRepresentation::new(
                    payload.manifest.uti,
                    payload.manifest.kind,
                    payload.manifest.raw_sha256,
                    payload.manifest.text_value,
                    payload.bytes,
                )
            }))
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

    fn load_snapshot_manifests(&self, snapshot_id: i64) -> Result<Vec<SnapshotItemManifest>> {
        let mut stmt = self.conn.prepare(
            "SELECT si.item_index, si.primary_kind, si.primary_uti, si.preview_text, si.search_text, si.total_bytes,
                    ir.uti, ir.kind, ir.byte_len, ir.raw_sha256, ir.text_value
             FROM snapshot_items si
             LEFT JOIN item_representations ir ON ir.snapshot_id = si.snapshot_id AND ir.item_index = si.item_index
             WHERE si.snapshot_id = ?1
             ORDER BY si.item_index, ir.uti"
        ).context("prepare snapshot manifest query")?;
        let rows = stmt
            .query_map([snapshot_id], |row| {
                let uti = row.get::<_, Option<String>>(6)?;
                let representation = match uti {
                    Some(uti) => {
                        let text_value = row.get::<_, Option<String>>(10)?;
                        Some(RepresentationManifest {
                            uti,
                            kind: row_enum(row, 7)?,
                            is_text: text_value.is_some(),
                            byte_len: row_usize(row, 8)?,
                            raw_sha256: row.get(9)?,
                            text_value,
                        })
                    }
                    None => None,
                };
                Ok((
                    row_usize(row, 0)?,
                    row_enum(row, 1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row_usize(row, 5)?,
                    representation,
                ))
            })
            .context("execute snapshot manifest query")?;
        let mut items: Vec<SnapshotItemManifest> = Vec::new();
        for (
            item_index,
            primary_kind,
            primary_uti,
            preview_text,
            search_text,
            total_bytes,
            representation,
        ) in collect_rows(rows)?
        {
            if items
                .last()
                .is_none_or(|item| item.item_index != item_index)
            {
                items.push(SnapshotItemManifest {
                    item_index,
                    primary_kind,
                    primary_uti,
                    preview_text,
                    search_text,
                    total_bytes,
                    representations: Vec::new(),
                });
            }
            if let Some(representation) = representation {
                items
                    .last_mut()
                    .expect("item was just inserted")
                    .representations
                    .push(representation);
            }
        }
        Ok(items)
    }
}

fn projection_from_manifests(items: &[SnapshotItemManifest]) -> FlattenedTextProjection {
    let items = items
        .iter()
        .map(|item| {
            ClipboardItem::from_manifest(
                item.item_index,
                item.primary_kind,
                item.primary_uti.clone(),
                item.preview_text.clone(),
                item.search_text.clone(),
                item.total_bytes,
                item.representations
                    .iter()
                    .map(|rep| {
                        ClipboardRepresentation::from_manifest(
                            rep.uti.clone(),
                            rep.kind,
                            rep.byte_len,
                            rep.raw_sha256.clone(),
                            rep.text_value.clone(),
                        )
                    })
                    .collect(),
            )
        })
        .collect::<Vec<_>>();
    FlattenedTextProjection::from_items(&items)
}

#[cfg(test)]
mod profile_tests {
    use std::time::{Duration, Instant};

    use anyhow::{Context, Result};
    use rusqlite::params;

    use crate::db::sqlite_helpers::{collect_rows, row_enum, row_usize, usize_to_i64};
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
