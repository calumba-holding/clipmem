use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::Path;

use anyhow::{anyhow, Result};
use serde_json::json;

use crate::db::{Database, RetrievalFilters, SearchCursorState, SearchMode, SearchResults};
use crate::model::SearchHit;

use crate::cli::commands::runtime::open_existing_db;
use crate::cli::commands::types::{RecallCandidate, RecallCandidateSource, RecallComputation};
use crate::cli::output::{
    emit_recall_output, generated_at_now, RecallEnvelope, RecallMatchConfidence, RecallOutputRow,
    OUTPUT_SCHEMA_VERSION,
};
use crate::cli::schema::{RecallArgs, SearchArgs};

use super::filters::{
    load_snapshot_projections, merge_applied_filters, normalize_retrieval_filters,
};

pub(in crate::cli) fn recall(db_path: &Path, args: &RecallArgs) -> Result<()> {
    let format = args.output.resolved()?;
    let filters = normalize_retrieval_filters(&args.filters)?;
    let db = open_existing_db(db_path)?;
    let recall = anyhow::Context::context(
        compute_recall(&db, args, &filters),
        "recall failed; if this is unexpected, run `clipmem service status` and `clipmem doctor`",
    )?;
    let envelope = build_recall_envelope(&db, args, &filters, &recall)?;
    emit_recall_output(format, &envelope)
}

fn build_recall_envelope(
    db: &Database,
    args: &RecallArgs,
    filters: &RetrievalFilters,
    recall: &RecallComputation,
) -> Result<RecallEnvelope> {
    let generated_at = generated_at_now()?;
    let projections = load_snapshot_projections(
        db,
        std::iter::once(recall.best.hit.snapshot_id()).chain(
            recall
                .alternatives
                .iter()
                .map(|candidate| candidate.hit.snapshot_id()),
        ),
    )?;
    let best_projection = projections
        .get(&recall.best.hit.snapshot_id())
        .cloned()
        .unwrap_or_default();
    let best_candidate = RecallOutputRow::from_hit(&recall.best.hit, args.full, &best_projection);
    let best_match_score = Some(recall.best.normalized_score);
    let confidence = RecallMatchConfidence::from_normalized_score(recall.best.normalized_score);
    let quoted_text = args
        .quote
        .then(|| best_candidate.best_text.clone())
        .filter(|text| !text.is_empty());
    Ok(RecallEnvelope {
        schema_version: OUTPUT_SCHEMA_VERSION,
        command: "recall",
        generated_at,
        applied_filters: merge_applied_filters(
            filters,
            json!({
                "limit": args.limit,
                "query_present": args.query.is_some(),
                "requested_mode": args.query.as_ref().map(|_| args.mode.as_str()),
                "mode_used": recall.search_mode_used.map(SearchMode::as_str),
                "full": args.full,
                "quote": args.quote,
                "min_score": args.min_score,
                "prefer_recent": args.prefer_recent,
                "prefer_app": args.prefer_app,
            }),
        ),
        query: args.query.clone(),
        best_candidate,
        alternatives: recall
            .alternatives
            .iter()
            .map(|candidate| {
                let projection = projections
                    .get(&candidate.hit.snapshot_id())
                    .cloned()
                    .unwrap_or_default();
                RecallOutputRow::from_hit(&candidate.hit, false, &projection)
            })
            .collect(),
        best_match_confidence: confidence,
        best_match_score,
        why_selected: recall.why_selected.clone(),
        quoted_text,
    })
}

pub(in crate::cli) fn query_search_results(
    db: &Database,
    args: &SearchArgs,
    filters: &RetrievalFilters,
    cursor: Option<&SearchCursorState>,
) -> Result<SearchResults> {
    match args.mode {
        SearchMode::Auto => db.search_auto_page(&args.query, args.limit, filters, cursor),
        SearchMode::Fts => db.search_fts_page(&args.query, args.limit, filters, cursor),
        SearchMode::Literal => db.search_literal_page(&args.query, args.limit, filters, cursor),
    }
}

