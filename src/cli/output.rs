use std::fmt::Write;

use anyhow::{anyhow, Result};
use serde::Serialize;
use serde_json::{json, Value};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::db::SearchResults;
use crate::model::{
    CaptureStoreResult, ClipboardSnapshot, DoctorReport, SearchHit, SnapshotDetails,
};

use super::OutputFormat;

pub(super) const OUTPUT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
pub(super) struct ListEnvelope {
    pub(super) schema_version: u32,
    pub(super) command: &'static str,
    pub(super) generated_at: String,
    pub(super) applied_filters: Value,
    pub(super) truncated: bool,
    pub(super) next_cursor: Option<String>,
    pub(super) results: Vec<ListRow>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct GetEnvelope {
    pub(super) schema_version: u32,
    pub(super) command: &'static str,
    pub(super) generated_at: String,
    pub(super) snapshot: SnapshotDetails,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RecallMatchConfidence {
    High,
    Medium,
    Low,
}

impl RecallMatchConfidence {
    #[must_use]
    pub(super) fn from_normalized_score(score: f64) -> Self {
        if score >= 0.8 {
            Self::High
        } else if score >= 0.6 {
            Self::Medium
        } else {
            Self::Low
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct RecallEnvelope {
    pub(super) schema_version: u32,
    pub(super) command: &'static str,
    pub(super) generated_at: String,
    pub(super) applied_filters: Value,
    pub(super) query: Option<String>,
    pub(super) best_candidate: RecallOutputRow,
    pub(super) alternatives: Vec<RecallOutputRow>,
    pub(super) best_match_confidence: RecallMatchConfidence,
    pub(super) best_match_score: Option<f64>,
    pub(super) why_selected: String,
    pub(super) quoted_text: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct ListRow {
    pub(super) snapshot_id: i64,
    pub(super) event_id: i64,
    pub(super) sha256: String,
    pub(super) kind: String,
    pub(super) observed_at: String,
    pub(super) first_seen_at: String,
    pub(super) last_seen_at: String,
    pub(super) app_name: Option<String>,
    pub(super) app_bundle_id: Option<String>,
    pub(super) best_text: String,
    pub(super) preview_text: String,
    pub(super) urls: Vec<String>,
    pub(super) file_paths: Vec<String>,
    pub(super) item_count: usize,
    pub(super) total_bytes: usize,
    pub(super) capture_count: usize,
    pub(super) score: Option<f64>,
    pub(super) why_matched: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct RecallOutputRow {
    pub(super) snapshot_id: i64,
    pub(super) event_id: i64,
    pub(super) sha256: String,
    pub(super) kind: String,
    pub(super) observed_at: String,
    pub(super) first_seen_at: String,
    pub(super) last_seen_at: String,
    pub(super) app_name: Option<String>,
    pub(super) app_bundle_id: Option<String>,
    pub(super) best_text: String,
    pub(super) preview_text: String,
    pub(super) urls: Vec<String>,
    pub(super) file_paths: Vec<String>,
    pub(super) item_count: usize,
    pub(super) total_bytes: usize,
    pub(super) capture_count: usize,
    pub(super) score: Option<f64>,
    pub(super) why_matched: Option<String>,
    pub(super) snippet: String,
}

impl ListRow {
    #[must_use]
    pub(super) fn from_hit(hit: &SearchHit, include_why_matched: bool) -> Self {
        let why_matched = include_why_matched.then(|| {
            hit.why_matched()
                .unwrap_or(hit.preview_text())
                .to_string()
        });
        let best_text = why_matched
            .clone()
            .filter(|value| value != hit.preview_text())
            .unwrap_or_else(|| hit.preview_text().to_string());

        Self {
            snapshot_id: hit.snapshot_id(),
            event_id: hit.event_id(),
            sha256: hit.sha256().to_string(),
            kind: hit.snapshot_kind().as_str().to_string(),
            observed_at: hit.last_observed_at().to_string(),
            first_seen_at: hit.first_observed_at().to_string(),
            last_seen_at: hit.last_observed_at().to_string(),
            app_name: hit.last_frontmost_app_name().map(ToOwned::to_owned),
            app_bundle_id: hit.last_frontmost_app_bundle_id().map(ToOwned::to_owned),
            best_text,
            preview_text: hit.preview_text().to_string(),
            urls: hit.urls().to_vec(),
            file_paths: hit.file_paths().to_vec(),
            item_count: hit.item_count(),
            total_bytes: hit.total_bytes(),
            capture_count: hit.capture_count(),
            score: hit.score(),
            why_matched,
        }
    }
}

impl RecallOutputRow {
    #[must_use]
    pub(super) fn from_hit(hit: &SearchHit, full: bool) -> Self {
        let why_matched = hit.why_matched().map(ToOwned::to_owned);
        let source_text = best_text_from_hit(hit);
        let best_text = if full {
            source_text.clone()
        } else {
            truncate_for_markdown(&source_text, 320)
        };
        let snippet = truncate_for_markdown(&source_text, 140);

        Self {
            snapshot_id: hit.snapshot_id(),
            event_id: hit.event_id(),
            sha256: hit.sha256().to_string(),
            kind: hit.snapshot_kind().as_str().to_string(),
            observed_at: hit.last_observed_at().to_string(),
            first_seen_at: hit.first_observed_at().to_string(),
            last_seen_at: hit.last_observed_at().to_string(),
            app_name: hit.last_frontmost_app_name().map(ToOwned::to_owned),
            app_bundle_id: hit.last_frontmost_app_bundle_id().map(ToOwned::to_owned),
            best_text,
            preview_text: hit.preview_text().to_string(),
            urls: hit.urls().to_vec(),
            file_paths: hit.file_paths().to_vec(),
            item_count: hit.item_count(),
            total_bytes: hit.total_bytes(),
            capture_count: hit.capture_count(),
            score: hit.score(),
            why_matched,
            snippet,
        }
    }
}

pub(super) fn generated_at_now() -> Result<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| anyhow!("format generated timestamp: {error}"))
}

pub(super) fn emit_json_or_text<T>(
    json: bool,
    value: &T,
    render_text: impl FnOnce(&T) -> String,
) -> Result<()>
where
    T: Serialize,
{
    if json {
        print_json(value)
    } else {
        print!("{}", render_text(value));
        Ok(())
    }
}

pub(super) fn emit_list_output(format: OutputFormat, envelope: &ListEnvelope) -> Result<()> {
    match format {
        OutputFormat::Text => unreachable!("list text output uses the legacy text renderer"),
        OutputFormat::Json => print_json(envelope),
        OutputFormat::Jsonl => print_jsonl_list(envelope),
        OutputFormat::Md => {
            print!("{}", render_list_markdown(envelope));
            Ok(())
        }
        OutputFormat::Toon => {
            print!("{}", render_list_toon(envelope));
            Ok(())
        }
    }
}

pub(super) fn emit_get_output(format: OutputFormat, envelope: &GetEnvelope) -> Result<()> {
    match format {
        OutputFormat::Text => unreachable!("get text output uses the legacy text renderer"),
        OutputFormat::Json => print_json(envelope),
        OutputFormat::Jsonl => {
            println!("{}", serde_json::to_string(envelope)?);
            Ok(())
        }
        OutputFormat::Md => {
            print!("{}", render_get_markdown(envelope));
            Ok(())
        }
        OutputFormat::Toon => Err(anyhow!(
            "`clipmem get --format toon` is not supported because TOON is only available for flattened list output"
        )),
    }
}

pub(super) fn emit_recall_output(
    format: super::RecallOutputFormat,
    envelope: &RecallEnvelope,
) -> Result<()> {
    match format {
        super::RecallOutputFormat::Json => print_json(envelope),
        super::RecallOutputFormat::Md => {
            print!("{}", render_recall_markdown(envelope));
            Ok(())
        }
        super::RecallOutputFormat::Toon => {
            print!("{}", render_recall_toon(envelope));
            Ok(())
        }
    }
}

pub(super) fn render_capture_once_text(
    store: &CaptureStoreResult,
    snapshot: &ClipboardSnapshot,
) -> String {
    let mut out = String::new();
    let store_state = if store.inserted_new_snapshot() {
        "new content"
    } else {
        "already known content"
    };
    let _ = writeln!(
        out,
        "stored snapshot {} via event {} ({store_state})",
        store.snapshot_id(),
        store.event_id()
    );
    let _ = writeln!(out, "kind: {}", snapshot.snapshot_kind());
    let _ = writeln!(out, "bytes: {}", snapshot.total_bytes());
    if let Some(app) = snapshot.frontmost_app_name() {
        let _ = writeln!(out, "frontmost app: {app}");
    }
    if !snapshot.preview_text().is_empty() {
        let _ = writeln!(out, "preview: {}", snapshot.preview_text());
    }
    out
}

pub(super) fn render_hits_text(hits: &[SearchHit]) -> String {
    let mut out = String::new();
    for hit in hits {
        let app = hit
            .last_frontmost_app_name()
            .or(hit.last_frontmost_app_bundle_id())
            .unwrap_or("unknown app");

        let _ = writeln!(
            out,
            "[{}] {} · last seen {} · captures {} · {} · {} bytes · {} items",
            hit.snapshot_id(),
            hit.snapshot_kind(),
            format_utc_timestamp(hit.last_observed_at()),
            hit.capture_count(),
            app,
            hit.total_bytes(),
            hit.item_count()
        );

        if !hit.preview_text().is_empty() {
            let _ = writeln!(out, "  preview: {}", hit.preview_text());
        }
        if let Some(why_matched) = hit.why_matched().filter(|value| *value != hit.preview_text()) {
            let _ = writeln!(out, "  match:   {why_matched}");
        }
        if !hit.urls().is_empty() {
            let _ = writeln!(out, "  urls:    {}", hit.urls().join(", "));
        }
        if !hit.file_paths().is_empty() {
            let _ = writeln!(out, "  files:   {}", hit.file_paths().join(", "));
        }
        if let Some(score) = hit.score() {
            let _ = writeln!(out, "  score:   {score:.3}");
        }
        push_blank_line(&mut out);
    }
    out
}

pub(super) fn render_search_results_text(results: &SearchResults) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "search mode: {}", results.mode_used().as_str());
    if !results.hits().is_empty() {
        out.push('\n');
    }
    out.push_str(&render_hits_text(results.hits()));
    out
}

pub(super) fn render_snapshot_text(snapshot: &SnapshotDetails) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "snapshot {} · sha256={} · kind={} · captures={}",
        snapshot.snapshot_id(),
        snapshot.sha256(),
        snapshot.snapshot_kind(),
        snapshot.capture_count()
    );
    let _ = writeln!(
        out,
        "first seen: {} · last seen: {}",
        format_utc_timestamp(snapshot.first_observed_at()),
        format_utc_timestamp(snapshot.last_observed_at())
    );
    let _ = writeln!(
        out,
        "bytes: {} · items: {}",
        snapshot.total_bytes(),
        snapshot.item_count()
    );
    if let Some(app) = snapshot
        .last_frontmost_app_name()
        .or(snapshot.last_frontmost_app_bundle_id())
    {
        let _ = writeln!(out, "last frontmost app: {app}");
    }
    if !snapshot.preview_text().is_empty() {
        let _ = writeln!(out, "preview: {}", snapshot.preview_text());
    }
    push_blank_line(&mut out);

    for item in snapshot.items() {
        let _ = writeln!(
            out,
            "item {} · kind={} · bytes={}{}",
            item.item_index(),
            item.primary_kind(),
            item.total_bytes(),
            item.primary_uti()
                .map(|uti| format!(" · primary type={uti}"))
                .unwrap_or_default()
        );
        if !item.preview_text().is_empty() {
            let _ = writeln!(out, "  preview: {}", item.preview_text());
        }
        for rep in item.representations() {
            let _ = writeln!(
                out,
                "  - {} · kind={} · {} bytes · sha256={}",
                rep.uti(),
                rep.kind(),
                rep.byte_len(),
                rep.raw_sha256()
            );
            if let Some(text) = rep.text_value() {
                let _ = writeln!(out, "    text: {text}");
            }
        }
        push_blank_line(&mut out);
    }

    if !snapshot.recent_events().is_empty() {
        let _ = writeln!(out, "recent events:");
        for event in snapshot.recent_events() {
            let source = event
                .frontmost_app_name()
                .or(event.frontmost_app_bundle_id())
                .unwrap_or("unknown app");
            let _ = writeln!(
                out,
                "  event {} · {} · change_count={} · {}",
                event.event_id(),
                format_utc_timestamp(event.observed_at()),
                event.change_count(),
                source
            );
        }
    }

    out
}

pub(super) fn render_doctor_text(report: &DoctorReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "database: {}", report.db_path());
    let _ = writeln!(out, "sqlite version: {}", report.sqlite_version());
    let _ = writeln!(out, "journal mode: {}", report.journal_mode());
    let _ = writeln!(
        out,
        "fts5 compile option present: {}",
        report.fts5_compile_option_present()
    );
    let _ = writeln!(
        out,
        "fts5 temp table creation works: {}",
        report.fts5_create_virtual_table_ok()
    );

