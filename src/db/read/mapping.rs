use anyhow::Result;
use rusqlite::{Error as SqlError, ErrorCode, Row};

use crate::db::core::{row_enum, row_usize};
use crate::db::read::search::{QueryAnalysis, LIST_VALUE_SEPARATOR, MATCHED_FIELDS_SEPARATOR};
use crate::db::types::{Page, RetrievalFilters, SearchMode, SearchResults};
use crate::model::{SearchHit, SearchHitParts, TimelineEvent};

pub(in crate::db) fn has_temporal_event_filters(filters: &RetrievalFilters) -> bool {
    filters.since().is_some() || filters.until().is_some() || filters.hours().is_some()
}

pub(in crate::db) fn requires_matching_events(filters: &RetrievalFilters) -> bool {
    has_temporal_event_filters(filters)
}

pub(in crate::db) fn can_use_snapshot_stats_since_filter(filters: &RetrievalFilters) -> bool {
    (filters.since().is_some() || filters.hours().is_some())
        && filters.until().is_none()
        && filters.app().is_none()
        && filters.bundle_id().is_none()
}

pub(in crate::db) fn can_use_snapshot_stats_for_stats(filters: &RetrievalFilters) -> bool {
    !has_temporal_event_filters(filters) && filters.app().is_none() && filters.bundle_id().is_none()
}

pub(in crate::db) fn can_use_snapshot_event_cache(filters: &RetrievalFilters) -> bool {
    !has_temporal_event_filters(filters)
        && (filters.app().is_some() || filters.bundle_id().is_some())
}

pub(in crate::db) fn event_filter_clause(alias: &str) -> String {
    format!(
        "(:since IS NULL OR datetime({alias}.observed_at) >= datetime(:since))
         AND (:until IS NULL OR datetime({alias}.observed_at) <= datetime(:until))
         AND (:app_like IS NULL OR ({alias}.frontmost_app_name IS NOT NULL AND lower({alias}.frontmost_app_name) LIKE :app_like ESCAPE '\\'))
         AND (:bundle_id IS NULL OR ({alias}.frontmost_app_bundle_id IS NOT NULL AND lower({alias}.frontmost_app_bundle_id) = :bundle_id))"
    )
}

pub(in crate::db) fn event_filter_bind_clause() -> &'static str {
    "(:since IS NULL OR 1)
     AND (:until IS NULL OR 1)
     AND (:app_like IS NULL OR 1)
     AND (:bundle_id IS NULL OR 1)"
}

pub(in crate::db) fn snapshot_event_filter_clause(cache_alias: &str) -> String {
    format!(
        "(:since IS NULL OR 1)
         AND (:until IS NULL OR 1)
         AND (:app_like IS NULL OR ({cache_alias}.app_names_lower != '' AND {cache_alias}.app_names_lower LIKE :app_like ESCAPE '\\'))
         AND (:bundle_id IS NULL OR instr(char(31) || {cache_alias}.bundle_ids_lower || char(31), char(31) || :bundle_id || char(31)) > 0)"
    )
}

pub(in crate::db) fn base_event_filter_clause(
    cache_alias: &str,
    include_matching_events: bool,
    use_snapshot_event_cache: bool,
) -> String {
    if include_matching_events {
        event_filter_bind_clause().to_string()
    } else if use_snapshot_event_cache {
        snapshot_event_filter_clause(cache_alias)
    } else {
        event_filter_bind_clause().to_string()
    }
}

pub(in crate::db) fn snapshot_stats_since_filter_clause(
    use_snapshot_stats_since_filter: bool,
) -> &'static str {
    if use_snapshot_stats_since_filter {
        "ss.last_observed_at >= datetime(:since)
         AND datetime(ss.last_observed_at) >= datetime(:since)"
    } else {
        "(:since IS NULL OR 1)"
    }
}

pub(in crate::db) fn event_filter_where_clause(
    snapshot_id_expr: &str,
    cache_alias: &str,
    use_snapshot_event_cache: bool,
    has_temporal_event_filters: bool,
) -> String {
    if use_snapshot_event_cache {
        snapshot_event_filter_clause(cache_alias)
    } else if has_temporal_event_filters {
        format!(
            "EXISTS (
                 SELECT 1
                 FROM capture_events ce
                 WHERE ce.snapshot_id = {snapshot_id_expr}
                   AND {}
             )",
            event_filter_clause("ce")
        )
    } else {
        event_filter_bind_clause().to_string()
    }
}

