use rusqlite::Row;

use crate::db::read::value_decoding::{
    split_aggregated_file_paths, split_aggregated_values, split_match_fields,
};
use crate::db::sqlite_helpers::{row_enum, row_usize};
use crate::model::{SearchHit, SearchHitParts, TimelineEvent};

pub(in crate::db) fn map_search_hit_row(
    row: &Row<'_>,
    has_score: bool,
) -> rusqlite::Result<SearchHit> {
    let urls = split_aggregated_values(&row.get::<_, String>("urls")?);
    let file_paths = split_aggregated_file_paths(&row.get::<_, String>("file_urls")?);
    let matched_fields = split_match_fields(&row.get::<_, String>("matched_fields")?);

    Ok(SearchHit::from_parts(SearchHitParts {
        snapshot_id: row.get("snapshot_id")?,
        event_id: row.get("event_id")?,
        sha256: row.get("sha256")?,
        snapshot_kind: row_enum_named(row, "snapshot_kind")?,
        preview_text: row.get("preview_text")?,
        search_text: row.get("search_text")?,
        why_matched: row.get("why_matched")?,
        matched_fields,
        capture_count: row_usize_named(row, "capture_count")?,
        first_observed_at: row.get("first_observed_at")?,
        last_observed_at: row.get("last_observed_at")?,
        last_frontmost_app_name: row.get("last_frontmost_app_name")?,
        last_frontmost_app_bundle_id: row.get("last_frontmost_app_bundle_id")?,
        urls,
        file_paths,
        total_bytes: row_usize_named(row, "total_bytes")?,
        item_count: row_usize_named(row, "item_count")?,
        score: if has_score { row.get("score")? } else { None },
    }))
}

pub(in crate::db) fn map_scored_search_hit_row(row: &Row<'_>) -> rusqlite::Result<SearchHit> {
    map_search_hit_row(row, true)
}

pub(in crate::db) fn map_timeline_event_row(row: &Row<'_>) -> rusqlite::Result<TimelineEvent> {
    let urls = split_aggregated_values(&row.get::<_, String>("urls")?);
    let file_paths = split_aggregated_file_paths(&row.get::<_, String>("file_urls")?);

    Ok(TimelineEvent::new(
        row.get("event_id")?,
        row.get("snapshot_id")?,
        row.get("observed_at")?,
        row.get("change_count")?,
        row.get("sha256")?,
        row_enum_named(row, "snapshot_kind")?,
        row.get("best_text")?,
        row.get("preview_text")?,
        row.get("frontmost_app_name")?,
        row.get("frontmost_app_bundle_id")?,
        urls,
        file_paths,
        row_usize_named(row, "total_bytes")?,
        row_usize_named(row, "item_count")?,
    ))
}

fn row_enum_named<T>(row: &Row<'_>, column: &str) -> rusqlite::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    let index = row.as_ref().column_index(column)?;
    row_enum(row, index)
}

fn row_usize_named(row: &Row<'_>, column: &str) -> rusqlite::Result<usize> {
    let index = row.as_ref().column_index(column)?;
    row_usize(row, index)
}