    if !report.compile_options().is_empty() {
        out.push('\n');
        let _ = writeln!(out, "compile options:");
        for opt in report.compile_options() {
            let _ = writeln!(out, "  {opt}");
        }
    }

    out
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(value)?;
    println!("{json}");
    Ok(())
}

fn print_jsonl_list(envelope: &ListEnvelope) -> Result<()> {
    let meta = json!({
        "type": "meta",
        "schema_version": envelope.schema_version,
        "command": envelope.command,
        "generated_at": envelope.generated_at,
        "applied_filters": envelope.applied_filters,
        "truncated": envelope.truncated,
        "next_cursor": envelope.next_cursor,
    });
    println!("{}", serde_json::to_string(&meta)?);

    for row in &envelope.results {
        let mut line = serde_json::to_value(row)?;
        let object = line
            .as_object_mut()
            .ok_or_else(|| anyhow!("list row JSONL serialization did not produce an object"))?;
        object.insert("type".to_string(), Value::String("result".to_string()));
        println!("{}", serde_json::to_string(&line)?);
    }

    Ok(())
}

fn render_list_markdown(envelope: &ListEnvelope) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# clipmem {}", envelope.command);
    let _ = writeln!(out);
    let _ = writeln!(out, "- Generated at: {}", envelope.generated_at);
    let _ = writeln!(out, "- Filters: {}", render_filter_pairs(&envelope.applied_filters));
    let _ = writeln!(out, "- Truncated: {}", envelope.truncated);
    let _ = writeln!(
        out,
        "- Next cursor: {}",
        envelope.next_cursor.as_deref().unwrap_or("null")
    );
    let _ = writeln!(out);

    if envelope.results.is_empty() {
        let _ = writeln!(out, "No results.");
        return out;
    }

    let _ = writeln!(
        out,
        "| snapshot_id | kind | observed_at | app | best_text | score |"
    );
    let _ = writeln!(out, "| --- | --- | --- | --- | --- | --- |");
    for row in &envelope.results {
        let app = row
            .app_name
            .as_deref()
            .or(row.app_bundle_id.as_deref())
            .unwrap_or("unknown app");
        let best_text = truncate_for_markdown(&row.best_text, 100);
        let score = row
            .score
            .map(|value| format!("{value:.3}"))
            .unwrap_or_else(|| "null".to_string());
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} |",
            row.snapshot_id,
            escape_markdown_cell(&row.kind),
            escape_markdown_cell(&row.observed_at),
            escape_markdown_cell(app),
            escape_markdown_cell(&best_text),
            score
        );
    }

    out
}

