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

    eprintln!("snapshot_items_n_plus_one_before={before:?} snapshot_items_grouped_after={after:?}");
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
                    let text =
                        format!("snapshot item {item_index} representation {representation_index}");
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
                SELECT uti, kind, raw_sha256, text_value, blob_value
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
