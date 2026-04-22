use super::*;

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

pub(in crate::db) fn snapshot_filter_clause(
    snapshot_alias: &str,
    snapshot_id_expr: &str,
) -> String {
    format!(
        "(:min_bytes IS NULL OR {snapshot_alias}.total_bytes >= :min_bytes)
         AND (:max_bytes IS NULL OR {snapshot_alias}.total_bytes <= :max_bytes)
         AND (
             :kind IS NULL
             OR (:kind = 'text' AND EXISTS (
                 SELECT 1 FROM item_representations ir
                 WHERE ir.snapshot_id = {snapshot_id_expr}
                   AND ir.kind IN ('plain_text', 'html', 'json', 'xml', 'rtf')
             ))
             OR (:kind = 'html' AND EXISTS (
                 SELECT 1 FROM item_representations ir
                 WHERE ir.snapshot_id = {snapshot_id_expr}
                   AND ir.kind = 'html'
             ))
             OR (:kind = 'rtf' AND EXISTS (
                 SELECT 1 FROM item_representations ir
                 WHERE ir.snapshot_id = {snapshot_id_expr}
                   AND ir.kind = 'rtf'
             ))
             OR (:kind = 'url' AND EXISTS (
                 SELECT 1 FROM item_representations ir
                 WHERE ir.snapshot_id = {snapshot_id_expr}
                   AND ir.kind = 'url'
             ))
             OR (:kind = 'file' AND EXISTS (
                 SELECT 1 FROM item_representations ir
                 WHERE ir.snapshot_id = {snapshot_id_expr}
                   AND ir.kind = 'file_url'
             ))
             OR (:kind = 'image' AND EXISTS (
                 SELECT 1 FROM item_representations ir
                 WHERE ir.snapshot_id = {snapshot_id_expr}
                   AND ir.kind = 'image'
             ))
             OR (:kind = 'pdf' AND EXISTS (
                 SELECT 1 FROM item_representations ir
                 WHERE ir.snapshot_id = {snapshot_id_expr}
                   AND ir.kind = 'pdf'
             ))
             OR (:kind = 'binary' AND EXISTS (
                 SELECT 1 FROM item_representations ir
                 WHERE ir.snapshot_id = {snapshot_id_expr}
                   AND ir.kind = 'binary'
             ))
             OR (:kind = 'other' AND {snapshot_alias}.snapshot_kind IN ('mixed', 'empty'))
         )
         AND (:has_text = 0 OR (
             ({snapshot_alias}.preview_text IS NOT NULL AND {snapshot_alias}.preview_text != '')
             OR EXISTS (
                 SELECT 1 FROM item_representations ir
                 WHERE ir.snapshot_id = {snapshot_id_expr}
                   AND ir.kind IN ('plain_text', 'url', 'file_url', 'html', 'json', 'xml', 'rtf')
                   AND ir.text_value IS NOT NULL AND ir.text_value != ''
             )
             OR EXISTS (
                 SELECT 1 FROM snapshot_ocr_cache soc
                 WHERE soc.snapshot_id = {snapshot_id_expr}
                   AND soc.ocr_text != ''
             )
         ))
         AND (:has_url = 0 OR EXISTS (
             SELECT 1 FROM item_representations ir
             WHERE ir.snapshot_id = {snapshot_id_expr}
               AND ir.kind = 'url'
         ))
         AND (:has_file_url = 0 OR EXISTS (
             SELECT 1 FROM item_representations ir
             WHERE ir.snapshot_id = {snapshot_id_expr}
               AND ir.kind = 'file_url'
         ))
         AND (:has_image = 0 OR EXISTS (
             SELECT 1 FROM item_representations ir
             WHERE ir.snapshot_id = {snapshot_id_expr}
               AND ir.kind = 'image'
         ))
         AND (:has_pdf = 0 OR EXISTS (
             SELECT 1 FROM item_representations ir
             WHERE ir.snapshot_id = {snapshot_id_expr}
               AND ir.kind = 'pdf'
         ))"
    )
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
    let urls = split_aggregated_values(&row.get::<_, String>(13)?);
    let file_paths = split_aggregated_values(&row.get::<_, String>(14)?)
        .into_iter()
        .map(|value| normalise_file_path(&value))
        .collect::<Vec<_>>();
    let matched_fields = split_match_fields(&row.get::<_, String>(7)?);

    Ok(SearchHit::new(
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row_enum(row, 3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        matched_fields,
        row_usize(row, 8)?,
        row.get(9)?,
        row.get(10)?,
        row.get(11)?,
        row.get(12)?,
        urls,
        file_paths,
        row_usize(row, 15)?,
        row_usize(row, 16)?,
        if has_score { row.get(17)? } else { None },
    ))
}

pub(in crate::db) fn map_scored_search_hit_row(row: &Row<'_>) -> rusqlite::Result<SearchHit> {
    map_search_hit_row(row, true)
}

pub(in crate::db) fn map_timeline_event_row(row: &Row<'_>) -> rusqlite::Result<TimelineEvent> {
    let urls = split_aggregated_values(&row.get::<_, String>(10)?);
    let file_paths = split_aggregated_values(&row.get::<_, String>(11)?)
        .into_iter()
        .map(|value| normalise_file_path(&value))
        .collect::<Vec<_>>();

    Ok(TimelineEvent::new(
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row_enum(row, 5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        urls,
        file_paths,
        row_usize(row, 12)?,
        row_usize(row, 13)?,
    ))
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

pub(in crate::db) fn normalise_file_path(value: &str) -> String {
    decode_file_url_path(value).unwrap_or_else(|| value.to_string())
}

pub(in crate::db) fn decode_file_url_path(value: &str) -> Option<String> {
    let rest = value.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let decoded = percent_decode(rest);

    if decoded.is_empty() {
        None
    } else {
        Some(decoded)
    }
}

pub(in crate::db) fn percent_decode(value: &str) -> String {
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
        Some(value.trim_start_matches('/').to_ascii_lowercase())
    } else if lower.starts_with("file://") {
        Some(normalise_file_path(&trimmed).to_ascii_lowercase())
    } else if trimmed.starts_with('/')
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
        || (trimmed.contains('\\')
            && (trimmed.starts_with(".\\")
                || trimmed.starts_with("..\\")
                || trimmed.starts_with('\\')
                || trimmed.contains(":\\")))
    {
        Some(lower.clone())
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