fn render_get_markdown(envelope: &GetEnvelope) -> String {
    let snapshot = &envelope.snapshot;
    let mut out = String::new();
    let _ = writeln!(out, "# clipmem get");
    let _ = writeln!(out);
    let _ = writeln!(out, "- Generated at: {}", envelope.generated_at);
    let _ = writeln!(out, "- Snapshot: {}", snapshot.snapshot_id());
    let _ = writeln!(out, "- Kind: {}", snapshot.snapshot_kind());
    let _ = writeln!(out, "- First seen: {}", snapshot.first_observed_at());
    let _ = writeln!(out, "- Last seen: {}", snapshot.last_observed_at());
    let _ = writeln!(out, "- Captures: {}", snapshot.capture_count());
    let _ = writeln!(out, "- Bytes: {}", snapshot.total_bytes());
    let _ = writeln!(out, "- Items: {}", snapshot.item_count());
    if !snapshot.preview_text().is_empty() {
        let _ = writeln!(
            out,
            "- Preview: {}",
            escape_markdown_cell(&truncate_for_markdown(snapshot.preview_text(), 160))
        );
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "## Items");
    if snapshot.items().is_empty() {
        let _ = writeln!(out, "No items.");
    } else {
        for item in snapshot.items() {
            let _ = writeln!(
                out,
                "- item={} kind={} bytes={} preview={}",
                item.item_index(),
                item.primary_kind(),
                item.total_bytes(),
                escape_markdown_cell(&truncate_for_markdown(item.preview_text(), 120))
            );
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "## Recent Events");
    if snapshot.recent_events().is_empty() {
        let _ = writeln!(out, "No recent events.");
    } else {
        for event in snapshot.recent_events() {
            let source = event
                .frontmost_app_name()
                .or(event.frontmost_app_bundle_id())
                .unwrap_or("unknown app");
            let _ = writeln!(
                out,
                "- event={} observed_at={} change_count={} source={}",
                event.event_id(),
                event.observed_at(),
                event.change_count(),
                escape_markdown_cell(source)
            );
        }
    }

    out
}

fn render_recall_markdown(envelope: &RecallEnvelope) -> String {
    let best = &envelope.best_candidate;
    let mut out = String::new();
    let _ = writeln!(out, "# Best Match");
    let _ = writeln!(out);

    if let Some(quoted_text) = &envelope.quoted_text {
        for line in quoted_text.lines() {
            let _ = writeln!(out, "> {line}");
        }
    } else if !best.best_text.is_empty() {
        let _ = writeln!(out, "{}", best.best_text);
    } else {
        let _ = writeln!(out, "No direct text was recovered for this clipboard item.");
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "**Why This Match**");
    let _ = writeln!(out, "{}", envelope.why_selected);
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Provenance: snapshot {} · observed {} · {} · confidence {}{}",
        best.snapshot_id,
        best.observed_at,
        best.app_name
            .as_deref()
            .or(best.app_bundle_id.as_deref())
            .unwrap_or("unknown app"),
        render_confidence_label(&envelope.best_match_confidence),
        envelope
            .best_match_score
            .map(|score| format!(" · score {:.3}", score))
            .unwrap_or_default()
    );
    if !best.urls.is_empty() {
        let _ = writeln!(out, "URLs: {}", best.urls.join(", "));
    }
    if !best.file_paths.is_empty() {
        let _ = writeln!(out, "Files: {}", best.file_paths.join(", "));
    }

    if !envelope.alternatives.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "## Alternatives");
        for alternative in envelope.alternatives.iter().take(5) {
            let app = alternative
                .app_name
                .as_deref()
                .or(alternative.app_bundle_id.as_deref())
                .unwrap_or("unknown app");
            let _ = writeln!(
                out,
                "- snapshot {} · {} · {} · {}",
                alternative.snapshot_id,
                alternative.observed_at,
                app,
                escape_markdown_cell(&alternative.snippet)
            );
        }
    }

    out
}

