// Recall scoring heuristics: thresholds, query expansion, and per-hit
// bonus/penalty terms shared by the recall candidate ranking.
use crate::db::SearchMode;
use crate::model::SearchHit;

pub(in crate::cli) fn default_recall_threshold(mode_used: SearchMode) -> f64 {
    match mode_used {
        SearchMode::Fts => 0.68,
        SearchMode::Literal | SearchMode::Auto => 0.72,
    }
}

#[cfg(test)]
pub(in crate::cli) fn expanded_recall_queries(query: &str) -> Vec<String> {
    let _ = query;
    Vec::new()
}

#[cfg(test)]
pub(in crate::cli) fn term_stuffing_penalty(value: &str, query: &str) -> f64 {
    let terms = query
        .split_whitespace()
        .map(|term| {
            term.trim_matches(|ch: char| !ch.is_ascii_alphanumeric())
                .to_ascii_lowercase()
        })
        .filter(|term| term.len() >= 3)
        .collect::<Vec<_>>();
    if terms.is_empty() {
        return 0.0;
    }

    let value = value.to_ascii_lowercase();
    let max_occurrences = terms
        .iter()
        .map(|term| value.match_indices(term).count())
        .max()
        .unwrap_or(0);

    match max_occurrences {
        0..=3 => 0.0,
        4..=6 => 0.18,
        _ => 0.32,
    }
}

#[cfg(test)]
pub(in crate::cli) fn command_specificity_bonus(value: &str, query: &str) -> f64 {
    let query = query.to_ascii_lowercase();
    let value = value.to_ascii_lowercase();
    if !contains_any(
        &query,
        &[
            "cargo",
            "git",
            "pytest",
            "pnpm",
            "npm",
            "docker",
            "xcodebuild",
        ],
    ) {
        return 0.0;
    }

    let mut bonus = 0.0;
    if value.starts_with(query.split_whitespace().next().unwrap_or_default()) {
        bonus += 0.08;
    }
    if value.contains(" --")
        || value.contains(" && ")
        || value.contains(" | ")
        || value.contains(" -")
    {
        bonus += 0.1;
    }
    if query
        .split_whitespace()
        .filter(|term| term.len() >= 3)
        .all(|term| contains_ascii_case_insensitive(&value, term))
    {
        bonus += 0.08;
    }

    bonus
}

#[cfg(test)]
pub(in crate::cli) fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles
        .iter()
        .any(|needle| contains_ascii_case_insensitive(value, needle))
}

#[cfg(test)]
pub(in crate::cli) fn literal_match_score(hit: &SearchHit, query: &str) -> f64 {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return 0.0;
    }

    let candidates = [
        hit.why_matched().unwrap_or(hit.preview_text()),
        hit.preview_text(),
    ];

    if candidates
        .iter()
        .any(|value| value.trim().eq_ignore_ascii_case(&query))
    {
        return 0.95;
    }
    if candidates
        .iter()
        .any(|value| starts_with_ascii_case_insensitive(value, &query))
    {
        return 0.88;
    }
    if candidates
        .iter()
        .any(|value| contains_ascii_case_insensitive(value, &query))
    {
        return 0.78;
    }

    let query_terms = query
        .split_whitespace()
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();
    if query_terms.is_empty() {
        return 0.0;
    }

    let best_overlap = candidates
        .iter()
        .map(|value| {
            let matched = query_terms
                .iter()
                .filter(|term| contains_ascii_case_insensitive(value, term))
                .count();
            matched as f64 / query_terms.len() as f64
        })
        .fold(0.0, f64::max);

    (0.55 + best_overlap * 0.25).clamp(0.0, 0.82)
}

pub(in crate::cli) fn matches_preferred_app(hit: &SearchHit, prefer_app: Option<&str>) -> bool {
    let Some(prefer_app) = prefer_app.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    hit.last_frontmost_app_name()
        .map(|value| contains_ascii_case_insensitive(value, prefer_app))
        .unwrap_or(false)
        || hit
            .last_frontmost_app_bundle_id()
            .map(|value| contains_ascii_case_insensitive(value, prefer_app))
            .unwrap_or(false)
}

#[cfg(test)]
pub(in crate::cli) fn starts_with_ascii_case_insensitive(value: &str, prefix: &str) -> bool {
    let value = value.as_bytes();
    let prefix = prefix.as_bytes();
    value
        .get(..prefix.len())
        .is_some_and(|head| ascii_eq_ignore_case(head, prefix))
}

pub(in crate::cli) fn contains_ascii_case_insensitive(value: &str, needle: &str) -> bool {
    let value = value.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() {
        return true;
    }
    if needle.len() > value.len() {
        return false;
    }

    value
        .windows(needle.len())
        .any(|window| ascii_eq_ignore_case(window, needle))
}

pub(in crate::cli) fn ascii_eq_ignore_case(left: &[u8], right: &[u8]) -> bool {
    left.iter()
        .zip(right)
        .all(|(left, right)| left.eq_ignore_ascii_case(right))
}

pub(in crate::cli) fn app_preference_boost(app_preferred: bool) -> f64 {
    if app_preferred {
        0.12
    } else {
        0.0
    }
}

pub(in crate::cli) fn recent_index_boost(index: usize) -> f64 {
    match index {
        0 => 0.22,
        1 => 0.18,
        2 => 0.14,
        3 => 0.1,
        _ => 0.06f64.max(0.12 - (index as f64 * 0.01)),
    }
}

pub(in crate::cli) fn search_rank_bonus(index: usize) -> f64 {
    match index {
        0 => 0.05,
        1 => 0.04,
        2 => 0.03,
        3..=9 => 0.02,
        _ => 0.0,
    }
}