pub(in crate::cli) fn compute_recall(
    db: &Database,
    args: &RecallArgs,
    filters: &RetrievalFilters,
) -> Result<RecallComputation> {
    let query = args
        .query
        .as_deref()
        .map(str::trim)
        .filter(|query| !query.is_empty());
    let mut merged = HashMap::<i64, RecallCandidate>::new();
    let mut search_mode_used = None;
    let mut search_was_weak = false;

    if let Some(query) = query {
        let results = run_search_query(db, query, args.mode, args.limit, filters)?;
        search_mode_used = Some(results.mode_used());
        let search_candidates = results
            .hits()
            .iter()
            .enumerate()
            .map(|(index, hit)| {
                build_search_candidate(
                    hit,
                    query,
                    results.mode_used(),
                    index,
                    args.prefer_app.as_deref(),
                    args.prefer_recent,
                )
            })
            .collect::<Vec<_>>();

        let threshold = args
            .min_score
            .unwrap_or(default_recall_threshold(results.mode_used()));
        search_was_weak = search_candidates
            .first()
            .is_none_or(|candidate| candidate.normalized_score < threshold);

        for mut candidate in search_candidates {
            if search_was_weak {
                candidate.sort_score *= 0.45;
            }
            upsert_recall_candidate(&mut merged, candidate);
        }

        if search_was_weak {
            for (index, hit) in db
                .recent(args.limit, filters)?
                .into_hits()
                .into_iter()
                .enumerate()
            {
                upsert_recall_candidate(
                    &mut merged,
                    build_recent_candidate(
                        hit,
                        index,
                        args.prefer_app.as_deref(),
                        args.prefer_recent,
                    ),
                );
            }
        }
    } else {
        for (index, hit) in db
            .recent(args.limit, filters)?
            .into_hits()
            .into_iter()
            .enumerate()
        {
            upsert_recall_candidate(
                &mut merged,
                build_recent_candidate(hit, index, args.prefer_app.as_deref(), args.prefer_recent),
            );
        }
    }

    let mut ranked = merged.into_values().collect::<Vec<_>>();
    ranked.sort_by(compare_recall_candidates);

    let best = ranked
        .first()
        .cloned()
        .ok_or_else(|| {
            anyhow!(
                "no clipboard candidates matched the recall request; if this is unexpected, run `clipmem service status` to confirm the watcher is running"
            )
        })?;
    let alternatives = ranked
        .into_iter()
        .skip(1)
        .take(args.limit.saturating_sub(1))
        .collect::<Vec<_>>();
    let why_selected = build_recall_why_selected(
        &best,
        query,
        search_was_weak,
        args.prefer_recent,
        args.prefer_app.as_deref(),
    );

    Ok(RecallComputation {
        best,
        alternatives,
        why_selected,
        search_mode_used,
    })
}

pub(in crate::cli) fn run_search_query(
    db: &Database,
    query: &str,
    mode: SearchMode,
    limit: usize,
    filters: &RetrievalFilters,
) -> Result<SearchResults> {
    match mode {
        SearchMode::Auto => db.search_auto(query, limit, filters),
        SearchMode::Fts => db.search_fts(query, limit, filters),
        SearchMode::Literal => db.search_literal(query, limit, filters),
    }
}

pub(in crate::cli) fn build_search_candidate(
    hit: &SearchHit,
    query: &str,
    mode_used: SearchMode,
    index: usize,
    prefer_app: Option<&str>,
    prefer_recent: bool,
) -> RecallCandidate {
    let normalized_score = match mode_used {
        SearchMode::Fts => normalize_fts_score(hit.score()),
        SearchMode::Literal | SearchMode::Auto => literal_match_score(hit, query),
    };
    let app_preferred = matches_preferred_app(hit, prefer_app);
    let mut sort_score = normalized_score;
    sort_score += app_preference_boost(app_preferred);
    sort_score += search_match_field_bonus(hit);
    if prefer_recent {
        sort_score += recent_index_boost(index) * 0.6;
    }
    sort_score += search_rank_bonus(index);

    RecallCandidate {
        hit: hit.clone(),
        source: RecallCandidateSource::Search,
        normalized_score,
        sort_score,
        app_preferred,
    }
}

pub(in crate::cli) fn build_recent_candidate(
    hit: SearchHit,
    index: usize,
    prefer_app: Option<&str>,
    prefer_recent: bool,
) -> RecallCandidate {
    let app_preferred = matches_preferred_app(&hit, prefer_app);
    let text_bonus = if !hit.preview_text().trim().is_empty() {
        0.08
    } else {
        0.0
    };
    let mut normalized_score = 0.55 + recent_index_boost(index) + text_bonus;
    if prefer_recent {
        normalized_score += 0.08;
    }
    normalized_score += app_preference_boost(app_preferred);
    normalized_score = normalized_score.clamp(0.0, 0.99);

    RecallCandidate {
        hit,
        source: RecallCandidateSource::Recent,
        normalized_score,
        sort_score: normalized_score,
        app_preferred,
    }
}