fn render_list_toon(envelope: &ListEnvelope) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "schema_version: {}", envelope.schema_version);
    let _ = writeln!(out, "command: {}", encode_toon_scalar(&Value::String(envelope.command.to_string())));
    let _ = writeln!(
        out,
        "generated_at: {}",
        encode_toon_scalar(&Value::String(envelope.generated_at.clone()))
    );
    let _ = writeln!(out, "applied_filters:");
    render_toon_object(&mut out, &envelope.applied_filters, 2);
    let _ = writeln!(
        out,
        "truncated: {}",
        encode_toon_scalar(&Value::Bool(envelope.truncated))
    );
    let _ = writeln!(
        out,
        "next_cursor: {}",
        encode_toon_scalar(
            &envelope
                .next_cursor
                .as_ref()
                .map(|value| Value::String(value.clone()))
                .unwrap_or(Value::Null),
        )
    );

    let fields = [
        "snapshot_id",
        "event_id",
        "sha256",
        "kind",
        "observed_at",
        "first_seen_at",
        "last_seen_at",
        "app_name",
        "app_bundle_id",
        "best_text",
        "preview_text",
        "urls",
        "file_paths",
        "item_count",
        "total_bytes",
        "capture_count",
        "score",
        "why_matched",
    ];
    let _ = writeln!(
        out,
        "results[#{}\t]{{{}}}:",
        envelope.results.len(),
        fields.join("\t")
    );

    for row in &envelope.results {
        let values = vec![
            Value::from(row.snapshot_id),
            Value::from(row.event_id),
            Value::String(row.sha256.clone()),
            Value::String(row.kind.clone()),
            Value::String(row.observed_at.clone()),
            Value::String(row.first_seen_at.clone()),
            Value::String(row.last_seen_at.clone()),
            row.app_name
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
            row.app_bundle_id
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
            Value::String(row.best_text.clone()),
            Value::String(row.preview_text.clone()),
            Value::String(serde_json::to_string(&row.urls).unwrap_or_else(|_| "[]".to_string())),
            Value::String(
                serde_json::to_string(&row.file_paths).unwrap_or_else(|_| "[]".to_string()),
            ),
            Value::from(row.item_count as u64),
            Value::from(row.total_bytes as u64),
            Value::from(row.capture_count as u64),
            row.score.map(Value::from).unwrap_or(Value::Null),
            row.why_matched
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        ];
        let encoded = values
            .iter()
            .map(encode_toon_scalar)
            .collect::<Vec<_>>()
            .join("\t");
        let _ = writeln!(out, "  {encoded}");
    }

    out
}

