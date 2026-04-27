use rusqlite::{Error as SqlError, ErrorCode};

use crate::db::read::file_url::{file_url_cache_path_fragment, normalize_file_path};

#[derive(Debug, Clone)]
pub(in crate::db) struct QueryAnalysis {
    pub(in crate::db) trimmed: String,
    pub(in crate::db) lower: String,
    pub(in crate::db) exact_phrase: Option<String>,
    pub(in crate::db) literal_preferred: bool,
    pub(in crate::db) path_fragment: Option<String>,
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