struct RetrievalKindPredicate {
    parameter_value: &'static str,
    predicate: SnapshotPredicate,
}

enum SnapshotPredicate {
    RepresentationKinds(&'static [&'static str]),
    SnapshotKinds(&'static [&'static str]),
}

struct PresencePredicate {
    parameter: &'static str,
    predicate: PresencePredicateSql,
}

enum PresencePredicateSql {
    SearchableText,
    RepresentationKinds(&'static [&'static str]),
}

const TEXT_RETRIEVAL_KINDS: &[&str] = &["plain_text", "html", "json", "xml", "rtf"];
const TEXT_PRESENCE_KINDS: &[&str] = &[
    "plain_text",
    "url",
    "file_url",
    "html",
    "json",
    "xml",
    "rtf",
];
const OTHER_SNAPSHOT_KINDS: &[&str] = &["mixed", "empty"];

const RETRIEVAL_KIND_PREDICATES: &[RetrievalKindPredicate] = &[
    RetrievalKindPredicate {
        parameter_value: "text",
        predicate: SnapshotPredicate::RepresentationKinds(TEXT_RETRIEVAL_KINDS),
    },
    RetrievalKindPredicate {
        parameter_value: "html",
        predicate: SnapshotPredicate::RepresentationKinds(&["html"]),
    },
    RetrievalKindPredicate {
        parameter_value: "rtf",
        predicate: SnapshotPredicate::RepresentationKinds(&["rtf"]),
    },
    RetrievalKindPredicate {
        parameter_value: "url",
        predicate: SnapshotPredicate::RepresentationKinds(&["url"]),
    },
    RetrievalKindPredicate {
        parameter_value: "file",
        predicate: SnapshotPredicate::RepresentationKinds(&["file_url"]),
    },
    RetrievalKindPredicate {
        parameter_value: "image",
        predicate: SnapshotPredicate::RepresentationKinds(&["image"]),
    },
    RetrievalKindPredicate {
        parameter_value: "pdf",
        predicate: SnapshotPredicate::RepresentationKinds(&["pdf"]),
    },
    RetrievalKindPredicate {
        parameter_value: "binary",
        predicate: SnapshotPredicate::RepresentationKinds(&["binary"]),
    },
    RetrievalKindPredicate {
        parameter_value: "other",
        predicate: SnapshotPredicate::SnapshotKinds(OTHER_SNAPSHOT_KINDS),
    },
];

const PRESENCE_PREDICATES: &[PresencePredicate] = &[
    PresencePredicate {
        parameter: ":has_text",
        predicate: PresencePredicateSql::SearchableText,
    },
    PresencePredicate {
        parameter: ":has_url",
        predicate: PresencePredicateSql::RepresentationKinds(&["url"]),
    },
    PresencePredicate {
        parameter: ":has_file_url",
        predicate: PresencePredicateSql::RepresentationKinds(&["file_url"]),
    },
    PresencePredicate {
        parameter: ":has_image",
        predicate: PresencePredicateSql::RepresentationKinds(&["image"]),
    },
    PresencePredicate {
        parameter: ":has_pdf",
        predicate: PresencePredicateSql::RepresentationKinds(&["pdf"]),
    },
];

pub(in crate::db) fn snapshot_filter_clause(
    snapshot_alias: &str,
    snapshot_id_expr: &str,
) -> String {
    let mut clauses = vec![
        format!("(:min_bytes IS NULL OR {snapshot_alias}.total_bytes >= :min_bytes)"),
        format!("(:max_bytes IS NULL OR {snapshot_alias}.total_bytes <= :max_bytes)"),
        retrieval_kind_filter_clause(snapshot_alias, snapshot_id_expr),
    ];
    clauses.extend(
        PRESENCE_PREDICATES
            .iter()
            .map(|predicate| presence_filter_clause(predicate, snapshot_alias, snapshot_id_expr)),
    );
    clauses.join("\n         AND ")
}