pub(in crate::cli) fn upsert_recall_candidate(
    store: &mut HashMap<i64, RecallCandidate>,
    candidate: RecallCandidate,
) {
    match store.get_mut(&candidate.hit.snapshot_id()) {
        Some(existing) => {
            let replace = compare_recall_candidates(&candidate, existing) == Ordering::Less;
            if replace {
                *existing = candidate;
            }
        }
        None => {
            store.insert(candidate.hit.snapshot_id(), candidate);
        }
    }
}

pub(in crate::cli) fn compare_recall_candidates(
    left: &RecallCandidate,
    right: &RecallCandidate,
) -> Ordering {
    right
        .sort_score
        .partial_cmp(&left.sort_score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| {
            right
                .hit
                .last_observed_at()
                .cmp(left.hit.last_observed_at())
        })
        .then_with(|| right.hit.snapshot_id().cmp(&left.hit.snapshot_id()))
        .then_with(|| match (left.source, right.source) {
            (RecallCandidateSource::Search, RecallCandidateSource::Recent) => Ordering::Less,
            (RecallCandidateSource::Recent, RecallCandidateSource::Search) => Ordering::Greater,
            _ => Ordering::Equal,
        })
}

pub(in crate::cli) fn default_recall_threshold(mode_used: SearchMode) -> f64 {
    match mode_used {
        SearchMode::Fts => 0.68,
        SearchMode::Literal | SearchMode::Auto => 0.72,
    }
}

pub(in crate::cli) fn normalize_fts_score(score: Option<f64>) -> f64 {
    score
        .map(|value| 1.0 / (1.0 + value.max(0.0)))
        .unwrap_or(0.0)
}

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

pub(in crate::cli) fn search_match_field_bonus(hit: &SearchHit) -> f64 {
    let mut bonus = 0.0;
    if hit.matched_fields().iter().any(|field| field == "urls") {
        bonus += 0.06;
    }
    if hit
        .matched_fields()
        .iter()
        .any(|field| field == "file_paths" || field == "app_bundle_id")
    {
        bonus += 0.05;
    }
    if hit
        .matched_fields()
        .iter()
        .any(|field| field == "best_text")
    {
        bonus += 0.03;
    }
    bonus
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
        0 => 0.1,
        1 => 0.06,
        2 => 0.04,
        _ => 0.02,
    }
}

pub(in crate::cli) fn build_recall_why_selected(
    best: &RecallCandidate,
    query: Option<&str>,
    search_was_weak: bool,
    prefer_recent: bool,
    prefer_app: Option<&str>,
) -> String {
    let mut parts = Vec::new();

    match (query, best.source, search_was_weak) {
        (Some(query), RecallCandidateSource::Search, false) => {
            parts.push(format!(
                "Selected the strongest search match for \"{query}\""
            ));
        }
        (Some(query), RecallCandidateSource::Search, true) => {
            parts.push(format!(
                "Selected the best available query match for \"{query}\" after weak search results were merged with recent candidates"
            ));
        }
        (Some(_query), RecallCandidateSource::Recent, true) => {
            parts.push(
                "Fell back to recent clipboard items because query matches were weak".to_string(),
            );
        }
        (None, RecallCandidateSource::Recent, _) => {
            parts.push("Selected the most likely useful recent clipboard item".to_string());
        }
        _ => {
            parts.push("Selected the top-ranked clipboard candidate".to_string());
        }
    }

    if best.app_preferred {
        if let Some(prefer_app) = prefer_app {
            parts.push(format!("it matched the preferred app \"{prefer_app}\""));
        }
    }
    if prefer_recent && matches!(best.source, RecallCandidateSource::Recent) {
        parts.push("recency preference boosted this candidate".to_string());
    }

    parts.join("; ")
}

#[cfg(test)]
mod profile_tests {
    use super::{literal_match_score, matches_preferred_app};
    use crate::model::{SearchHit, SearchHitParts};
    use std::time::{Duration, Instant};

