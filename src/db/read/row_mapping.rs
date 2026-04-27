use rusqlite::Row;

use crate::db::read::file_url::normalize_file_path;
use crate::db::sqlite_helpers::{row_enum, row_usize};
use crate::db::types::{Page, SearchMode, SearchResults};
use crate::model::{SearchHit, SearchHitParts, TimelineEvent};

const LIST_VALUE_SEPARATOR: char = '\u{1f}';
const MATCHED_FIELDS_SEPARATOR: char = '\u{1e}';

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

pub(in crate::db) fn paginate_rows(mut rows: Vec<SearchHit>, limit: usize) -> Page<SearchHit> {
    let has_more = rows.len() > limit;
    if has_more {
        rows.truncate(limit);
    }

    Page::new(rows, has_more)
}

pub(in crate::db) fn merge_scored_search_results(
    mode: SearchMode,
    native: SearchResults,
    ocr: SearchResults,
    limit: usize,
    lower_score_is_better: bool,
) -> SearchResults {
    let mut rows = native.hits().to_vec();
    let mut seen = rows
        .iter()
        .map(SearchHit::snapshot_id)
        .collect::<std::collections::HashSet<_>>();
    for hit in ocr.hits() {
        if seen.insert(hit.snapshot_id()) {
            rows.push(hit.clone());
        }
    }

    rows.sort_by(|left, right| {
        let left_score = left.score().unwrap_or(if lower_score_is_better {
            f64::INFINITY
        } else {
            f64::NEG_INFINITY
        });
        let right_score = right.score().unwrap_or(if lower_score_is_better {
            f64::INFINITY
        } else {
            f64::NEG_INFINITY
        });
        let score_order = if lower_score_is_better {
            left_score
                .partial_cmp(&right_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        } else {
            right_score
                .partial_cmp(&left_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        };
        score_order
            .then_with(|| right.last_observed_at().cmp(left.last_observed_at()))
            .then_with(|| right.snapshot_id().cmp(&left.snapshot_id()))
    });

    let has_more = native.has_more() || ocr.has_more() || rows.len() > limit;
    if rows.len() > limit {
        rows.truncate(limit);
    }
    SearchResults::new(mode, rows, has_more)
}

pub(in crate::db) fn paginate_timeline_rows(
    mut rows: Vec<TimelineEvent>,
    limit: usize,
) -> Page<TimelineEvent> {
    let has_more = rows.len() > limit;
    if has_more {
        rows.truncate(limit);
    }

    Page::new(rows, has_more)
}

pub(in crate::db) fn split_aggregated_values(value: &str) -> Vec<String> {
    if value.is_empty() {
        Vec::new()
    } else {
        value
            .split(LIST_VALUE_SEPARATOR)
            .filter(|entry| !entry.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }
}

pub(in crate::db) fn split_aggregated_file_paths(value: &str) -> Vec<String> {
    if value.is_empty() {
        Vec::new()
    } else {
        value
            .split(LIST_VALUE_SEPARATOR)
            .filter(|entry| !entry.is_empty())
            .map(normalize_file_path)
            .collect()
    }
}

pub(in crate::db) fn split_match_fields(value: &str) -> Vec<String> {
    if value.is_empty() {
        Vec::new()
    } else {
        value
            .split(MATCHED_FIELDS_SEPARATOR)
            .filter(|entry| !entry.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }
}

#[cfg(test)]
mod profile_tests {
    use std::time::{Duration, Instant};

    use super::{split_aggregated_file_paths, split_aggregated_values, LIST_VALUE_SEPARATOR};
    use crate::db::read::file_url::hex_value;

    #[test]
    #[ignore = "profiling harness for aggregated file path row mapping"]
    fn profile_aggregated_file_path_normalization() {
        let aggregated = large_file_url_list(20_000);

        let before = median_duration(11, 20_000, || {
            let paths = split_aggregated_file_paths_before_for_profile(&aggregated);
            assert_eq!(
                paths.last().map(String::as_str),
                Some("/Users/test/project-19999/Cargo.toml")
            );
            paths.len()
        });
        let after = median_duration(11, 20_000, || {
            let paths = split_aggregated_file_paths(&aggregated);
            assert_eq!(
                paths.last().map(String::as_str),
                Some("/Users/test/project-19999/Cargo.toml")
            );
            paths.len()
        });

        eprintln!(
            "aggregated_file_paths_allocate_then_decode_before={before:?} aggregated_file_paths_stream_after={after:?}"
        );
    }

    fn median_duration(
        runs: usize,
        expected_count: usize,
        mut f: impl FnMut() -> usize,
    ) -> Duration {
        let mut samples = Vec::with_capacity(runs);
        for _ in 0..runs {
            let started = Instant::now();
            let count = f();
            assert_eq!(count, expected_count);
            samples.push(started.elapsed());
        }
        samples.sort();
        samples[samples.len() / 2]
    }

    fn large_file_url_list(path_count: usize) -> String {
        let mut out = String::with_capacity(path_count * 48);
        for index in 0..path_count {
            if index > 0 {
                out.push(LIST_VALUE_SEPARATOR);
            }
            out.push_str("file:///Users/test/project-");
            out.push_str(&index.to_string());
            out.push_str("/Cargo.toml");
        }
        out
    }

    fn split_aggregated_file_paths_before_for_profile(value: &str) -> Vec<String> {
        split_aggregated_values(value)
            .into_iter()
            .map(|value| normalize_file_path_before_for_profile(&value))
            .collect()
    }

    fn normalize_file_path_before_for_profile(value: &str) -> String {
        decode_file_url_path_before_for_profile(value).unwrap_or_else(|| value.to_string())
    }

    fn decode_file_url_path_before_for_profile(value: &str) -> Option<String> {
        let rest = value.strip_prefix("file://")?;
        let rest = rest.strip_prefix("localhost").unwrap_or(rest);
        let decoded = percent_decode_before_for_profile(rest);

        if decoded.is_empty() {
            None
        } else {
            Some(decoded)
        }
    }

    fn percent_decode_before_for_profile(value: &str) -> String {
        let bytes = value.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut index = 0;

        while index < bytes.len() {
            if bytes[index] == b'%' && index + 2 < bytes.len() {
                if let (Some(first), Some(second)) =
                    (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
                {
                    out.push((first << 4) | second);
                    index += 3;
                    continue;
                }
            }

            out.push(bytes[index]);
            index += 1;
        }

        String::from_utf8_lossy(&out).into_owned()
    }
}