fn retrieval_kind_filter_clause(snapshot_alias: &str, snapshot_id_expr: &str) -> String {
    let predicates = RETRIEVAL_KIND_PREDICATES
        .iter()
        .map(|predicate| {
            format!(
                "(:kind = '{}' AND {})",
                predicate.parameter_value,
                snapshot_predicate_sql(&predicate.predicate, snapshot_alias, snapshot_id_expr)
            )
        })
        .collect::<Vec<_>>()
        .join("\n             OR ");

    format!("(:kind IS NULL\n             OR {predicates})")
}

fn presence_filter_clause(
    predicate: &PresencePredicate,
    snapshot_alias: &str,
    snapshot_id_expr: &str,
) -> String {
    format!(
        "({} = 0 OR {})",
        predicate.parameter,
        presence_predicate_sql(&predicate.predicate, snapshot_alias, snapshot_id_expr)
    )
}

fn snapshot_predicate_sql(
    predicate: &SnapshotPredicate,
    snapshot_alias: &str,
    snapshot_id_expr: &str,
) -> String {
    match predicate {
        SnapshotPredicate::RepresentationKinds(kinds) => {
            representation_kind_exists_clause(snapshot_id_expr, kinds, false)
        }
        SnapshotPredicate::SnapshotKinds(kinds) => {
            format!(
                "{snapshot_alias}.snapshot_kind IN ({})",
                sql_string_list(kinds)
            )
        }
    }
}

fn presence_predicate_sql(
    predicate: &PresencePredicateSql,
    snapshot_alias: &str,
    snapshot_id_expr: &str,
) -> String {
    match predicate {
        PresencePredicateSql::SearchableText => format!(
            "(
             ({snapshot_alias}.preview_text IS NOT NULL AND {snapshot_alias}.preview_text != '')
             OR {}
             OR EXISTS (
                 SELECT 1 FROM snapshot_ocr_cache soc
                 WHERE soc.snapshot_id = {snapshot_id_expr}
                   AND soc.ocr_text != ''
             )
         )",
            representation_kind_exists_clause(snapshot_id_expr, TEXT_PRESENCE_KINDS, true)
        ),
        PresencePredicateSql::RepresentationKinds(kinds) => {
            representation_kind_exists_clause(snapshot_id_expr, kinds, false)
        }
    }
}

fn representation_kind_exists_clause(
    snapshot_id_expr: &str,
    kinds: &[&str],
    require_text_value: bool,
) -> String {
    let text_value_clause = if require_text_value {
        "\n                   AND ir.text_value IS NOT NULL AND ir.text_value != ''"
    } else {
        ""
    };
    format!(
        "EXISTS (
                 SELECT 1 FROM item_representations ir
                 WHERE ir.snapshot_id = {snapshot_id_expr}
                   AND ir.kind IN ({}){text_value_clause}
             )",
        sql_string_list(kinds)
    )
}

fn sql_string_list(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| format!("'{value}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(in crate::db) fn app_like_pattern(filters: &RetrievalFilters) -> Option<String> {
    filters
        .app()
        .map(|value| format!("%{}%", escape_like_pattern(&value.to_ascii_lowercase())))
}

pub(in crate::db) fn is_simple_fts_query(analysis: &QueryAnalysis) -> bool {
    analysis.exact_phrase.is_none()
        && !analysis.literal_preferred
        && !analysis.trimmed.is_empty()
        && analysis
            .trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric())
}

pub(in crate::db) fn effective_since_param(filters: &RetrievalFilters) -> Result<Option<String>> {
    if let Some(since) = filters.since() {
        return Ok(Some(since.to_string()));
    }

    let Some(hours) = filters.hours() else {
        return Ok(None);
    };
    let since = (time::OffsetDateTime::now_utc() - time::Duration::hours(i64::from(hours)))
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|error| anyhow::anyhow!("format filter time: {error}"))?;
    Ok(Some(since))
}

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

pub(in crate::db) fn normalize_file_path(value: &str) -> String {
    decode_file_url_path(value).unwrap_or_else(|| value.to_string())
}

pub(in crate::db) fn decode_file_url_path(value: &str) -> Option<String> {
    const PREFIX: &str = "file://";
    if !has_ascii_prefix(value, PREFIX) {
        return None;
    }

    let rest = strip_localhost_authority(&value[PREFIX.len()..]);

    if rest.is_empty() {
        None
    } else if rest.as_bytes().contains(&b'%') {
        Some(percent_decode(rest))
    } else {
        Some(rest.to_string())
    }
}

