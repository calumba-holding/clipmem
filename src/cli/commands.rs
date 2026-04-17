use std::path::Path;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::cmp::Ordering;
use std::collections::HashMap;

use crate::app::{format_watch_capture_line, process_watch_snapshot, WatchState};
use crate::db::{
    Database, RecentCursorState, RetrievalFilters, SearchCursorState, SearchMode, SearchResults,
    TimelineCursorState, TimelineSort,
};
use crate::model::{CaptureStoreResult, ClipboardSnapshot, SearchHit, TimelineEvent};
use crate::platform::{capture_snapshot, current_change_count};

use super::output::{
    emit_get_output, emit_json_or_text, emit_list_output, emit_recall_output, generated_at_now,
    render_capture_once_text, render_doctor_text, render_hits_text, render_search_results_text,
    render_snapshot_text, render_timeline_text, GetEnvelope, ListEnvelope, ListRow,
    RecallEnvelope, RecallMatchConfidence, RecallOutputRow, OUTPUT_SCHEMA_VERSION,
};
use super::{
    CaptureOnceArgs, Command, DoctorArgs, ExportArgs, GetArgs, OutputFormat, RecallArgs,
    RecentArgs, SearchArgs, TimelineArgs, WatchArgs,
};

#[derive(Debug, Serialize)]
struct CaptureOnceOutput {
    store: CaptureStoreResult,
    snapshot: ClipboardSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SearchCursorToken {
    command: String,
    query: String,
    requested_mode: SearchMode,
    mode_used: SearchMode,
    filters: RetrievalFilters,
    last_seen_at: String,
    snapshot_id: i64,
    score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RecentCursorToken {
    command: String,
    filters: RetrievalFilters,
    last_seen_at: String,
    snapshot_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TimelineCursorToken {
    command: String,
    filters: RetrievalFilters,
    sort: TimelineSort,
    observed_at: String,
    event_id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecallCandidateSource {
    Search,
    Recent,
}

#[derive(Debug, Clone)]
struct RecallCandidate {
    hit: SearchHit,
    source: RecallCandidateSource,
    normalized_score: f64,
    sort_score: f64,
    app_preferred: bool,
}

#[derive(Debug)]
struct RecallComputation {
    best: RecallCandidate,
    alternatives: Vec<RecallCandidate>,
    why_selected: String,
    search_mode_used: Option<SearchMode>,
}

pub(super) fn run_command(command: Command, db_path: &Path) -> Result<()> {
    match command {
        Command::Watch(args) => watch(db_path, &args),
        Command::CaptureOnce(args) => capture_once(db_path, &args),
        Command::Search(args) => search(db_path, &args),
        Command::Recent(args) => recent(db_path, &args),
        Command::Timeline(args) => timeline(db_path, &args),
        Command::Recall(args) => recall(db_path, &args),
        Command::Get(args) => show_snapshot(db_path, &args),
        Command::Export(args) => export_snapshot_bytes(db_path, &args),
        Command::Doctor(args) => doctor(db_path, &args),
    }
}

fn watch(db_path: &Path, args: &WatchArgs) -> Result<()> {
    let mut db = open_or_init_db(db_path)?;
    let interval_ms = args.interval_ms.max(50);
    let mut state = WatchState::new();

    loop {
        if let Err(err) = run_watch_iteration(&mut db, args, &mut state) {
            eprintln!("{err:#}");
        }

        thread::sleep(Duration::from_millis(interval_ms));
    }
}

fn run_watch_iteration(db: &mut Database, args: &WatchArgs, state: &mut WatchState) -> Result<()> {
    run_watch_iteration_with_capture(
        db,
        args,
        state,
        || current_change_count(),
        || capture_snapshot(),
    )
}

fn run_watch_iteration_with_capture<CountFn, CaptureFn>(
    db: &mut Database,
    args: &WatchArgs,
    state: &mut WatchState,
    current_change_count_fn: CountFn,
    capture_snapshot_fn: CaptureFn,
) -> Result<()>
where
    CountFn: FnOnce() -> Result<i64>,
    CaptureFn: FnOnce() -> Result<ClipboardSnapshot>,
{
    let change_count =
        anyhow::Context::context(current_change_count_fn(), "read clipboard change count failed")?;

    if !crate::app::should_capture_change(change_count, args.skip_initial, state) {
        return Ok(());
    }

    let snapshot = anyhow::Context::context(capture_snapshot_fn(), "capture failed")?;
    let result =
        anyhow::Context::context(process_watch_snapshot(db, &snapshot, state), "database write failed")?;

    if !args.quiet {
        if let Some(result) = result {
            println!("{}", format_watch_capture_line(&snapshot, &result));
        }
    }

    Ok(())
}

fn capture_once(db_path: &Path, args: &CaptureOnceArgs) -> Result<()> {
    let mut db = open_or_init_db(db_path)?;
    let snapshot =
        anyhow::Context::context(capture_snapshot(), "capture-once clipboard read failed")?;
    let store = anyhow::Context::context(
        db.store_capture(&snapshot),
        "capture-once database write failed",
    )?;
    let payload = CaptureOnceOutput { store, snapshot };
    emit_json_or_text(args.json, &payload, |output| {
        render_capture_once_text(&output.store, &output.snapshot)
    })?;

    Ok(())
}

fn search(db_path: &Path, args: &SearchArgs) -> Result<()> {
    let format = args.output.resolved()?;
    let filters = normalize_retrieval_filters(&args.filters)?;
    let db = open_existing_db(db_path)?;
    let cursor = args
        .cursor
        .as_deref()
        .map(|value| parse_search_cursor(value, &args.query, args.mode, &filters))
        .transpose()?;
    let results = anyhow::Context::with_context(
        query_search_results(&db, args, &filters, cursor.as_ref()),
        || format!("search failed for query '{}'", args.query),
    )?;

    if matches!(format, OutputFormat::Text) {
        print!("{}", render_search_results_text(&results));
        return Ok(());
    }

    let generated_at = generated_at_now()?;
    let next_cursor = if results.has_more() {
        results
            .hits()
            .last()
            .map(|hit| encode_search_cursor(&args.query, args.mode, &filters, results.mode_used(), hit))
            .transpose()?
    } else {
        None
    };
    let envelope = ListEnvelope {
        schema_version: OUTPUT_SCHEMA_VERSION,
        command: "search",
        generated_at,
        applied_filters: merge_applied_filters(&filters, json!({
            "query": args.query,
            "requested_mode": args.mode.as_str(),
            "mode_used": results.mode_used().as_str(),
            "limit": args.limit,
            "cursor": args.cursor,
        })),
        truncated: results.has_more(),
        next_cursor,
        results: results
            .hits()
            .iter()
            .map(|hit| ListRow::from_hit(hit, true))
            .collect(),
    };
    emit_list_output(format, &envelope)
}

fn recent(db_path: &Path, args: &RecentArgs) -> Result<()> {
    let format = args.output.resolved()?;
    let filters = normalize_retrieval_filters(&args.filters)?;
    let db = open_existing_db(db_path)?;
    let cursor = args
        .cursor
        .as_deref()
        .map(|value| parse_recent_cursor(value, &filters))
        .transpose()?;
    let hits = anyhow::Context::with_context(
        db.recent_page(args.limit, &filters, cursor.as_ref()),
        || "recent query failed".to_string(),
    )?;

    if matches!(format, OutputFormat::Text) {
        print!("{}", render_hits_text(hits.items()));
        return Ok(());
    }

    let generated_at = generated_at_now()?;
    let next_cursor = if hits.has_more() {
        hits.items()
            .last()
            .map(|hit| encode_recent_cursor(&filters, hit))
            .transpose()?
    } else {
        None
    };
    let envelope = ListEnvelope {
        schema_version: OUTPUT_SCHEMA_VERSION,
        command: "recent",
        generated_at,
        applied_filters: merge_applied_filters(&filters, json!({
            "limit": args.limit,
            "cursor": args.cursor,
        })),
        truncated: hits.has_more(),
        next_cursor,
        results: hits
            .items()
            .iter()
            .map(|hit| ListRow::from_hit(hit, false))
            .collect(),
    };
    emit_list_output(format, &envelope)
}

fn timeline(db_path: &Path, args: &TimelineArgs) -> Result<()> {
    let format = args.output.resolved()?;
    let filters = normalize_retrieval_filters(&args.filters)?;
    let db = open_existing_db(db_path)?;
    let cursor = args
        .cursor
        .as_deref()
        .map(|value| parse_timeline_cursor(value, &filters, args.sort))
        .transpose()?;
    let events = anyhow::Context::with_context(
        db.timeline_page(args.limit, &filters, args.sort, cursor.as_ref()),
        || "timeline query failed".to_string(),
    )?;

    if matches!(format, OutputFormat::Text) {
        print!("{}", render_timeline_text(events.items()));
        return Ok(());
    }

    let next_cursor = if events.has_more() {
        events
            .items()
            .last()
            .map(|event| encode_timeline_cursor(&filters, args.sort, event))
            .transpose()?
    } else {
        None
    };
    let envelope = ListEnvelope {
        schema_version: OUTPUT_SCHEMA_VERSION,
        command: "timeline",
        generated_at: generated_at_now()?,
        applied_filters: merge_applied_filters(&filters, json!({
            "limit": args.limit,
            "sort": args.sort.as_str(),
            "cursor": args.cursor,
        })),
        truncated: events.has_more(),
        next_cursor,
        results: events
            .items()
            .iter()
            .map(ListRow::from_timeline_event)
            .collect(),
    };
    emit_list_output(format, &envelope)
}

fn recall(db_path: &Path, args: &RecallArgs) -> Result<()> {
    let format = args.output.resolved();
    let filters = normalize_retrieval_filters(&args.filters)?;
    let db = open_existing_db(db_path)?;
    let recall = anyhow::Context::context(compute_recall(&db, args, &filters), "recall query failed")?;
    let generated_at = generated_at_now()?;
    let best_candidate = RecallOutputRow::from_hit(&recall.best.hit, args.full);
    let best_match_score = Some(recall.best.normalized_score);
    let confidence = RecallMatchConfidence::from_normalized_score(recall.best.normalized_score);
    let quoted_text = args
        .quote
        .then(|| best_candidate.best_text.clone())
        .filter(|text| !text.is_empty());
    let envelope = RecallEnvelope {
        schema_version: OUTPUT_SCHEMA_VERSION,
        command: "recall",
        generated_at,
        applied_filters: merge_applied_filters(&filters, json!({
            "limit": args.limit,
            "query_present": args.query.is_some(),
            "requested_mode": args.query.as_ref().map(|_| args.mode.as_str()),
            "mode_used": recall.search_mode_used.map(SearchMode::as_str),
            "full": args.full,
            "quote": args.quote,
            "min_score": args.min_score,
            "prefer_recent": args.prefer_recent,
            "prefer_app": args.prefer_app,
        })),
        query: args.query.clone(),
        best_candidate,
        alternatives: recall
            .alternatives
            .iter()
            .map(|candidate| RecallOutputRow::from_hit(&candidate.hit, false))
            .collect(),
        best_match_confidence: confidence,
        best_match_score,
        why_selected: recall.why_selected,
        quoted_text,
    };
    emit_recall_output(format, &envelope)
}

fn show_snapshot(db_path: &Path, args: &GetArgs) -> Result<()> {
    let format = args.output.resolved()?;
    let filters = normalize_retrieval_filters(&args.filters)?;
    let db = open_existing_db(db_path)?;
    if !db.snapshot_matches_filters(args.snapshot_id, &filters)? {
        return Err(anyhow!(
            "snapshot {} does not satisfy the active filters",
            args.snapshot_id
        ));
    }
    let snapshot =
        anyhow::Context::with_context(db.find_snapshot(args.snapshot_id, args.events), || {
            format!("get failed for snapshot {}", args.snapshot_id)
        })?
        .ok_or_else(|| anyhow!("snapshot {} was not found", args.snapshot_id))?;

    if matches!(format, OutputFormat::Text) {
        print!("{}", render_snapshot_text(&snapshot));
        return Ok(());
    }

    let envelope = GetEnvelope {
        schema_version: OUTPUT_SCHEMA_VERSION,
        command: "get",
        generated_at: generated_at_now()?,
        applied_filters: serde_json::to_value(&filters)?,
        snapshot,
    };
    emit_get_output(format, &envelope)
}

fn export_snapshot_bytes(db_path: &Path, args: &ExportArgs) -> Result<()> {
    let filters = normalize_retrieval_filters(&args.filters)?;
    let db = open_existing_db(db_path)?;
    if !db.snapshot_matches_filters(args.snapshot_id, &filters)? {
        return Err(anyhow!(
            "snapshot {} does not satisfy the active filters",
            args.snapshot_id
        ));
    }
    let representation = db
        .find_representation_bytes(args.snapshot_id, args.item, &args.uti)?
        .ok_or_else(|| {
            anyhow!(
                "representation not found for snapshot {} item {} uti {}",
                args.snapshot_id,
                args.item,
                args.uti
            )
        })?;

    if let Some(parent) = args.out.parent() {
        anyhow::Context::with_context(std::fs::create_dir_all(parent), || {
            format!("failed to create {}", parent.display())
        })?;
    }

    anyhow::Context::with_context(std::fs::write(&args.out, representation.raw_bytes()), || {
        format!("failed to write {}", args.out.display())
    })?;

    println!(
        "exported snapshot={} item={} uti={} bytes={} sha256={} out={}",
        args.snapshot_id,
        args.item,
        args.uti,
        representation.byte_len(),
        representation.raw_sha256(),
        args.out.display()
    );

    Ok(())
}

fn doctor(db_path: &Path, args: &DoctorArgs) -> Result<()> {
    let db = open_existing_db(db_path)?;
    let report = anyhow::Context::context(db.doctor(), "doctor diagnostics failed")?;
    emit_json_or_text(args.json, &report, render_doctor_text)?;

    Ok(())
}

fn open_existing_db(path: &Path) -> Result<Database> {
    anyhow::Context::with_context(Database::open_existing(path), || {
        format!("failed to open database at {}", path.display())
    })
}

fn open_or_init_db(path: &Path) -> Result<Database> {
    anyhow::Context::with_context(Database::open_or_init(path), || {
        format!("failed to open database at {}", path.display())
    })
}

fn query_search_results(
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

fn compute_recall(
    db: &Database,
    args: &RecallArgs,
    filters: &RetrievalFilters,
) -> Result<RecallComputation> {
    let query = args.query.as_deref().map(str::trim).filter(|query| !query.is_empty());
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
            .map_or(true, |candidate| candidate.normalized_score < threshold);

        for mut candidate in search_candidates {
            if search_was_weak {
                candidate.sort_score *= 0.72;
            }
            upsert_recall_candidate(&mut merged, candidate);
        }

        if search_was_weak {
            for (index, hit) in db.recent(args.limit, filters)?.into_iter().enumerate() {
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
        for (index, hit) in db.recent(args.limit, filters)?.into_iter().enumerate() {
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

    let mut ranked = merged.into_values().collect::<Vec<_>>();
    ranked.sort_by(compare_recall_candidates);

    let best = ranked
        .first()
        .cloned()
        .ok_or_else(|| anyhow!("no clipboard candidates matched the recall request"))?;
    let alternatives = ranked.into_iter().skip(1).take(args.limit).collect::<Vec<_>>();
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

fn run_search_query(
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

fn build_search_candidate(
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

fn build_recent_candidate(
    hit: SearchHit,
    index: usize,
    prefer_app: Option<&str>,
    prefer_recent: bool,
) -> RecallCandidate {
    let app_preferred = matches_preferred_app(&hit, prefer_app);
    let text_bonus = if !hit.preview_text().trim().is_empty() { 0.08 } else { 0.0 };
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

fn upsert_recall_candidate(store: &mut HashMap<i64, RecallCandidate>, candidate: RecallCandidate) {
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

fn compare_recall_candidates(left: &RecallCandidate, right: &RecallCandidate) -> Ordering {
    right
        .sort_score
        .partial_cmp(&left.sort_score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| right.hit.last_observed_at().cmp(left.hit.last_observed_at()))
        .then_with(|| right.hit.snapshot_id().cmp(&left.hit.snapshot_id()))
        .then_with(|| match (left.source, right.source) {
            (RecallCandidateSource::Search, RecallCandidateSource::Recent) => Ordering::Less,
            (RecallCandidateSource::Recent, RecallCandidateSource::Search) => Ordering::Greater,
            _ => Ordering::Equal,
        })
}

fn default_recall_threshold(mode_used: SearchMode) -> f64 {
    match mode_used {
        SearchMode::Fts => 0.68,
        SearchMode::Literal | SearchMode::Auto => 0.72,
    }
}

fn normalize_fts_score(score: Option<f64>) -> f64 {
    score
        .map(|value| 1.0 / (1.0 + value.max(0.0)))
        .unwrap_or(0.0)
}

fn literal_match_score(hit: &SearchHit, query: &str) -> f64 {
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

fn matches_preferred_app(hit: &SearchHit, prefer_app: Option<&str>) -> bool {
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

fn app_preference_boost(app_preferred: bool) -> f64 {
    if app_preferred {
        0.12
    } else {
        0.0
    }
}

fn recent_index_boost(index: usize) -> f64 {
    match index {
        0 => 0.22,
        1 => 0.18,
        2 => 0.14,
        3 => 0.1,
        _ => 0.06f64.max(0.12 - (index as f64 * 0.01)),
    }
}

fn search_rank_bonus(index: usize) -> f64 {
    match index {
        0 => 0.1,
        1 => 0.06,
        2 => 0.04,
        _ => 0.02,
    }
}

fn build_recall_why_selected(
    best: &RecallCandidate,
    query: Option<&str>,
    search_was_weak: bool,
    prefer_recent: bool,
    prefer_app: Option<&str>,
) -> String {
    let mut parts = Vec::new();

    match (query, best.source, search_was_weak) {
        (Some(query), RecallCandidateSource::Search, false) => {
            parts.push(format!("Selected the strongest search match for \"{query}\""));
        }
        (Some(query), RecallCandidateSource::Search, true) => {
            parts.push(format!(
                "Selected the best available query match for \"{query}\" after weak search results were merged with recent candidates"
            ));
        }
        (Some(_query), RecallCandidateSource::Recent, true) => {
            parts.push("Fell back to recent clipboard items because query matches were weak".to_string());
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

fn parse_search_cursor(
    encoded: &str,
    query: &str,
    requested_mode: SearchMode,
    filters: &RetrievalFilters,
) -> Result<SearchCursorState> {
    let token: SearchCursorToken = decode_cursor(encoded)?;
    if token.command != "search" {
        return Err(anyhow!(
            "cursor is for command `{}` but was used with `search`",
            token.command
        ));
    }
    if token.query != query || token.requested_mode != requested_mode {
        return Err(anyhow!(
            "cursor does not match the active search query or mode"
        ));
    }
    if token.filters != *filters {
        return Err(anyhow!(
            "cursor does not match the active search filters"
        ));
    }
    if requested_mode != SearchMode::Auto && token.mode_used != requested_mode {
        return Err(anyhow!(
            "cursor mode `{}` does not match the requested search mode `{}`",
            token.mode_used.as_str(),
            requested_mode.as_str()
        ));
    }

    Ok(SearchCursorState::new(
        token.mode_used,
        token.score,
        token.last_seen_at,
        token.snapshot_id,
    ))
}

fn parse_recent_cursor(encoded: &str, filters: &RetrievalFilters) -> Result<RecentCursorState> {
    let token: RecentCursorToken = decode_cursor(encoded)?;
    if token.command != "recent" {
        return Err(anyhow!(
            "cursor is for command `{}` but was used with `recent`",
            token.command
        ));
    }
    if token.filters != *filters {
        return Err(anyhow!(
            "cursor does not match the active recent filters"
        ));
    }

    Ok(RecentCursorState::new(token.last_seen_at, token.snapshot_id))
}

fn parse_timeline_cursor(
    encoded: &str,
    filters: &RetrievalFilters,
    sort: TimelineSort,
) -> Result<TimelineCursorState> {
    let token: TimelineCursorToken = decode_cursor(encoded)?;
    if token.command != "timeline" {
        return Err(anyhow!(
            "cursor is for command `{}` but was used with `timeline`",
            token.command
        ));
    }
    if token.filters != *filters || token.sort != sort {
        return Err(anyhow!(
            "cursor does not match the active timeline filters or sort"
        ));
    }

    Ok(TimelineCursorState::new(token.observed_at, token.event_id))
}

fn encode_search_cursor(
    query: &str,
    requested_mode: SearchMode,
    filters: &RetrievalFilters,
    mode_used: SearchMode,
    hit: &crate::model::SearchHit,
) -> Result<String> {
    encode_cursor(&SearchCursorToken {
        command: "search".to_string(),
        query: query.to_string(),
        requested_mode,
        mode_used,
        filters: filters.clone(),
        last_seen_at: hit.last_observed_at().to_string(),
        snapshot_id: hit.snapshot_id(),
        score: hit.score(),
    })
}

fn encode_recent_cursor(filters: &RetrievalFilters, hit: &crate::model::SearchHit) -> Result<String> {
    encode_cursor(&RecentCursorToken {
        command: "recent".to_string(),
        filters: filters.clone(),
        last_seen_at: hit.last_observed_at().to_string(),
        snapshot_id: hit.snapshot_id(),
    })
}

fn encode_timeline_cursor(
    filters: &RetrievalFilters,
    sort: TimelineSort,
    event: &TimelineEvent,
) -> Result<String> {
    encode_cursor(&TimelineCursorToken {
        command: "timeline".to_string(),
        filters: filters.clone(),
        sort,
        observed_at: event.observed_at().to_string(),
        event_id: event.event_id(),
    })
}

fn encode_cursor<T: Serialize>(token: &T) -> Result<String> {
    let json = serde_json::to_vec(token)?;
    Ok(hex::encode(json))
}

fn decode_cursor<T: for<'de> Deserialize<'de>>(encoded: &str) -> Result<T> {
    let bytes = hex::decode(encoded).map_err(|error| anyhow!("invalid cursor encoding: {error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| anyhow!("invalid cursor payload: {error}"))
}

fn normalize_retrieval_filters(
    args: &super::RetrievalFilterArgs,
) -> Result<RetrievalFilters> {
    args.normalized()
        .map_err(|error| anyhow!(error.to_string()))
}

fn merge_applied_filters(filters: &RetrievalFilters, command_specific: serde_json::Value) -> serde_json::Value {
    let mut value = serde_json::to_value(filters).unwrap_or_else(|_| json!({}));
    if let (Some(base), Some(extra)) = (value.as_object_mut(), command_specific.as_object()) {
        for (key, value) in extra {
            base.insert(key.clone(), value.clone());
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::fs;
    use std::path::PathBuf;
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        encode_recent_cursor, encode_search_cursor, parse_recent_cursor, parse_search_cursor,
        query_search_results, run_watch_iteration_with_capture, SearchArgs, SearchMode,
        SearchResults, WatchArgs, WatchState,
    };
    use crate::db::{Database, RetrievalFilters};
    use crate::model::{build_item, build_representation, build_snapshot, CaptureContext};

    fn temp_db_path(test_name: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();

        std::env::temp_dir()
            .join("clipmem-main-tests")
            .join(format!("{test_name}-{}-{timestamp}.sqlite3", process::id()))
    }

    fn cleanup_db(path: &std::path::Path) {
        for suffix in ["", "-shm", "-wal"] {
            let candidate = if suffix.is_empty() {
                path.to_path_buf()
            } else {
                PathBuf::from(format!("{}{suffix}", path.display()))
            };
            let _ = fs::remove_file(candidate);
        }

        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    fn unfiltered() -> RetrievalFilters {
        RetrievalFilters::default()
    }

    #[test]
    fn search_query_dispatches_literal_mode_against_database() {
        let path = temp_db_path("search-literal");
        let mut db = Database::open_or_init(&path).expect("test database should open");
        let first = build_snapshot(
            CaptureContext::new(1)
                .with_frontmost_app_name("Editor")
                .with_frontmost_app_bundle_id("com.example.Editor"),
            vec![build_item(
                0,
                vec![build_representation(
                    "public.utf8-plain-text".to_string(),
                    None,
                    b"git status".to_vec(),
                )],
            )],
        );
        let second = build_snapshot(
            CaptureContext::new(2)
                .with_frontmost_app_name("Terminal")
                .with_frontmost_app_bundle_id("com.apple.Terminal"),
            vec![build_item(
                0,
                vec![build_representation(
                    "public.utf8-plain-text".to_string(),
                    None,
                    b"cargo test".to_vec(),
                )],
            )],
        );

        db.store_capture(&first).expect("seed should succeed");
        db.store_capture(&second).expect("seed should succeed");

        let results: SearchResults = query_search_results(
            &db,
            &SearchArgs {
                query: "git".to_string(),
                mode: SearchMode::Literal,
                limit: 10,
                cursor: None,
                filters: crate::cli::RetrievalFilterArgs {
                    since: None,
                    until: None,
                    hours: None,
                    app: None,
                    bundle_id: None,
                    kind: None,
                    has_text: false,
                    has_url: false,
                    has_file_url: false,
                    has_image: false,
                    has_pdf: false,
                    min_bytes: None,
                    max_bytes: None,
                },
                output: crate::cli::OutputArgs {
                    format: None,
                    json: false,
                },
            },
            &unfiltered(),
            None,
        )
        .expect("query should succeed");

        assert_eq!(results.mode_used(), SearchMode::Literal);
        assert_eq!(results.hits().len(), 1);
        assert_eq!(results.hits()[0].preview_text(), "git status");

        cleanup_db(&path);
    }

    #[test]
    fn search_cursor_round_trips_and_rejects_mismatches() {
        let hit = crate::model::SearchHit::new(
            9,
            12,
            "abc".to_string(),
            crate::model::SnapshotKind::PlainText,
            "git status".to_string(),
            Some("git status".to_string()),
            1,
            "2026-04-16T09:00:00Z".to_string(),
            "2026-04-16T10:00:00Z".to_string(),
            Some("Terminal".to_string()),
            Some("com.apple.Terminal".to_string()),
            Vec::new(),
            Vec::new(),
            10,
            1,
            Some(0.5),
        );
        let filters = unfiltered();
        let encoded =
            encode_search_cursor("git", SearchMode::Auto, &filters, SearchMode::Fts, &hit).unwrap();
        let cursor = parse_search_cursor(&encoded, "git", SearchMode::Auto, &filters).unwrap();

        assert_eq!(cursor.mode_used(), SearchMode::Fts);
        assert!(parse_search_cursor(&encoded, "cargo", SearchMode::Auto, &filters).is_err());
    }

    #[test]
    fn recent_cursor_round_trips_and_rejects_mismatches() {
        let hit = crate::model::SearchHit::new(
            9,
            12,
            "abc".to_string(),
            crate::model::SnapshotKind::PlainText,
            "git status".to_string(),
            None,
            1,
            "2026-04-16T09:00:00Z".to_string(),
            "2026-04-16T10:00:00Z".to_string(),
            Some("Terminal".to_string()),
            Some("com.apple.Terminal".to_string()),
            Vec::new(),
            Vec::new(),
            10,
            1,
            None,
        );
        let filters = RetrievalFilters::new(
            None,
            None,
            Some(24),
            None,
            None,
            None,
            false,
            false,
            false,
            false,
            false,
            None,
            None,
        );
        let encoded = encode_recent_cursor(&filters, &hit).unwrap();
        let cursor = parse_recent_cursor(&encoded, &filters).unwrap();

        assert_eq!(cursor.snapshot_id(), 9);
        assert!(parse_recent_cursor(&encoded, &RetrievalFilters::new(
            None,
            None,
            Some(12),
            None,
            None,
            None,
            false,
            false,
            false,
            false,
            false,
            None,
            None,
        ))
        .is_err());
    }

    #[test]
    fn watch_iteration_skips_initial_clipboard_state_once() {
        let path = temp_db_path("watch-skip-initial");
        let mut db = Database::open_or_init(&path).expect("test database should open");
        let args = WatchArgs {
            interval_ms: 350,
            quiet: true,
            skip_initial: true,
        };
        let mut state = WatchState::new();
        let capture_calls = Cell::new(0);

        let result = run_watch_iteration_with_capture(
            &mut db,
            &args,
            &mut state,
            || Ok(7),
            || {
                capture_calls.set(capture_calls.get() + 1);
                Ok(build_snapshot(
                    CaptureContext::new(7)
                        .with_frontmost_app_name("Terminal")
                        .with_frontmost_app_bundle_id("com.apple.Terminal"),
                    vec![build_item(
                        0,
                        vec![build_representation(
                            "public.utf8-plain-text".to_string(),
                            None,
                            b"git status".to_vec(),
                        )],
                    )],
                ))
            },
        );

        assert!(result.is_ok());
        assert_eq!(capture_calls.get(), 0);
        assert!(db.recent(10, &unfiltered()).unwrap().is_empty());

        cleanup_db(&path);
    }

    #[test]
    fn watch_iteration_captures_when_clipboard_changes() {
        let path = temp_db_path("watch-capture-change");
        let mut db = Database::open_or_init(&path).expect("test database should open");
        let args = WatchArgs {
            interval_ms: 350,
            quiet: true,
            skip_initial: false,
        };
        let mut state = WatchState::new();
        let capture_calls = Cell::new(0);

        let result = run_watch_iteration_with_capture(
            &mut db,
            &args,
            &mut state,
            || Ok(2),
            || {
                capture_calls.set(capture_calls.get() + 1);
                Ok(build_snapshot(
                    CaptureContext::new(2)
                        .with_frontmost_app_name("Terminal")
                        .with_frontmost_app_bundle_id("com.apple.Terminal"),
                    vec![build_item(
                        0,
                        vec![build_representation(
                            "public.utf8-plain-text".to_string(),
                            None,
                            b"git status".to_vec(),
                        )],
                    )],
                ))
            },
        );

        assert!(result.is_ok());
        assert_eq!(capture_calls.get(), 1);
        assert_eq!(db.recent(10, &unfiltered()).unwrap().len(), 1);

        cleanup_db(&path);
    }
}