fn render_recall_toon(envelope: &RecallEnvelope) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "schema_version: {}", envelope.schema_version);
    let _ = writeln!(out, "command: recall");
    let _ = writeln!(
        out,
        "generated_at: {}",
        encode_toon_scalar(&Value::String(envelope.generated_at.clone()))
    );
    let _ = writeln!(
        out,
        "query: {}",
        encode_toon_scalar(
            &envelope
                .query
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        )
    );
    let _ = writeln!(
        out,
        "best_match_confidence: {}",
        encode_toon_scalar(&serde_json::to_value(&envelope.best_match_confidence).unwrap_or(Value::Null))
    );
    let _ = writeln!(
        out,
        "best_match_score: {}",
        encode_toon_scalar(
            &envelope
                .best_match_score
                .map(Value::from)
                .unwrap_or(Value::Null),
        )
    );
    let _ = writeln!(
        out,
        "why_selected: {}",
        encode_toon_scalar(&Value::String(envelope.why_selected.clone()))
    );
    if let Some(quoted_text) = &envelope.quoted_text {
        let _ = writeln!(
            out,
            "quoted_text: {}",
            encode_toon_scalar(&Value::String(quoted_text.clone()))
        );
    }
    let _ = writeln!(out, "applied_filters:");
    render_toon_object(&mut out, &envelope.applied_filters, 2);

    render_recall_rows_toon(&mut out, "best_candidate", std::slice::from_ref(&envelope.best_candidate));
    render_recall_rows_toon(&mut out, "alternatives", &envelope.alternatives);

    out
}