fn strip_localhost_authority(value: &str) -> &str {
    const LOCALHOST: &str = "localhost";
    if has_ascii_prefix(value, LOCALHOST)
        && (value.len() == LOCALHOST.len() || value.as_bytes()[LOCALHOST.len()] == b'/')
    {
        &value[LOCALHOST.len()..]
    } else {
        value
    }
}

fn has_ascii_prefix(value: &str, prefix: &str) -> bool {
    value
        .as_bytes()
        .get(..prefix.len())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(prefix.as_bytes()))
}

pub(in crate::db) fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    if !bytes.contains(&b'%') {
        return value.to_string();
    }

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

pub(in crate::db) fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(in crate::db) fn analyze_query(query: &str) -> QueryAnalysis {
    let trimmed = query.trim().to_string();
    let lower = trimmed.to_ascii_lowercase();
    let exact_phrase = trimmed
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let is_url_like = lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("file://");
    let is_path_like = trimmed.starts_with("~/")
        || trimmed.starts_with('/')
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
        || trimmed.contains('\\');
    let is_bundle_id_like = lower.contains('.')
        && !lower.contains(' ')
        && !is_url_like
        && !lower.contains('/')
        && !lower.contains('\\')
        && !lower.starts_with("~/");
    let punctuation_heavy =
        trimmed.contains(':') || trimmed.contains('/') || trimmed.contains('\\');
    let shell_fragment = trimmed.contains(" -")
        || trimmed.contains(" --")
        || trimmed.contains(" | ")
        || trimmed.contains(" && ")
        || trimmed.contains('=')
        || trimmed.contains('$');
    let literal_preferred = trimmed.contains('%')
        || trimmed.contains('_')
        || trimmed.contains('\\')
        || is_url_like
        || is_path_like
        || is_bundle_id_like
        || punctuation_heavy
        || shell_fragment;
    let path_fragment = if let Some(value) = trimmed.strip_prefix("~/") {
        Some(file_url_cache_path_fragment(value.trim_start_matches('/')))
    } else if lower.starts_with("file://") {
        Some(file_url_cache_path_fragment(&normalize_file_path(&trimmed)))
    } else if trimmed.starts_with('/')
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
        || (trimmed.contains('\\')
            && (trimmed.starts_with(".\\")
                || trimmed.starts_with("..\\")
                || trimmed.starts_with('\\')
                || trimmed.contains(":\\")))
    {
        Some(file_url_cache_path_fragment(&trimmed))
    } else {
        None
    }
    .filter(|value| !value.is_empty());

    QueryAnalysis {
        trimmed,
        lower,
        exact_phrase,
        literal_preferred,
        path_fragment,
    }
}

pub(in crate::db) fn file_url_cache_path_fragment(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .bytes()
        .map(|byte| match byte {
            b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02x}"),
        })
        .collect()
}

pub(in crate::db) fn is_invalid_fts_query(error: &SqlError) -> bool {
    let sqlite_unknown_error = matches!(error.sqlite_error_code(), Some(ErrorCode::Unknown));

    match error {
        SqlError::SqlInputError { msg, .. } => invalid_fts_message(msg),
        SqlError::SqliteFailure(_, Some(msg)) if sqlite_unknown_error => invalid_fts_message(msg),
        _ => false,
    }
}

pub(in crate::db) fn invalid_fts_message(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("fts5: syntax error")
        || message.contains("malformed match expression")
        || message.contains("unterminated string")
        || message.contains("no such column:")
}

pub(in crate::db) fn escape_like_pattern(query: &str) -> String {
    query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

pub(in crate::db) fn literal_fts_match_query(analysis: &QueryAnalysis) -> Option<String> {
    let candidate = analysis.path_fragment.as_deref().unwrap_or(&analysis.lower);

    if candidate.chars().count() < 3 {
        return None;
    }

    if analysis
        .trimmed
        .chars()
        .any(|ch| matches!(ch, '%' | '_' | '\\'))
    {
        return None;
    }

    Some(format!("\"{}\"", candidate.replace('"', "\"\"")))
}

#[cfg(test)]
mod profile_tests {
    use std::time::{Duration, Instant};

    use super::{
        hex_value, split_aggregated_file_paths, split_aggregated_values, LIST_VALUE_SEPARATOR,
    };

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
