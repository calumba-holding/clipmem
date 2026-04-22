use std::fmt::Write;

use anyhow::{anyhow, Result};
use serde::Serialize;
use serde_json::{json, Value};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::db::{SearchResults, StatsReport, StatsSnapshotLeaderboardEntry, StatsTimeBucketEntry};
use crate::model::{
    DoctorReport, FlattenedTextProjection, SearchHit, SnapshotDetails, TextFragment, TimelineEvent,
};

use super::commands::CaptureOnceOutput;
use super::human::{render_get_human, render_list_human, render_recall_human};
use super::OutputFormat;

pub(super) const OUTPUT_SCHEMA_VERSION: u32 = 2;

#[derive(Debug)]
pub(super) struct UnsupportedFormatError {
    message: String,
}

impl UnsupportedFormatError {
    pub(super) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for UnsupportedFormatError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for UnsupportedFormatError {}

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
    pub(super) applied_filters: Value,
    pub(super) snapshot: SnapshotDetails,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct StatsEnvelope {
    pub(super) schema_version: u32,
    pub(super) command: &'static str,
    pub(super) generated_at: String,
    pub(super) applied_filters: Value,
    pub(super) stats: StatsReport,
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
pub(super) struct SnapshotListRow {
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
    pub(super) best_text_uti: Option<String>,
    pub(super) text_fragments: Vec<TextFragment>,
    pub(super) urls: Vec<String>,
    pub(super) file_paths: Vec<String>,
    pub(super) html_text: Option<String>,
    pub(super) rtf_text: Option<String>,
    pub(super) text_summary: String,
    pub(super) ocr_text: Option<String>,
    pub(super) ocr_status: Option<String>,
    pub(super) preview_text: String,
    pub(super) item_count: usize,
    pub(super) total_bytes: usize,
    pub(super) capture_count: usize,
    pub(super) score: Option<f64>,
    pub(super) why_matched: Option<String>,
    pub(super) matched_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct TimelineListRow {
    pub(super) event_id: i64,
    pub(super) snapshot_id: i64,
    pub(super) observed_at: String,
    pub(super) change_count: i64,
    pub(super) kind: String,
    pub(super) app_name: Option<String>,
    pub(super) app_bundle_id: Option<String>,
    pub(super) best_text: String,
    pub(super) best_text_uti: Option<String>,
    pub(super) text_fragments: Vec<TextFragment>,
    pub(super) urls: Vec<String>,
    pub(super) file_paths: Vec<String>,
    pub(super) html_text: Option<String>,
    pub(super) rtf_text: Option<String>,
    pub(super) text_summary: String,
    pub(super) ocr_text: Option<String>,
    pub(super) ocr_status: Option<String>,
    pub(super) preview_text: String,
    pub(super) total_bytes: usize,
    pub(super) sha256: String,
    pub(super) item_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub(super) enum ListRow {
    Snapshot(SnapshotListRow),
    Timeline(TimelineListRow),
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
    pub(super) best_text_uti: Option<String>,
    pub(super) text_fragments: Vec<TextFragment>,
    pub(super) urls: Vec<String>,
    pub(super) file_paths: Vec<String>,
    pub(super) html_text: Option<String>,
    pub(super) rtf_text: Option<String>,
    pub(super) text_summary: String,
    pub(super) ocr_text: Option<String>,
    pub(super) ocr_status: Option<String>,
    pub(super) preview_text: String,
    pub(super) item_count: usize,
    pub(super) total_bytes: usize,
    pub(super) capture_count: usize,
    pub(super) score: Option<f64>,
    pub(super) why_matched: Option<String>,
    pub(super) matched_fields: Vec<String>,
    pub(super) snippet: String,
}

struct ToonSnapshotRowProjection {
    snapshot_id: i64,
    event_id: i64,
    observed_at: String,
    first_seen_at: String,
    last_seen_at: String,
    kind: String,
    app_name: Option<String>,
    app_bundle_id: Option<String>,
    display_text: String,
    capture_count: usize,
    item_count: usize,
    total_bytes: usize,
    score: Option<f64>,
    why_matched: Option<String>,
}

impl ToonSnapshotRowProjection {
    const FIELDS: [&'static str; 14] = [
        "snapshot_id",
        "event_id",
        "observed_at",
        "first_seen_at",
        "last_seen_at",
        "kind",
        "app_name",
        "app_bundle_id",
        "display_text",
        "capture_count",
        "item_count",
        "total_bytes",
        "score",
        "why_matched",
    ];

    fn from_row(row: &SnapshotListRow) -> Self {
        Self {
            snapshot_id: row.snapshot_id,
            event_id: row.event_id,
            observed_at: row.observed_at.clone(),
            first_seen_at: row.first_seen_at.clone(),
            last_seen_at: row.last_seen_at.clone(),
            kind: row.kind.clone(),
            app_name: row.app_name.clone(),
            app_bundle_id: row.app_bundle_id.clone(),
            display_text: first_non_empty_text(&[
                row.best_text.as_str(),
                row.preview_text.as_str(),
                row.text_summary.as_str(),
            ]),
            capture_count: row.capture_count,
            item_count: row.item_count,
            total_bytes: row.total_bytes,
            score: row.score,
            why_matched: row.why_matched.clone(),
        }
    }

    fn values(&self) -> Vec<Value> {
        vec![
            Value::from(self.snapshot_id),
            Value::from(self.event_id),
            Value::String(self.observed_at.clone()),
            Value::String(self.first_seen_at.clone()),
            Value::String(self.last_seen_at.clone()),
            Value::String(self.kind.clone()),
            self.app_name.clone().map_or(Value::Null, Value::String),
            self.app_bundle_id
                .clone()
                .map_or(Value::Null, Value::String),
            Value::String(self.display_text.clone()),
            Value::from(self.capture_count as u64),
            Value::from(self.item_count as u64),
            Value::from(self.total_bytes as u64),
            self.score.map_or(Value::Null, Value::from),
            self.why_matched.clone().map_or(Value::Null, Value::String),
        ]
    }
}

struct ToonTimelineRowProjection {
    event_id: i64,
    snapshot_id: i64,
    observed_at: String,
    change_count: i64,
    kind: String,
    app_name: Option<String>,
    app_bundle_id: Option<String>,
    display_text: String,
    item_count: usize,
    total_bytes: usize,
}

impl ToonTimelineRowProjection {
    const FIELDS: [&'static str; 10] = [
        "event_id",
        "snapshot_id",
        "observed_at",
        "change_count",
        "kind",
        "app_name",
        "app_bundle_id",
        "display_text",
        "item_count",
        "total_bytes",
    ];

    fn from_row(row: &TimelineListRow) -> Self {
        Self {
            event_id: row.event_id,
            snapshot_id: row.snapshot_id,
            observed_at: row.observed_at.clone(),
            change_count: row.change_count,
            kind: row.kind.clone(),
            app_name: row.app_name.clone(),
            app_bundle_id: row.app_bundle_id.clone(),
            display_text: first_non_empty_text(&[
                row.best_text.as_str(),
                row.preview_text.as_str(),
                row.text_summary.as_str(),
            ]),
            item_count: row.item_count,
            total_bytes: row.total_bytes,
        }
    }

    fn values(&self) -> Vec<Value> {
        vec![
            Value::from(self.event_id),
            Value::from(self.snapshot_id),
            Value::String(self.observed_at.clone()),
            Value::from(self.change_count),
            Value::String(self.kind.clone()),
            self.app_name.clone().map_or(Value::Null, Value::String),
            self.app_bundle_id
                .clone()
                .map_or(Value::Null, Value::String),
            Value::String(self.display_text.clone()),
            Value::from(self.item_count as u64),
            Value::from(self.total_bytes as u64),
        ]
    }
}

struct ToonRecallRowProjection {
    snapshot_id: i64,
    event_id: i64,
    observed_at: String,
    first_seen_at: String,
    last_seen_at: String,
    kind: String,
    app_name: Option<String>,
    app_bundle_id: Option<String>,
    display_text: String,
    capture_count: usize,
    item_count: usize,
    total_bytes: usize,
    score: Option<f64>,
    why_matched: Option<String>,
}

impl ToonRecallRowProjection {
    const FIELDS: [&'static str; 14] = [
        "snapshot_id",
        "event_id",
        "observed_at",
        "first_seen_at",
        "last_seen_at",
        "kind",
        "app_name",
        "app_bundle_id",
        "display_text",
        "capture_count",
        "item_count",
        "total_bytes",
        "score",
        "why_matched",
    ];

    fn from_row(row: &RecallOutputRow) -> Self {
        Self {
            snapshot_id: row.snapshot_id,
            event_id: row.event_id,
            observed_at: row.observed_at.clone(),
            first_seen_at: row.first_seen_at.clone(),
            last_seen_at: row.last_seen_at.clone(),
            kind: row.kind.clone(),
            app_name: row.app_name.clone(),
            app_bundle_id: row.app_bundle_id.clone(),
            display_text: first_non_empty_text(&[
                row.snippet.as_str(),
                row.best_text.as_str(),
                row.preview_text.as_str(),
                row.text_summary.as_str(),
            ]),
            capture_count: row.capture_count,
            item_count: row.item_count,
            total_bytes: row.total_bytes,
            score: row.score,
            why_matched: row.why_matched.clone(),
        }
    }

    fn values(&self) -> Vec<Value> {
        vec![
            Value::from(self.snapshot_id),
            Value::from(self.event_id),
            Value::String(self.observed_at.clone()),
            Value::String(self.first_seen_at.clone()),
            Value::String(self.last_seen_at.clone()),
            Value::String(self.kind.clone()),
            self.app_name.clone().map_or(Value::Null, Value::String),
            self.app_bundle_id
                .clone()
                .map_or(Value::Null, Value::String),
            Value::String(self.display_text.clone()),
            Value::from(self.capture_count as u64),
            Value::from(self.item_count as u64),
            Value::from(self.total_bytes as u64),
            self.score.map_or(Value::Null, Value::from),
            self.why_matched.clone().map_or(Value::Null, Value::String),
        ]
    }
}

fn first_non_empty_text(candidates: &[&str]) -> String {
    candidates
        .iter()
        .find(|candidate| !candidate.trim().is_empty())
        .map(|candidate| (*candidate).to_string())
        .unwrap_or_default()
}

impl SnapshotListRow {
    #[must_use]
    pub(super) fn from_hit(
        hit: &SearchHit,
        include_why_matched: bool,
        projection: &FlattenedTextProjection,
    ) -> Self {
        let why_matched = include_why_matched
            .then(|| hit.why_matched().unwrap_or(hit.preview_text()).to_string());

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
            best_text: projection.best_text().to_string(),
            best_text_uti: projection.best_text_uti().map(ToOwned::to_owned),
            text_fragments: projection.text_fragments().to_vec(),
            urls: projection.urls().to_vec(),
            file_paths: projection.file_paths().to_vec(),
            html_text: projection.html_text().map(ToOwned::to_owned),
            rtf_text: projection.rtf_text().map(ToOwned::to_owned),
            text_summary: projection.text_summary().to_string(),
            ocr_text: projection.ocr_text().map(ToOwned::to_owned),
            ocr_status: projection.ocr_status().map(ToOwned::to_owned),
            preview_text: hit.preview_text().to_string(),
            item_count: hit.item_count(),
            total_bytes: hit.total_bytes(),
            capture_count: hit.capture_count(),
            score: hit.score(),
            why_matched,
            matched_fields: hit.matched_fields().to_vec(),
        }
    }
}

impl TimelineListRow {
    #[must_use]
    pub(super) fn from_event(event: &TimelineEvent, projection: &FlattenedTextProjection) -> Self {
        Self {
            event_id: event.event_id(),
            snapshot_id: event.snapshot_id(),
            observed_at: event.observed_at().to_string(),
            change_count: event.change_count(),
            kind: event.snapshot_kind().as_str().to_string(),
            app_name: event.frontmost_app_name().map(ToOwned::to_owned),
            app_bundle_id: event.frontmost_app_bundle_id().map(ToOwned::to_owned),
            best_text: projection.best_text().to_string(),
            best_text_uti: projection.best_text_uti().map(ToOwned::to_owned),
            text_fragments: projection.text_fragments().to_vec(),
            urls: projection.urls().to_vec(),
            file_paths: projection.file_paths().to_vec(),
            html_text: projection.html_text().map(ToOwned::to_owned),
            rtf_text: projection.rtf_text().map(ToOwned::to_owned),
            text_summary: projection.text_summary().to_string(),
            ocr_text: projection.ocr_text().map(ToOwned::to_owned),
            ocr_status: projection.ocr_status().map(ToOwned::to_owned),
            preview_text: event.preview_text().to_string(),
            total_bytes: event.total_bytes(),
            sha256: event.sha256().to_string(),
            item_count: event.item_count(),
        }
    }
}

impl ListRow {
    #[must_use]
    pub(super) fn from_hit(
        hit: &SearchHit,
        include_why_matched: bool,
        projection: &FlattenedTextProjection,
    ) -> Self {
        Self::Snapshot(SnapshotListRow::from_hit(
            hit,
            include_why_matched,
            projection,
        ))
    }

    #[must_use]
    pub(super) fn from_timeline_event(
        event: &TimelineEvent,
        projection: &FlattenedTextProjection,
    ) -> Self {
        Self::Timeline(TimelineListRow::from_event(event, projection))
    }

    #[must_use]
    fn app_name(&self) -> Option<&str> {
        match self {
            Self::Snapshot(row) => row.app_name.as_deref(),
            Self::Timeline(row) => row.app_name.as_deref(),
        }
    }

    #[must_use]
    fn app_bundle_id(&self) -> Option<&str> {
        match self {
            Self::Snapshot(row) => row.app_bundle_id.as_deref(),
            Self::Timeline(row) => row.app_bundle_id.as_deref(),
        }
    }

    #[must_use]
    fn best_text(&self) -> &str {
        match self {
            Self::Snapshot(row) => &row.best_text,
            Self::Timeline(row) => &row.best_text,
        }
    }
}

impl RecallOutputRow {
    #[must_use]
    pub(super) fn from_hit(
        hit: &SearchHit,
        full: bool,
        projection: &FlattenedTextProjection,
    ) -> Self {
        let why_matched = hit.why_matched().map(ToOwned::to_owned);
        let source_text = if projection.best_text().trim().is_empty() {
            best_text_from_hit(hit)
        } else {
            projection.best_text().to_string()
        };
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
            best_text_uti: projection.best_text_uti().map(ToOwned::to_owned),
            text_fragments: projection.text_fragments().to_vec(),
            urls: projection.urls().to_vec(),
            file_paths: projection.file_paths().to_vec(),
            html_text: projection.html_text().map(ToOwned::to_owned),
            rtf_text: projection.rtf_text().map(ToOwned::to_owned),
            text_summary: projection.text_summary().to_string(),
            ocr_text: projection.ocr_text().map(ToOwned::to_owned),
            ocr_status: projection.ocr_status().map(ToOwned::to_owned),
            preview_text: hit.preview_text().to_string(),
            item_count: hit.item_count(),
            total_bytes: hit.total_bytes(),
            capture_count: hit.capture_count(),
            score: hit.score(),
            why_matched,
            matched_fields: hit.matched_fields().to_vec(),
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
        OutputFormat::Human => {
            print!("{}", render_list_human(envelope));
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
        OutputFormat::Toon => Err(UnsupportedFormatError::new(
            "format toon is only supported for flattened list outputs; `clipmem get` returns nested snapshot detail",
        )
        .into()),
        OutputFormat::Human => {
            print!("{}", render_get_human(envelope));
            Ok(())
        }
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
        super::RecallOutputFormat::Human => {
            print!("{}", render_recall_human(envelope));
            Ok(())
        }
    }
}

pub(super) fn render_capture_once_text(output: &CaptureOnceOutput) -> String {
    let mut out = String::new();
    match output {
        CaptureOnceOutput::Stored(output) => {
            let store_state = if output.store.inserted_new_snapshot() {
                "new content"
            } else {
                "already known content"
            };
            let _ = writeln!(
                out,
                "stored snapshot {} via event {} ({store_state})",
                output.store.snapshot_id(),
                output.store.event_id()
            );
            let _ = writeln!(out, "kind: {}", output.snapshot.snapshot_kind());
            let _ = writeln!(out, "bytes: {}", output.snapshot.total_bytes());
            if let Some(app) = output.snapshot.frontmost_app_name() {
                let _ = writeln!(out, "frontmost app: {app}");
            }
            if !output.snapshot.preview_text().is_empty() {
                let _ = writeln!(out, "preview: {}", output.snapshot.preview_text());
            }
        }
        CaptureOnceOutput::Skipped(output) => {
            let _ = writeln!(out, "skipped capture reason={}", output.reason.as_str());
            let _ = writeln!(out, "kind: {}", output.kind);
            let _ = writeln!(out, "bytes: {}", output.total_bytes);
            if let Some(app) = &output.frontmost_app_name {
                let _ = writeln!(out, "frontmost app: {app}");
            }
        }
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
        if let Some(why_matched) = hit
            .why_matched()
            .filter(|value| *value != hit.preview_text())
        {
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

pub(super) fn render_timeline_text(events: &[TimelineEvent]) -> String {
    let mut out = String::new();
    for event in events {
        let app = event
            .frontmost_app_name()
            .or(event.frontmost_app_bundle_id())
            .unwrap_or("unknown app");

        let _ = writeln!(
            out,
            "event {} · snapshot {} · {} · change_count={} · {} · {} bytes",
            event.event_id(),
            event.snapshot_id(),
            format_utc_timestamp(event.observed_at()),
            event.change_count(),
            app,
            event.total_bytes()
        );

        if !event.best_text().is_empty() {
            let _ = writeln!(out, "  best:    {}", event.best_text());
        }
        if !event.preview_text().is_empty() && event.preview_text() != event.best_text() {
            let _ = writeln!(out, "  preview: {}", event.preview_text());
        }
        if !event.urls().is_empty() {
            let _ = writeln!(out, "  urls:    {}", event.urls().join(", "));
        }
        if !event.file_paths().is_empty() {
            let _ = writeln!(out, "  files:   {}", event.file_paths().join(", "));
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

pub(super) fn render_stats_text(stats: &StatsReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Overview");
    let _ = writeln!(out, "  snapshots: {}", stats.snapshot_count);
    let _ = writeln!(out, "  capture events: {}", stats.capture_event_count);
    let _ = writeln!(out, "  unique apps: {}", stats.unique_app_count);
    let _ = writeln!(out, "  total bytes: {}", stats.total_bytes);
    let _ = writeln!(
        out,
        "  average bytes per snapshot: {:.1}",
        stats.average_bytes_per_snapshot
    );
    let _ = writeln!(
        out,
        "  average captures per snapshot: {:.2}",
        stats.average_captures_per_snapshot
    );
    let _ = writeln!(out, "  dedupe ratio: {:.1}%", stats.dedupe_ratio * 100.0);
    let _ = writeln!(
        out,
        "  observed: {} to {}",
        stats
            .first_observed_at
            .as_deref()
            .map_or("none".to_string(), format_utc_timestamp),
        stats
            .last_observed_at
            .as_deref()
            .map_or("none".to_string(), format_utc_timestamp)
    );
    let _ = writeln!(
        out,
        "  archive span: {}",
        stats.archive_span_seconds.map_or_else(
            || "0s".to_string(),
            |seconds| { format_duration_seconds(seconds.max(0) as u64) }
        )
    );
    push_blank_line(&mut out);

    let _ = writeln!(out, "Content mix");
    if stats.kind_breakdown.is_empty() {
        let _ = writeln!(out, "  none");
    } else {
        for entry in &stats.kind_breakdown {
            let _ = writeln!(
                out,
                "  {}: {} snapshots, {} bytes",
                entry.kind, entry.snapshot_count, entry.total_bytes
            );
        }
    }
    push_blank_line(&mut out);

    let _ = writeln!(out, "Top apps");
    if stats.top_apps.is_empty() {
        let _ = writeln!(out, "  none");
    } else {
        for app in &stats.top_apps {
            let _ = writeln!(out, "  {}: {} events", app.app, app.capture_event_count);
        }
    }
    push_blank_line(&mut out);

    let _ = writeln!(out, "Activity patterns");
    let busiest_hour = peak_bucket(&stats.busiest_hours).map_or_else(
        || "none".to_string(),
        |entry| {
            format!(
                "{}:00 UTC ({} events)",
                entry.bucket, entry.capture_event_count
            )
        },
    );
    let busiest_weekday = peak_bucket(&stats.busiest_weekdays).map_or_else(
        || "none".to_string(),
        |entry| format!("{} ({} events)", entry.bucket, entry.capture_event_count),
    );
    let _ = writeln!(out, "  Busiest hour: {busiest_hour}");
    let _ = writeln!(out, "  Busiest weekday: {busiest_weekday}");
    push_blank_line(&mut out);

    let _ = writeln!(out, "Leaderboards");
    if let Some(snapshot) = &stats.most_recopied_snapshot {
        let _ = writeln!(out, "  Most re-copied snapshot:");
        push_snapshot_leaderboard_entry(&mut out, snapshot, 4);
    } else {
        let _ = writeln!(out, "  Most re-copied snapshot: none");
    }
    let _ = writeln!(out, "  Largest snapshots:");
    push_snapshot_leaderboard(&mut out, &stats.largest_snapshots, 4);
    let _ = writeln!(out, "  Most captured snapshots:");
    push_snapshot_leaderboard(&mut out, &stats.most_captured_snapshots, 4);

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
    if !snapshot.best_text().is_empty() {
        let _ = writeln!(out, "best: {}", snapshot.best_text());
    }
    if !snapshot.preview_text().is_empty() {
        let _ = writeln!(out, "preview: {}", snapshot.preview_text());
    }
    if !snapshot.urls().is_empty() {
        let _ = writeln!(out, "urls: {}", snapshot.urls().join(", "));
    }
    if !snapshot.file_paths().is_empty() {
        let _ = writeln!(out, "files: {}", snapshot.file_paths().join(", "));
    }
    if let Some(ocr_text) = snapshot.ocr_text() {
        let _ = writeln!(out, "ocr: {}", ocr_text);
    }
    if let Some(ocr_status) = snapshot.ocr_status() {
        let _ = writeln!(out, "ocr status: {}", ocr_status);
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
    let _ = writeln!(
        out,
        "- Filters: {}",
        render_filter_pairs(&envelope.applied_filters)
    );
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

    if envelope.command == "timeline" {
        let _ = writeln!(
            out,
            "| event_id | snapshot_id | observed_at | app | best_text |"
        );
        let _ = writeln!(out, "| --- | --- | --- | --- | --- |");
    } else {
        let _ = writeln!(
            out,
            "| snapshot_id | kind | observed_at | app | best_text | score |"
        );
        let _ = writeln!(out, "| --- | --- | --- | --- | --- | --- |");
    }
    for row in &envelope.results {
        let app = row
            .app_name()
            .or(row.app_bundle_id())
            .unwrap_or("unknown app");
        let best_text = truncate_for_markdown(row.best_text(), 100);
        match row {
            ListRow::Snapshot(row) => {
                let score = row
                    .score
                    .map_or_else(|| "null".to_string(), |value| format!("{value:.3}"));
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
            ListRow::Timeline(row) => {
                let _ = writeln!(
                    out,
                    "| {} | {} | {} | {} | {} |",
                    row.event_id,
                    row.snapshot_id,
                    escape_markdown_cell(&row.observed_at),
                    escape_markdown_cell(app),
                    escape_markdown_cell(&best_text)
                );
            }
        }
    }

    out
}

fn render_get_markdown(envelope: &GetEnvelope) -> String {
    let snapshot = &envelope.snapshot;
    let mut out = String::new();
    let _ = writeln!(out, "# clipmem get");
    let _ = writeln!(out);
    let _ = writeln!(out, "- Generated at: {}", envelope.generated_at);
    let _ = writeln!(
        out,
        "- Filters: {}",
        render_filter_pairs(&envelope.applied_filters)
    );
    let _ = writeln!(out, "- Snapshot: {}", snapshot.snapshot_id());
    let _ = writeln!(out, "- Kind: {}", snapshot.snapshot_kind());
    let _ = writeln!(out, "- First seen: {}", snapshot.first_observed_at());
    let _ = writeln!(out, "- Last seen: {}", snapshot.last_observed_at());
    let _ = writeln!(out, "- Captures: {}", snapshot.capture_count());
    let _ = writeln!(out, "- Bytes: {}", snapshot.total_bytes());
    let _ = writeln!(out, "- Items: {}", snapshot.item_count());
    if !snapshot.best_text().is_empty() {
        let _ = writeln!(
            out,
            "- Best text: {}",
            escape_markdown_cell(&truncate_for_markdown(snapshot.best_text(), 240))
        );
    }
    if let Some(best_text_uti) = snapshot.best_text_uti() {
        let _ = writeln!(
            out,
            "- Best text UTI: {}",
            escape_markdown_cell(best_text_uti)
        );
    }
    if !snapshot.text_summary().is_empty() {
        let _ = writeln!(
            out,
            "- Text summary: {}",
            escape_markdown_cell(&truncate_for_markdown(snapshot.text_summary(), 240))
        );
    }
    if !snapshot.urls().is_empty() {
        let _ = writeln!(
            out,
            "- URLs: {}",
            escape_markdown_cell(&truncate_for_markdown(&snapshot.urls().join(", "), 240))
        );
    }
    if !snapshot.file_paths().is_empty() {
        let _ = writeln!(
            out,
            "- File paths: {}",
            escape_markdown_cell(&truncate_for_markdown(
                &snapshot.file_paths().join(", "),
                240
            ))
        );
    }
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
    render_toon_entry(
        &mut out,
        "schema_version",
        &Value::from(envelope.schema_version as u64),
        0,
    );
    render_toon_entry(
        &mut out,
        "command",
        &Value::String(envelope.command.to_string()),
        0,
    );
    render_toon_entry(
        &mut out,
        "generated_at",
        &Value::String(envelope.generated_at.clone()),
        0,
    );
    render_toon_entry(&mut out, "applied_filters", &envelope.applied_filters, 0);
    render_toon_entry(&mut out, "truncated", &Value::Bool(envelope.truncated), 0);
    render_toon_entry(
        &mut out,
        "next_cursor",
        &envelope
            .next_cursor
            .as_ref()
            .map_or(Value::Null, |value| Value::String(value.clone())),
        0,
    );

    let fields = if envelope.command == "timeline" {
        ToonTimelineRowProjection::FIELDS.as_slice()
    } else {
        ToonSnapshotRowProjection::FIELDS.as_slice()
    };
    let _ = writeln!(
        out,
        "results[#{}\t]{{{}}}:",
        envelope.results.len(),
        fields.join("\t")
    );

    for row in &envelope.results {
        let values = match row {
            ListRow::Snapshot(row) => ToonSnapshotRowProjection::from_row(row).values(),
            ListRow::Timeline(row) => ToonTimelineRowProjection::from_row(row).values(),
        };
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
    render_toon_entry(
        &mut out,
        "schema_version",
        &Value::from(envelope.schema_version as u64),
        0,
    );
    render_toon_entry(&mut out, "command", &Value::String("recall".to_string()), 0);
    render_toon_entry(
        &mut out,
        "generated_at",
        &Value::String(envelope.generated_at.clone()),
        0,
    );
    render_toon_entry(
        &mut out,
        "query",
        &envelope.query.clone().map_or(Value::Null, Value::String),
        0,
    );
    render_toon_entry(
        &mut out,
        "best_match_confidence",
        &serde_json::to_value(&envelope.best_match_confidence).unwrap_or(Value::Null),
        0,
    );
    render_toon_entry(
        &mut out,
        "best_match_score",
        &envelope.best_match_score.map_or(Value::Null, Value::from),
        0,
    );
    render_toon_entry(
        &mut out,
        "why_selected",
        &Value::String(envelope.why_selected.clone()),
        0,
    );
    if let Some(quoted_text) = &envelope.quoted_text {
        render_toon_entry(
            &mut out,
            "quoted_text",
            &Value::String(quoted_text.clone()),
            0,
        );
    }
    render_toon_entry(&mut out, "applied_filters", &envelope.applied_filters, 0);

    render_recall_rows_toon(
        &mut out,
        "best_candidate",
        std::slice::from_ref(&envelope.best_candidate),
    );
    render_recall_rows_toon(&mut out, "alternatives", &envelope.alternatives);

    out
}

fn render_recall_rows_toon(out: &mut String, key: &str, rows: &[RecallOutputRow]) {
    let _ = writeln!(
        out,
        "{key}[#{}\t]{{{}}}:",
        rows.len(),
        ToonRecallRowProjection::FIELDS.join("\t")
    );
    for row in rows {
        let values = ToonRecallRowProjection::from_row(row).values();
        let encoded = values
            .iter()
            .map(encode_toon_scalar)
            .collect::<Vec<_>>()
            .join("\t");
        let _ = writeln!(out, "  {encoded}");
    }
}

fn render_toon_entry(out: &mut String, key: &str, value: &Value, indent: usize) {
    let padding = " ".repeat(indent);
    match value {
        Value::Object(object) => {
            let _ = writeln!(out, "{padding}{key}:");
            render_toon_object_entries(out, object, indent + 2);
        }
        Value::Array(array) => render_toon_array(out, Some(key), array, indent),
        _ => {
            let _ = writeln!(out, "{padding}{key}: {}", encode_toon_scalar(value));
        }
    }
}

fn render_toon_object_entries(
    out: &mut String,
    object: &serde_json::Map<String, Value>,
    indent: usize,
) {
    for (key, value) in object {
        render_toon_entry(out, key, value, indent);
    }
}

fn render_toon_array(out: &mut String, key: Option<&str>, values: &[Value], indent: usize) {
    let padding = " ".repeat(indent);
    let key_prefix = key
        .map(|name| format!("{padding}{name}"))
        .unwrap_or(padding);

    if values.iter().all(Value::is_null)
        || values
            .iter()
            .all(|value| !matches!(value, Value::Array(_) | Value::Object(_)))
    {
        if values.is_empty() {
            let _ = writeln!(out, "{key_prefix}[#0\t]:");
            return;
        }

        let encoded = values
            .iter()
            .map(encode_toon_scalar)
            .collect::<Vec<_>>()
            .join("\t");
        let _ = writeln!(out, "{key_prefix}[#{}\t]: {encoded}", values.len());
        return;
    }

    let _ = writeln!(out, "{key_prefix}[#{}]:", values.len());
    for value in values {
        render_toon_list_item(out, value, indent + 2);
    }
}

fn render_toon_list_item(out: &mut String, value: &Value, indent: usize) {
    let padding = " ".repeat(indent);
    match value {
        Value::Object(object) => render_toon_object_list_item(out, object, indent),
        Value::Array(array) => {
            let _ = writeln!(out, "{padding}-");
            render_toon_array(out, None, array, indent + 2);
        }
        _ => {
            let _ = writeln!(out, "{padding}- {}", encode_toon_scalar(value));
        }
    }
}

fn render_toon_object_list_item(
    out: &mut String,
    object: &serde_json::Map<String, Value>,
    indent: usize,
) {
    let padding = " ".repeat(indent);
    let mut entries = object.iter();
    let Some((first_key, first_value)) = entries.next() else {
        let _ = writeln!(out, "{padding}-");
        return;
    };

    match first_value {
        Value::Object(nested) => {
            let _ = writeln!(out, "{padding}- {first_key}:");
            render_toon_object_entries(out, nested, indent + 4);
        }
        Value::Array(array) => {
            let _ = writeln!(out, "{padding}- {first_key}:");
            render_toon_array(out, None, array, indent + 4);
        }
        _ => {
            let _ = writeln!(
                out,
                "{padding}- {first_key}: {}",
                encode_toon_scalar(first_value)
            );
        }
    }

    for (key, value) in entries {
        render_toon_entry(out, key, value, indent + 2);
    }
}

fn encode_toon_scalar(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => encode_toon_string(text),
        Value::Array(_) | Value::Object(_) => {
            unreachable!("encode_toon_scalar only accepts primitive TOON values")
        }
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

const fn render_confidence_label(confidence: &RecallMatchConfidence) -> &'static str {
    match confidence {
        RecallMatchConfidence::High => "high",
        RecallMatchConfidence::Medium => "medium",
        RecallMatchConfidence::Low => "low",
    }
}

fn best_text_from_hit(hit: &SearchHit) -> String {
    if hit.preview_text().trim().is_empty() {
        hit.why_matched()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(hit.preview_text())
            .to_string()
    } else {
        hit.preview_text().to_string()
    }
}

fn escape_markdown_cell(value: &str) -> String {
    value.replace('|', "\\|")
}

fn truncate_for_markdown(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        value.to_string()
    } else {
        value
            .chars()
            .take(limit.saturating_sub(1))
            .collect::<String>()
            + "…"
    }
}

fn push_blank_line(out: &mut String) {
    out.push('\n');
}

fn peak_bucket(buckets: &[StatsTimeBucketEntry]) -> Option<&StatsTimeBucketEntry> {
    buckets
        .iter()
        .max_by_key(|entry| {
            (
                entry.capture_event_count,
                std::cmp::Reverse(entry.bucket.as_str()),
            )
        })
        .filter(|entry| entry.capture_event_count > 0)
}

fn push_snapshot_leaderboard(
    out: &mut String,
    snapshots: &[StatsSnapshotLeaderboardEntry],
    indent: usize,
) {
    if snapshots.is_empty() {
        let _ = writeln!(out, "{}none", " ".repeat(indent));
        return;
    }

    for snapshot in snapshots {
        push_snapshot_leaderboard_entry(out, snapshot, indent);
    }
}

fn push_snapshot_leaderboard_entry(
    out: &mut String,
    snapshot: &StatsSnapshotLeaderboardEntry,
    indent: usize,
) {
    let prefix = " ".repeat(indent);
    let app = snapshot.app_name.as_deref().unwrap_or("Unknown");
    let preview = if snapshot.preview_text.trim().is_empty() {
        "(no preview)".to_string()
    } else {
        truncate_for_markdown(&snapshot.preview_text.replace('\n', " "), 120)
    };
    let _ = writeln!(
        out,
        "{prefix}[{}] {} captures, {}, {} bytes, last seen {}, {}",
        snapshot.snapshot_id,
        snapshot.capture_count,
        snapshot.kind,
        snapshot.total_bytes,
        format_utc_timestamp(&snapshot.last_observed_at),
        app
    );
    let _ = writeln!(out, "{prefix}  preview: {preview}");
}

fn format_duration_seconds(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let remainder = seconds % 60;

    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {remainder}s")
    } else {
        format!("{remainder}s")
    }
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
        CaptureStoreResult, DoctorReport, SearchHit, SnapshotDetails, SnapshotKind, TimelineEvent,
    };

    use super::super::commands::{CaptureOnceOutput, CaptureOnceStoredOutput};
    use super::{
        render_capture_once_text, render_doctor_text, render_get_markdown, render_hits_text,
        render_list_markdown, render_list_toon, render_recall_markdown, render_recall_toon,
        render_search_results_text, render_snapshot_text, render_timeline_text, GetEnvelope,
        ListEnvelope, ListRow, RecallEnvelope, RecallMatchConfidence, RecallOutputRow,
        SnapshotListRow, TimelineListRow, OUTPUT_SCHEMA_VERSION,
    };

    #[test]
    fn render_capture_once_text_reports_summary_lines() {
        let text = render_capture_once_text(&CaptureOnceOutput::Stored(CaptureOnceStoredOutput {
            store: CaptureStoreResult::new(9, 12, true),
            snapshot: build_snapshot(
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
        }));

        assert!(text.contains("stored snapshot 9 via event 12"));
        assert!(text.contains("kind: plain_text"));
        assert!(text.contains("preview: git status"));
    }

    #[test]
    fn render_capture_once_text_skips_preview_for_filtered_content() {
        let text = render_capture_once_text(&CaptureOnceOutput::Skipped(
            super::super::commands::CaptureOnceSkippedOutput {
                status: "skipped",
                reason: crate::db::CaptureSkipReason::ApiKeyFilter,
                kind: "plain_text".to_string(),
                total_bytes: 48,
                frontmost_app_name: Some("Terminal".to_string()),
                frontmost_app_bundle_id: Some("com.apple.Terminal".to_string()),
            },
        ));

        assert!(text.contains("skipped capture reason=api_key_filter"));
        assert!(text.contains("kind: plain_text"));
        assert!(!text.contains("preview:"));
    }

    #[test]
    fn render_hits_text_includes_match_score_urls_and_files() {
        let text = render_hits_text(&[SearchHit::new(
            42,
            77,
            "abc123".to_string(),
            SnapshotKind::PlainText,
            "git clone".to_string(),
            "git clone".to_string(),
            Some("⟦git⟧ clone".to_string()),
            vec!["best_text".to_string(), "search_text".to_string()],
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
            None,
            None,
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
                "git push".to_string(),
                Some("git push".to_string()),
                vec!["best_text".to_string(), "search_text".to_string()],
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
    fn render_timeline_text_reports_event_history() {
        let text = render_timeline_text(&[TimelineEvent::new(
            21,
            7,
            "2026-04-16 11:00:00".to_string(),
            3,
            "abc123".to_string(),
            SnapshotKind::PlainText,
            "git push".to_string(),
            "git push".to_string(),
            Some("Terminal".to_string()),
            Some("com.apple.Terminal".to_string()),
            vec!["https://example.com".to_string()],
            vec!["/Users/test/repo".to_string()],
            8,
            1,
        )]);

        assert!(text.contains("event 21"));
        assert!(text.contains("change_count=3"));
        assert!(text.contains("best:    git push"));
    }

    #[test]
    fn markdown_and_toon_render_envelopes() {
        let row = ListRow::Snapshot(SnapshotListRow {
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
            best_text_uti: Some("public.utf8-plain-text".to_string()),
            text_fragments: Vec::new(),
            urls: vec!["https://example.com".to_string()],
            file_paths: Vec::new(),
            html_text: None,
            rtf_text: None,
            ocr_text: None,
            ocr_status: None,
            text_summary: "git status".to_string(),
            preview_text: "git status".to_string(),
            item_count: 1,
            total_bytes: 10,
            capture_count: 1,
            score: Some(0.3),
            why_matched: Some("git status".to_string()),
            matched_fields: vec!["best_text".to_string(), "search_text".to_string()],
        });
        let envelope = ListEnvelope {
            schema_version: OUTPUT_SCHEMA_VERSION,
            command: "search",
            generated_at: "2026-04-17T10:00:00Z".to_string(),
            applied_filters: json!({
                "query": "git",
                "requested_mode": "auto",
                "mode_used": "literal",
                "apps": ["Terminal", "Safari"],
                "window": {
                    "hours": 24,
                },
                "flags": [
                    {
                        "name": "prefer_recent",
                        "enabled": true,
                    }
                ],
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
        assert!(toon.contains(
            "results[#1\t]{snapshot_id\tevent_id\tobserved_at\tfirst_seen_at\tlast_seen_at\tkind\tapp_name\tapp_bundle_id\tdisplay_text\tcapture_count\titem_count\ttotal_bytes\tscore\twhy_matched}:"
        ));
        assert!(toon.contains("git status"));
        assert!(!toon.contains("sha256"));
        assert!(!toon.contains("text_fragments"));
        assert!(toon.contains("apps[#2\t]: Terminal\tSafari"));
        assert!(toon.contains("window:"));
        assert!(toon.contains("hours: 24"));
        assert!(toon.contains("flags[#1]:"));
        assert!(toon.contains("name: prefer_recent"));
    }

    #[test]
    fn get_markdown_renders_summary_items_and_events() {
        let markdown = render_get_markdown(&GetEnvelope {
            schema_version: OUTPUT_SCHEMA_VERSION,
            command: "get",
            generated_at: "2026-04-17T10:00:00Z".to_string(),
            applied_filters: json!({}),
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
                None,
                None,
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
                best_text: "".to_string(),
                best_text_uti: Some("public.utf8-plain-text".to_string()),
                text_fragments: Vec::new(),
                urls: vec!["https://example.com".to_string()],
                file_paths: vec!["/tmp/file.txt".to_string()],
                html_text: None,
                rtf_text: None,
                ocr_text: None,
                ocr_status: None,
                text_summary: "git status".to_string(),
                preview_text: "git status".to_string(),
                item_count: 1,
                total_bytes: 10,
                capture_count: 1,
                score: Some(0.1),
                why_matched: Some("git status".to_string()),
                matched_fields: vec!["best_text".to_string(), "search_text".to_string()],
                snippet: "quoted snippet".to_string(),
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
                best_text_uti: Some("public.utf8-plain-text".to_string()),
                text_fragments: Vec::new(),
                urls: Vec::new(),
                file_paths: Vec::new(),
                html_text: None,
                rtf_text: None,
                ocr_text: None,
                ocr_status: None,
                text_summary: "git commit".to_string(),
                preview_text: "git commit".to_string(),
                item_count: 1,
                total_bytes: 10,
                capture_count: 1,
                score: None,
                why_matched: None,
                matched_fields: Vec::new(),
                snippet: String::new(),
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
        assert!(toon.contains(
            "best_candidate[#1\t]{snapshot_id\tevent_id\tobserved_at\tfirst_seen_at\tlast_seen_at\tkind\tapp_name\tapp_bundle_id\tdisplay_text\tcapture_count\titem_count\ttotal_bytes\tscore\twhy_matched}:"
        ));
        assert!(toon.contains("alternatives[#1\t]{snapshot_id\tevent_id\tobserved_at\tfirst_seen_at\tlast_seen_at\tkind\tapp_name\tapp_bundle_id\tdisplay_text\tcapture_count\titem_count\ttotal_bytes\tscore\twhy_matched}:"));
        assert!(toon.contains("quoted snippet"));
        assert!(toon.contains("git commit"));
        assert!(!toon.contains("\tsnippet}:"));
        assert!(!toon.contains("matched_fields"));
        assert!(!toon.contains("sha256"));
    }

    #[test]
    fn timeline_toon_uses_scalar_projection_display_text_fallback() {
        let envelope = ListEnvelope {
            schema_version: OUTPUT_SCHEMA_VERSION,
            command: "timeline",
            generated_at: "2026-04-17T10:00:00Z".to_string(),
            applied_filters: json!({ "hours": 24 }),
            truncated: false,
            next_cursor: None,
            results: vec![ListRow::Timeline(TimelineListRow {
                event_id: 9,
                snapshot_id: 3,
                observed_at: "2026-04-17T10:00:00Z".to_string(),
                change_count: 2,
                kind: "plain_text".to_string(),
                app_name: Some("Terminal".to_string()),
                app_bundle_id: Some("com.apple.Terminal".to_string()),
                best_text: String::new(),
                best_text_uti: None,
                text_fragments: Vec::new(),
                urls: Vec::new(),
                file_paths: Vec::new(),
                html_text: None,
                rtf_text: None,
                ocr_text: None,
                ocr_status: None,
                text_summary: "git summary".to_string(),
                preview_text: "git preview".to_string(),
                total_bytes: 42,
                sha256: "abc123".to_string(),
                item_count: 1,
            })],
        };

        let toon = render_list_toon(&envelope);

        assert!(toon.contains(
            "results[#1\t]{event_id\tsnapshot_id\tobserved_at\tchange_count\tkind\tapp_name\tapp_bundle_id\tdisplay_text\titem_count\ttotal_bytes}:"
        ));
        assert!(toon.contains("git preview"));
        assert!(!toon.contains("best_text_uti"));
        assert!(!toon.contains("sha256"));
    }
}