fn render_recall_rows_toon(out: &mut String, key: &str, rows: &[RecallOutputRow]) {
    let fields = [
        "snapshot_id",
        "event_id",
        "sha256",
        "kind",
        "observed_at",
        "first_seen_at",
        "last_seen_at",
        "app_name",
        "app_bundle_id",
        "best_text",
        "preview_text",
        "urls",
        "file_paths",
        "item_count",
        "total_bytes",
        "capture_count",
        "score",
        "why_matched",
        "snippet",
    ];
    let _ = writeln!(out, "{key}[#{}\t]{{{}}}:", rows.len(), fields.join("\t"));
    for row in rows {
        let values = vec![
            Value::from(row.snapshot_id),
            Value::from(row.event_id),
            Value::String(row.sha256.clone()),
            Value::String(row.kind.clone()),
            Value::String(row.observed_at.clone()),
            Value::String(row.first_seen_at.clone()),
            Value::String(row.last_seen_at.clone()),
            row.app_name.clone().map(Value::String).unwrap_or(Value::Null),
            row.app_bundle_id
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
            Value::String(row.best_text.clone()),
            Value::String(row.preview_text.clone()),
            Value::String(serde_json::to_string(&row.urls).unwrap_or_else(|_| "[]".to_string())),
            Value::String(
                serde_json::to_string(&row.file_paths).unwrap_or_else(|_| "[]".to_string()),
            ),
            Value::from(row.item_count as u64),
            Value::from(row.total_bytes as u64),
            Value::from(row.capture_count as u64),
            row.score.map(Value::from).unwrap_or(Value::Null),
            row.why_matched
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
            Value::String(row.snippet.clone()),
        ];
        let encoded = values
            .iter()
            .map(encode_toon_scalar)
            .collect::<Vec<_>>()
            .join("\t");
        let _ = writeln!(out, "  {encoded}");
    }
}

fn render_toon_object(out: &mut String, value: &Value, indent: usize) {
    let Some(object) = value.as_object() else {
        return;
    };

    for (key, value) in object {
        let padding = " ".repeat(indent);
        match value {
            Value::Object(_) => {
                let _ = writeln!(out, "{padding}{key}:");
                render_toon_object(out, value, indent + 2);
            }
            other => {
                let _ = writeln!(out, "{padding}{key}: {}", encode_toon_scalar(other));
            }
        }
    }
}

fn encode_toon_scalar(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => encode_toon_string(text),
        Value::Array(_) | Value::Object(_) => encode_toon_string(
            &serde_json::to_string(value).unwrap_or_else(|_| "null".to_string()),
        ),
    }
}

fn encode_toon_string(text: &str) -> String {
    if text.is_empty()
        || text.contains('\t')
        || text.contains('\n')
        || text.contains('\r')
        || text.contains(':')
        || text.contains('"')
        || text.contains('\\')
        || text.starts_with(' ')
        || text.ends_with(' ')
    {
        let escaped = text
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t");
        format!("\"{escaped}\"")
    } else {
        text.to_string()
    }
}

