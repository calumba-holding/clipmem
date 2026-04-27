use serde::Serialize;
use serde_json::Value;

use crate::cli::output::support::{best_text_from_hit, truncate_for_markdown};
use crate::db::StatsReport;
use crate::model::{
    FlattenedTextProjection, SearchHit, SnapshotDetails, TextFragment, TimelineEvent,
};

pub(in crate::cli) const OUTPUT_SCHEMA_VERSION: u32 = 2;

#[derive(Debug)]
pub(in crate::cli) struct UnsupportedFormatError {
    pub(in crate::cli) message: String,
}

impl UnsupportedFormatError {
    pub(in crate::cli) fn new(message: impl Into<String>) -> Self {
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
pub(in crate::cli) struct ListEnvelope {
    pub(in crate::cli) schema_version: u32,
    pub(in crate::cli) command: &'static str,
    pub(in crate::cli) generated_at: String,
    pub(in crate::cli) applied_filters: Value,
    pub(in crate::cli) truncated: bool,
    pub(in crate::cli) next_cursor: Option<String>,
    pub(in crate::cli) results: Vec<ListRow>,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::cli) struct GetEnvelope {
    pub(in crate::cli) schema_version: u32,
    pub(in crate::cli) command: &'static str,
    pub(in crate::cli) generated_at: String,
    pub(in crate::cli) applied_filters: Value,
    pub(in crate::cli) snapshot: SnapshotDetails,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::cli) struct StatsEnvelope {
    pub(in crate::cli) schema_version: u32,
    pub(in crate::cli) command: &'static str,
    pub(in crate::cli) generated_at: String,
    pub(in crate::cli) applied_filters: Value,
    pub(in crate::cli) stats: StatsReport,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::cli) enum RecallMatchConfidence {
    High,
    Medium,
    Low,
}

impl RecallMatchConfidence {
    #[must_use]
    pub(in crate::cli) fn from_normalized_score(score: f64) -> Self {
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
pub(in crate::cli) struct RecallEnvelope {
    pub(in crate::cli) schema_version: u32,
    pub(in crate::cli) command: &'static str,
    pub(in crate::cli) generated_at: String,
    pub(in crate::cli) applied_filters: Value,
    pub(in crate::cli) query: Option<String>,
    pub(in crate::cli) best_candidate: RecallOutputRow,
    pub(in crate::cli) alternatives: Vec<RecallOutputRow>,
    pub(in crate::cli) best_match_confidence: RecallMatchConfidence,
    pub(in crate::cli) best_match_score: Option<f64>,
    pub(in crate::cli) why_selected: String,
    pub(in crate::cli) quoted_text: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::cli) struct SnapshotListRow {
    pub(in crate::cli) snapshot_id: i64,
    pub(in crate::cli) event_id: i64,
    pub(in crate::cli) sha256: String,
    pub(in crate::cli) kind: String,
    pub(in crate::cli) observed_at: String,
    pub(in crate::cli) first_seen_at: String,
    pub(in crate::cli) last_seen_at: String,
    pub(in crate::cli) app_name: Option<String>,
    pub(in crate::cli) app_bundle_id: Option<String>,
    pub(in crate::cli) best_text: String,
    pub(in crate::cli) best_text_uti: Option<String>,
    pub(in crate::cli) text_fragments: Vec<TextFragment>,
    pub(in crate::cli) urls: Vec<String>,
    pub(in crate::cli) file_paths: Vec<String>,
    pub(in crate::cli) html_text: Option<String>,
    pub(in crate::cli) rtf_text: Option<String>,
    pub(in crate::cli) text_summary: String,
    pub(in crate::cli) ocr_text: Option<String>,
    pub(in crate::cli) ocr_status: Option<String>,
    pub(in crate::cli) preview_text: String,
    pub(in crate::cli) item_count: usize,
    pub(in crate::cli) total_bytes: usize,
    pub(in crate::cli) capture_count: usize,
    pub(in crate::cli) score: Option<f64>,
    pub(in crate::cli) why_matched: Option<String>,
    pub(in crate::cli) matched_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::cli) struct TimelineListRow {
    pub(in crate::cli) event_id: i64,
    pub(in crate::cli) snapshot_id: i64,
    pub(in crate::cli) observed_at: String,
    pub(in crate::cli) change_count: i64,
    pub(in crate::cli) kind: String,
    pub(in crate::cli) app_name: Option<String>,
    pub(in crate::cli) app_bundle_id: Option<String>,
    pub(in crate::cli) best_text: String,
    pub(in crate::cli) best_text_uti: Option<String>,
    pub(in crate::cli) text_fragments: Vec<TextFragment>,
    pub(in crate::cli) urls: Vec<String>,
    pub(in crate::cli) file_paths: Vec<String>,
    pub(in crate::cli) html_text: Option<String>,
    pub(in crate::cli) rtf_text: Option<String>,
    pub(in crate::cli) text_summary: String,
    pub(in crate::cli) ocr_text: Option<String>,
    pub(in crate::cli) ocr_status: Option<String>,
    pub(in crate::cli) preview_text: String,
    pub(in crate::cli) total_bytes: usize,
    pub(in crate::cli) sha256: String,
    pub(in crate::cli) item_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub(in crate::cli) enum ListRow {
    Snapshot(SnapshotListRow),
    Timeline(TimelineListRow),
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::cli) struct RecallOutputRow {
    pub(in crate::cli) snapshot_id: i64,
    pub(in crate::cli) event_id: i64,
    pub(in crate::cli) sha256: String,
    pub(in crate::cli) kind: String,
    pub(in crate::cli) observed_at: String,
    pub(in crate::cli) first_seen_at: String,
    pub(in crate::cli) last_seen_at: String,
    pub(in crate::cli) app_name: Option<String>,
    pub(in crate::cli) app_bundle_id: Option<String>,
    pub(in crate::cli) best_text: String,
    pub(in crate::cli) best_text_uti: Option<String>,
    pub(in crate::cli) text_fragments: Vec<TextFragment>,
    pub(in crate::cli) urls: Vec<String>,
    pub(in crate::cli) file_paths: Vec<String>,
    pub(in crate::cli) html_text: Option<String>,
    pub(in crate::cli) rtf_text: Option<String>,
    pub(in crate::cli) text_summary: String,
    pub(in crate::cli) ocr_text: Option<String>,
    pub(in crate::cli) ocr_status: Option<String>,
    pub(in crate::cli) preview_text: String,
    pub(in crate::cli) item_count: usize,
    pub(in crate::cli) total_bytes: usize,
    pub(in crate::cli) capture_count: usize,
    pub(in crate::cli) score: Option<f64>,
    pub(in crate::cli) why_matched: Option<String>,
    pub(in crate::cli) matched_fields: Vec<String>,
    pub(in crate::cli) snippet: String,
}

pub(in crate::cli) struct ToonSnapshotRowProjection {
    pub(in crate::cli) snapshot_id: i64,
    pub(in crate::cli) event_id: i64,
    pub(in crate::cli) observed_at: String,
    pub(in crate::cli) first_seen_at: String,
    pub(in crate::cli) last_seen_at: String,
    pub(in crate::cli) kind: String,
    pub(in crate::cli) app_name: Option<String>,
    pub(in crate::cli) app_bundle_id: Option<String>,
    pub(in crate::cli) display_text: String,
    pub(in crate::cli) capture_count: usize,
    pub(in crate::cli) item_count: usize,
    pub(in crate::cli) total_bytes: usize,
    pub(in crate::cli) score: Option<f64>,
    pub(in crate::cli) why_matched: Option<String>,
}

impl ToonSnapshotRowProjection {
    pub(in crate::cli) const FIELDS: [&'static str; 14] = [
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

    pub(in crate::cli) fn from_row(row: &SnapshotListRow) -> Self {
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

    pub(in crate::cli) fn values(&self) -> Vec<Value> {
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

pub(in crate::cli) struct ToonTimelineRowProjection {
    pub(in crate::cli) event_id: i64,
    pub(in crate::cli) snapshot_id: i64,
    pub(in crate::cli) observed_at: String,
    pub(in crate::cli) change_count: i64,
    pub(in crate::cli) kind: String,
    pub(in crate::cli) app_name: Option<String>,
    pub(in crate::cli) app_bundle_id: Option<String>,
    pub(in crate::cli) display_text: String,
    pub(in crate::cli) item_count: usize,
    pub(in crate::cli) total_bytes: usize,
}

impl ToonTimelineRowProjection {
    pub(in crate::cli) const FIELDS: [&'static str; 10] = [
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

    pub(in crate::cli) fn from_row(row: &TimelineListRow) -> Self {
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

    pub(in crate::cli) fn values(&self) -> Vec<Value> {
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

pub(in crate::cli) struct ToonRecallRowProjection {
    pub(in crate::cli) snapshot_id: i64,
    pub(in crate::cli) event_id: i64,
    pub(in crate::cli) observed_at: String,
    pub(in crate::cli) first_seen_at: String,
    pub(in crate::cli) last_seen_at: String,
    pub(in crate::cli) kind: String,
    pub(in crate::cli) app_name: Option<String>,
    pub(in crate::cli) app_bundle_id: Option<String>,
    pub(in crate::cli) display_text: String,
    pub(in crate::cli) capture_count: usize,
    pub(in crate::cli) item_count: usize,
    pub(in crate::cli) total_bytes: usize,
    pub(in crate::cli) score: Option<f64>,
    pub(in crate::cli) why_matched: Option<String>,
}

impl ToonRecallRowProjection {
    pub(in crate::cli) const FIELDS: [&'static str; 14] = [
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

    pub(in crate::cli) fn from_row(row: &RecallOutputRow) -> Self {
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

    pub(in crate::cli) fn values(&self) -> Vec<Value> {
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

pub(in crate::cli) fn first_non_empty_text(candidates: &[&str]) -> String {
    candidates
        .iter()
        .find(|candidate| !candidate.trim().is_empty())
        .map(|candidate| (*candidate).to_string())
        .unwrap_or_default()
}

impl SnapshotListRow {
    #[must_use]
    pub(in crate::cli) fn from_hit(
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
    pub(in crate::cli) fn from_event(
        event: &TimelineEvent,
        projection: &FlattenedTextProjection,
    ) -> Self {
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
    pub(in crate::cli) fn from_hit(
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
    pub(in crate::cli) fn from_timeline_event(
        event: &TimelineEvent,
        projection: &FlattenedTextProjection,
    ) -> Self {
        Self::Timeline(TimelineListRow::from_event(event, projection))
    }

    #[must_use]
    pub(in crate::cli) fn app_name(&self) -> Option<&str> {
        match self {
            Self::Snapshot(row) => row.app_name.as_deref(),
            Self::Timeline(row) => row.app_name.as_deref(),
        }
    }

    #[must_use]
    pub(in crate::cli) fn app_bundle_id(&self) -> Option<&str> {
        match self {
            Self::Snapshot(row) => row.app_bundle_id.as_deref(),
            Self::Timeline(row) => row.app_bundle_id.as_deref(),
        }
    }

    #[must_use]
    pub(in crate::cli) fn best_text(&self) -> &str {
        match self {
            Self::Snapshot(row) => &row.best_text,
            Self::Timeline(row) => &row.best_text,
        }
    }
}

impl RecallOutputRow {
    #[must_use]
    pub(in crate::cli) fn from_hit(
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