    #[test]
    #[ignore = "profiling harness for recall literal scoring"]
    fn profile_recall_literal_match_scoring() {
        let hits = large_recall_hits(20_000);
        let query = "needle term";

        let before = median_duration(11, || {
            let total = hits
                .iter()
                .map(|hit| literal_match_score_before_for_profile(hit, query))
                .sum::<f64>();
            assert!(total > 10_000.0);
        });
        let after = median_duration(11, || {
            let total = hits
                .iter()
                .map(|hit| literal_match_score(hit, query))
                .sum::<f64>();
            assert!(total > 10_000.0);
        });

        eprintln!(
            "recall_literal_lowercase_before={before:?} recall_literal_ascii_after={after:?}"
        );
    }

    #[test]
    #[ignore = "profiling harness for recall preferred-app matching"]
    fn profile_recall_preferred_app_matching() {
        let hits = large_recall_hits(20_000);

        let before = median_duration(11, || {
            let matches = hits
                .iter()
                .filter(|hit| matches_preferred_app_before_for_profile(hit, Some("terminal")))
                .count();
            assert_eq!(matches, hits.len());
        });
        let after = median_duration(11, || {
            let matches = hits
                .iter()
                .filter(|hit| matches_preferred_app(hit, Some("terminal")))
                .count();
            assert_eq!(matches, hits.len());
        });

        eprintln!("recall_preferred_app_lowercase_before={before:?} recall_preferred_app_ascii_after={after:?}");
    }

    fn median_duration(runs: usize, mut f: impl FnMut()) -> Duration {
        let mut samples = Vec::with_capacity(runs);
        for _ in 0..runs {
            let started = Instant::now();
            f();
            samples.push(started.elapsed());
        }
        samples.sort();
        samples[samples.len() / 2]
    }

    fn large_recall_hits(count: usize) -> Vec<SearchHit> {
        (0..count)
            .map(|index| {
                let preview = format!(
                    "Clipboard row {index} includes Needle text with surrounding words and term"
                );
                let why_matched = Some(format!(
                    "Search snippet {index} with another Needle Term occurrence"
                ));
                SearchHit::from_parts(
                    SearchHitParts::plain_text(index as i64 + 1, index as i64 + 100_000, preview)
                        .with_sha256(format!("{:064x}", index + 1))
                        .with_match(why_matched, vec!["best_text".to_string()])
                        .with_capture_summary(
                            1,
                            "2026-04-17 10:00:00".to_string(),
                            "2026-04-17 11:00:00".to_string(),
                        )
                        .with_last_frontmost_app(
                            Some("Terminal".to_string()),
                            Some("com.apple.Terminal".to_string()),
                        )
                        .with_size(128, 1)
                        .with_score(Some(0.25)),
                )
            })
            .collect()
    }

    fn literal_match_score_before_for_profile(hit: &SearchHit, query: &str) -> f64 {
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
            .any(|value| value.to_ascii_lowercase().starts_with(&query))
        {
            return 0.88;
        }
        if candidates
            .iter()
            .any(|value| value.to_ascii_lowercase().contains(&query))
        {
            return 0.78;
        }

        let query_terms = token_candidates_for_profile(&query);
        if query_terms.is_empty() {
            return 0.0;
        }

        let best_overlap = candidates
            .iter()
            .map(|value| {
                let lower = value.to_ascii_lowercase();
                let matched = query_terms
                    .iter()
                    .filter(|term| lower.contains(**term))
                    .count();
                matched as f64 / query_terms.len() as f64
            })
            .fold(0.0, f64::max);

        (0.55 + best_overlap * 0.25).clamp(0.0, 0.82)
    }

    fn matches_preferred_app_before_for_profile(hit: &SearchHit, prefer_app: Option<&str>) -> bool {
        let Some(prefer_app) = prefer_app.map(str::trim).filter(|value| !value.is_empty()) else {
            return false;
        };
        let prefer_app = prefer_app.to_ascii_lowercase();
        hit.last_frontmost_app_name()
            .map(|value| value.to_ascii_lowercase().contains(&prefer_app))
            .unwrap_or(false)
            || hit
                .last_frontmost_app_bundle_id()
                .map(|value| value.to_ascii_lowercase().contains(&prefer_app))
                .unwrap_or(false)
    }

    fn token_candidates_for_profile(query: &str) -> Vec<&str> {
        query
            .split_whitespace()
            .filter(|term| !term.is_empty())
            .collect()
    }
}