fn render_filter_pairs(filters: &Value) -> String {
    let Some(object) = filters.as_object() else {
        return "none".to_string();
    };

    object
        .iter()
        .map(|(key, value)| {
            let rendered = match value {
                Value::Null => "null".to_string(),
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            format!("{key}={rendered}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_confidence_label(confidence: &RecallMatchConfidence) -> &'static str {
    match confidence {
        RecallMatchConfidence::High => "high",
        RecallMatchConfidence::Medium => "medium",
        RecallMatchConfidence::Low => "low",
    }
}

fn best_text_from_hit(hit: &SearchHit) -> String {
    hit.preview_text()
        .trim()
        .is_empty()
        .then(|| {
            hit.why_matched()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(hit.preview_text())
        })
        .unwrap_or(hit.preview_text())
        .to_string()
}

fn escape_markdown_cell(value: &str) -> String {
    value.replace('|', "\\|")
}

fn truncate_for_markdown(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        value.to_string()
    } else {
        value.chars().take(limit.saturating_sub(1)).collect::<String>() + "…"
    }
}

fn push_blank_line(out: &mut String) {
    out.push('\n');
}

fn format_utc_timestamp(timestamp: &str) -> String {
    if timestamp.ends_with('Z')
        || timestamp.ends_with(" UTC")
        || timestamp.contains('+')
        || timestamp.rfind('-').is_some_and(|idx| idx > 9)
    {
        timestamp.to_string()
    } else {
        format!("{timestamp} UTC")
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::db::{SearchMode, SearchResults};
    use crate::model::{
        build_item, build_representation, build_snapshot, CaptureContext, CaptureEvent,
        CaptureStoreResult, DoctorReport, SearchHit, SnapshotDetails, SnapshotKind,
    };

    use super::{
        render_capture_once_text, render_doctor_text, render_get_markdown, render_hits_text,
        render_list_markdown, render_list_toon, render_recall_markdown, render_recall_toon,
        render_search_results_text, render_snapshot_text, GetEnvelope, ListEnvelope, ListRow,
        RecallEnvelope, RecallMatchConfidence, RecallOutputRow, OUTPUT_SCHEMA_VERSION,
    };

    #[test]
    fn render_capture_once_text_reports_summary_lines() {
        let text = render_capture_once_text(
            &CaptureStoreResult::new(9, 12, true),
            &build_snapshot(
                CaptureContext::new(3)
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
            ),
        );

        assert!(text.contains("stored snapshot 9 via event 12"));
        assert!(text.contains("kind: plain_text"));
        assert!(text.contains("preview: git status"));
    }

    #[test]
    fn render_hits_text_includes_match_score_urls_and_files() {
        let text = render_hits_text(&[SearchHit::new(
            42,
            77,
            "abc123".to_string(),
            SnapshotKind::PlainText,
            "git clone".to_string(),
            Some("⟦git⟧ clone".to_string()),
            3,
            "2026-04-16 10:00:00".to_string(),
            "2026-04-16 11:00:00".to_string(),
            Some("Terminal".to_string()),
            Some("com.apple.Terminal".to_string()),
            vec!["https://example.com".to_string()],
            vec!["/Users/test/repo".to_string()],
            9,
            1,
            Some(0.125),
        )]);

        assert!(text.contains("[42] plain_text"));
        assert!(text.contains("match:   ⟦git⟧ clone"));
        assert!(text.contains("urls:    https://example.com"));
        assert!(text.contains("files:   /Users/test/repo"));
        assert!(text.contains("score:   0.125"));
    }

    #[test]
    fn render_snapshot_text_lists_items_and_events() {
        let text = render_snapshot_text(&SnapshotDetails::new(
            7,
            "abc123".to_string(),
            SnapshotKind::PlainText,
            "git push".to_string(),
            "git push".to_string(),
            1,
            8,
            "2026-04-16 10:00:00".to_string(),
            2,
            "2026-04-16 10:00:00".to_string(),
            "2026-04-16 11:00:00".to_string(),
            Some("Terminal".to_string()),
            Some("com.apple.Terminal".to_string()),
            vec![CaptureEvent::new(
                21,
                "2026-04-16 11:00:00".to_string(),
                3,
                Some("Terminal".to_string()),
                Some("com.apple.Terminal".to_string()),
            )],
            vec![build_item(
                0,
                vec![build_representation(
                    "public.utf8-plain-text".to_string(),
                    None,
                    b"git push".to_vec(),
                )],
            )],
        ));

        assert!(text.contains("snapshot 7"));
        assert!(text.contains("item 0 · kind=plain_text"));
        assert!(text.contains("event 21"));
    }

    #[test]
    fn render_doctor_text_lists_compile_options() {
        let text = render_doctor_text(&DoctorReport::new(
            "/tmp/clipmem.sqlite3".to_string(),
            "3.46.0".to_string(),
            "wal".to_string(),
            true,
            true,
            vec!["ENABLE_FTS5".to_string()],
        ));

        assert!(text.contains("database: /tmp/clipmem.sqlite3"));
        assert!(text.contains("compile options:"));
        assert!(text.contains("ENABLE_FTS5"));
    }

    #[test]
    fn render_search_results_text_reports_effective_mode() {
        let text = render_search_results_text(&SearchResults::new(
            SearchMode::Literal,
            vec![SearchHit::new(
                7,
                21,
                "abc123".to_string(),
                SnapshotKind::PlainText,
                "git push".to_string(),
                Some("git push".to_string()),
                2,
                "2026-04-16 10:00:00".to_string(),
                "2026-04-16 11:00:00".to_string(),
                Some("Terminal".to_string()),
                Some("com.apple.Terminal".to_string()),
                Vec::new(),
                Vec::new(),
                8,
                1,
                None,
            )],
            false,
        ));

        assert!(text.contains("search mode: literal"));
    }

    #[test]
    fn markdown_and_toon_render_envelopes() {
        let row = ListRow {
            snapshot_id: 1,
            event_id: 2,
            sha256: "abc".to_string(),
            kind: "plain_text".to_string(),
            observed_at: "2026-04-17T10:00:00Z".to_string(),
            first_seen_at: "2026-04-17T09:00:00Z".to_string(),
            last_seen_at: "2026-04-17T10:00:00Z".to_string(),
            app_name: Some("Terminal".to_string()),
            app_bundle_id: Some("com.apple.Terminal".to_string()),
            best_text: "git status".to_string(),
            preview_text: "git status".to_string(),
            urls: vec!["https://example.com".to_string()],
            file_paths: Vec::new(),
            item_count: 1,
            total_bytes: 10,
            capture_count: 1,
            score: Some(0.3),
            why_matched: Some("git status".to_string()),
        };
        let envelope = ListEnvelope {
            schema_version: OUTPUT_SCHEMA_VERSION,
            command: "search",
            generated_at: "2026-04-17T10:00:00Z".to_string(),
            applied_filters: json!({
                "query": "git",
                "requested_mode": "auto",
                "mode_used": "literal",
                "limit": 10,
                "cursor": null,
            }),
            truncated: false,
            next_cursor: None,
            results: vec![row],
        };
        let markdown = render_list_markdown(&envelope);
        let toon = render_list_toon(&envelope);

        assert!(markdown.contains("| snapshot_id | kind | observed_at |"));
        assert!(toon.contains("results[#1\t]{snapshot_id"));
        assert!(toon.contains("git status"));
    }

    #[test]
    fn get_markdown_renders_summary_items_and_events() {
        let markdown = render_get_markdown(&GetEnvelope {
            schema_version: OUTPUT_SCHEMA_VERSION,
            command: "get",
            generated_at: "2026-04-17T10:00:00Z".to_string(),
            snapshot: SnapshotDetails::new(
                7,
                "abc123".to_string(),
                SnapshotKind::PlainText,
                "git push".to_string(),
                "git push".to_string(),
                1,
                8,
                "2026-04-16 10:00:00".to_string(),
                2,
                "2026-04-16 10:00:00".to_string(),
                "2026-04-16 11:00:00".to_string(),
                Some("Terminal".to_string()),
                Some("com.apple.Terminal".to_string()),
                vec![CaptureEvent::new(
                    21,
                    "2026-04-16 11:00:00".to_string(),
                    3,
                    Some("Terminal".to_string()),
                    Some("com.apple.Terminal".to_string()),
                )],
                vec![build_item(
                    0,
                    vec![build_representation(
                        "public.utf8-plain-text".to_string(),
                        None,
                        b"git push".to_vec(),
                    )],
                )],
            ),
        });

        assert!(markdown.contains("## Items"));
        assert!(markdown.contains("## Recent Events"));
        assert!(markdown.contains("Snapshot: 7"));
    }

    #[test]
    fn recall_markdown_and_toon_render_best_match_and_alternatives() {
        let envelope = RecallEnvelope {
            schema_version: OUTPUT_SCHEMA_VERSION,
            command: "recall",
            generated_at: "2026-04-17T10:00:00Z".to_string(),
            applied_filters: json!({
                "limit": 5,
                "prefer_recent": true,
            }),
            query: Some("git".to_string()),
            best_candidate: RecallOutputRow {
                snapshot_id: 1,
                event_id: 2,
                sha256: "abc".to_string(),
                kind: "plain_text".to_string(),
                observed_at: "2026-04-17T10:00:00Z".to_string(),
                first_seen_at: "2026-04-17T09:00:00Z".to_string(),
                last_seen_at: "2026-04-17T10:00:00Z".to_string(),
                app_name: Some("Terminal".to_string()),
                app_bundle_id: Some("com.apple.Terminal".to_string()),
                best_text: "git status".to_string(),
                preview_text: "git status".to_string(),
                urls: vec!["https://example.com".to_string()],
                file_paths: vec!["/tmp/file.txt".to_string()],
                item_count: 1,
                total_bytes: 10,
                capture_count: 1,
                score: Some(0.1),
                why_matched: Some("git status".to_string()),
                snippet: "git status".to_string(),
            },
            alternatives: vec![RecallOutputRow {
                snapshot_id: 3,
                event_id: 4,
                sha256: "def".to_string(),
                kind: "plain_text".to_string(),
                observed_at: "2026-04-17T09:00:00Z".to_string(),
                first_seen_at: "2026-04-17T08:00:00Z".to_string(),
                last_seen_at: "2026-04-17T09:00:00Z".to_string(),
                app_name: Some("Editor".to_string()),
                app_bundle_id: Some("com.example.Editor".to_string()),
                best_text: "git commit".to_string(),
                preview_text: "git commit".to_string(),
                urls: Vec::new(),
                file_paths: Vec::new(),
                item_count: 1,
                total_bytes: 10,
                capture_count: 1,
                score: None,
                why_matched: None,
                snippet: "git commit".to_string(),
            }],
            best_match_confidence: RecallMatchConfidence::High,
            best_match_score: Some(0.91),
            why_selected: "Selected the strongest search match".to_string(),
            quoted_text: Some("git status".to_string()),
        };

        let markdown = render_recall_markdown(&envelope);
        let toon = render_recall_toon(&envelope);

        assert!(markdown.contains("# Best Match"));
        assert!(markdown.contains("## Alternatives"));
        assert!(markdown.contains("> git status"));
        assert!(toon.contains("best_candidate[#1"));
        assert!(toon.contains("alternatives[#1"));
    }
}
