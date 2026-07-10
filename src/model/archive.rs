use serde::Serialize;

use super::clipboard::ClipboardItem;
use super::kinds::SnapshotKind;
use super::text_projection::{FlattenedTextProjection, TextFragment};

mod doctor_report;
pub use doctor_report::DoctorIntegrityReport;

#[derive(Debug, Clone, Serialize)]
pub struct CaptureStoreResult {
    snapshot_id: i64,
    event_id: i64,
    inserted_new_snapshot: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MatchEvidence {
    matched_fields: Vec<String>,
    exact_phrase: bool,
    term_coverage: Option<f64>,
    snippet_source: Option<String>,
    snippet_text: Option<String>,
    query_classification: String,
    match_quality: Option<f64>,
}

impl MatchEvidence {
    #[must_use]
    pub(crate) fn new(
        matched_fields: Vec<String>,
        exact_phrase: bool,
        term_coverage: Option<f64>,
        snippet_source: Option<String>,
        snippet_text: Option<String>,
        query_classification: &str,
        match_quality: Option<f64>,
    ) -> Self {
        Self {
            matched_fields,
            exact_phrase,
            term_coverage,
            snippet_source,
            snippet_text,
            query_classification: query_classification.to_string(),
            match_quality,
        }
    }

    #[must_use]
    pub fn matched_fields(&self) -> &[String] {
        &self.matched_fields
    }
    #[must_use]
    pub fn exact_phrase(&self) -> bool {
        self.exact_phrase
    }
    #[must_use]
    pub fn term_coverage(&self) -> Option<f64> {
        self.term_coverage
    }
    #[must_use]
    pub fn match_quality(&self) -> Option<f64> {
        self.match_quality
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SearchOrderKey {
    pub(crate) primary: f64,
    pub(crate) secondary: f64,
    pub(crate) exact_phrase: bool,
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    snapshot_id: i64,
    event_id: i64,
    sha256: String,
    snapshot_kind: SnapshotKind,
    preview_text: String,
    search_text: String,
    why_matched: Option<String>,
    matched_fields: Vec<String>,
    capture_count: usize,
    first_observed_at: String,
    last_observed_at: String,
    last_frontmost_app_name: Option<String>,
    last_frontmost_app_bundle_id: Option<String>,
    urls: Vec<String>,
    file_paths: Vec<String>,
    total_bytes: usize,
    item_count: usize,
    score: Option<f64>,
    match_evidence: Option<MatchEvidence>,
    #[serde(skip)]
    search_order_key: Option<SearchOrderKey>,
}

#[derive(Debug, Clone)]
pub(crate) struct SearchHitParts {
    pub(crate) snapshot_id: i64,
    pub(crate) event_id: i64,
    pub(crate) sha256: String,
    pub(crate) snapshot_kind: SnapshotKind,
    pub(crate) preview_text: String,
    pub(crate) search_text: String,
    pub(crate) why_matched: Option<String>,
    pub(crate) matched_fields: Vec<String>,
    pub(crate) capture_count: usize,
    pub(crate) first_observed_at: String,
    pub(crate) last_observed_at: String,
    pub(crate) last_frontmost_app_name: Option<String>,
    pub(crate) last_frontmost_app_bundle_id: Option<String>,
    pub(crate) urls: Vec<String>,
    pub(crate) file_paths: Vec<String>,
    pub(crate) total_bytes: usize,
    pub(crate) item_count: usize,
    pub(crate) score: Option<f64>,
    pub(crate) match_evidence: Option<MatchEvidence>,
    pub(crate) search_order_key: Option<SearchOrderKey>,
}

#[cfg(test)]
impl SearchHitParts {
    #[must_use]
    pub(crate) fn plain_text(snapshot_id: i64, event_id: i64, preview_text: String) -> Self {
        Self {
            snapshot_id,
            event_id,
            sha256: String::new(),
            snapshot_kind: SnapshotKind::PlainText,
            search_text: preview_text.clone(),
            preview_text,
            why_matched: None,
            matched_fields: Vec::new(),
            capture_count: 1,
            first_observed_at: String::new(),
            last_observed_at: String::new(),
            last_frontmost_app_name: None,
            last_frontmost_app_bundle_id: None,
            urls: Vec::new(),
            file_paths: Vec::new(),
            total_bytes: 0,
            item_count: 1,
            score: None,
            match_evidence: None,
            search_order_key: None,
        }
    }

    #[must_use]
    pub(crate) fn with_sha256(mut self, sha256: String) -> Self {
        self.sha256 = sha256;
        self
    }

    #[must_use]
    pub(crate) fn with_match(
        mut self,
        why_matched: Option<String>,
        matched_fields: Vec<String>,
    ) -> Self {
        self.why_matched = why_matched;
        self.matched_fields = matched_fields;
        self
    }

    #[must_use]
    pub(crate) fn with_capture_summary(
        mut self,
        capture_count: usize,
        first_observed_at: String,
        last_observed_at: String,
    ) -> Self {
        self.capture_count = capture_count;
        self.first_observed_at = first_observed_at;
        self.last_observed_at = last_observed_at;
        self
    }

    #[must_use]
    pub(crate) fn with_last_frontmost_app(
        mut self,
        name: Option<String>,
        bundle_id: Option<String>,
    ) -> Self {
        self.last_frontmost_app_name = name;
        self.last_frontmost_app_bundle_id = bundle_id;
        self
    }

    #[must_use]
    pub(crate) fn with_size(mut self, total_bytes: usize, item_count: usize) -> Self {
        self.total_bytes = total_bytes;
        self.item_count = item_count;
        self
    }

    #[must_use]
    pub(crate) fn with_score(mut self, score: Option<f64>) -> Self {
        self.score = score;
        self
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize)]
pub struct CaptureEvent {
    event_id: i64,
    observed_at: String,
    change_count: i64,
    frontmost_app_name: Option<String>,
    frontmost_app_bundle_id: Option<String>,
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize)]
pub struct TimelineEvent {
    event_id: i64,
    snapshot_id: i64,
    observed_at: String,
    change_count: i64,
    sha256: String,
    snapshot_kind: SnapshotKind,
    best_text: String,
    preview_text: String,
    frontmost_app_name: Option<String>,
    frontmost_app_bundle_id: Option<String>,
    urls: Vec<String>,
    file_paths: Vec<String>,
    total_bytes: usize,
    item_count: usize,
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize)]
pub struct SnapshotDetails {
    snapshot_id: i64,
    sha256: String,
    snapshot_kind: SnapshotKind,
    best_text: String,
    best_text_uti: Option<String>,
    text_fragments: Vec<TextFragment>,
    urls: Vec<String>,
    file_paths: Vec<String>,
    html_text: Option<String>,
    rtf_text: Option<String>,
    text_summary: String,
    ocr_text: Option<String>,
    ocr_status: Option<String>,
    preview_text: String,
    search_text: String,
    item_count: usize,
    total_bytes: usize,
    created_at: String,
    capture_count: usize,
    first_observed_at: String,
    last_observed_at: String,
    last_frontmost_app_name: Option<String>,
    last_frontmost_app_bundle_id: Option<String>,
    recent_events: Vec<CaptureEvent>,
    items: Vec<ClipboardItem>,
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    db_path: String,
    sqlite_version: String,
    journal_mode: String,
    fts5_compile_option_present: bool,
    fts5_create_virtual_table_ok: bool,
    compile_options: Vec<String>,
    integrity: DoctorIntegrityReport,
}

impl CaptureStoreResult {
    #[must_use]
    pub(crate) fn new(snapshot_id: i64, event_id: i64, inserted_new_snapshot: bool) -> Self {
        Self {
            snapshot_id,
            event_id,
            inserted_new_snapshot,
        }
    }

    #[must_use]
    pub fn snapshot_id(&self) -> i64 {
        self.snapshot_id
    }

    #[must_use]
    pub fn event_id(&self) -> i64 {
        self.event_id
    }

    #[must_use]
    pub fn inserted_new_snapshot(&self) -> bool {
        self.inserted_new_snapshot
    }
}

impl SearchHit {
    #[must_use]
    pub(crate) fn from_parts(parts: SearchHitParts) -> Self {
        Self {
            snapshot_id: parts.snapshot_id,
            event_id: parts.event_id,
            sha256: parts.sha256,
            snapshot_kind: parts.snapshot_kind,
            preview_text: parts.preview_text,
            search_text: parts.search_text,
            why_matched: parts.why_matched,
            matched_fields: parts.matched_fields,
            capture_count: parts.capture_count,
            first_observed_at: parts.first_observed_at,
            last_observed_at: parts.last_observed_at,
            last_frontmost_app_name: parts.last_frontmost_app_name,
            last_frontmost_app_bundle_id: parts.last_frontmost_app_bundle_id,
            urls: parts.urls,
            file_paths: parts.file_paths,
            total_bytes: parts.total_bytes,
            item_count: parts.item_count,
            score: parts.score,
            match_evidence: parts.match_evidence,
            search_order_key: parts.search_order_key,
        }
    }

    pub(crate) fn with_search_evidence(
        mut self,
        evidence: MatchEvidence,
        order_key: SearchOrderKey,
    ) -> Self {
        self.match_evidence = Some(evidence);
        self.search_order_key = Some(order_key);
        self
    }

    #[must_use]
    pub fn snapshot_id(&self) -> i64 {
        self.snapshot_id
    }

    #[must_use]
    pub fn event_id(&self) -> i64 {
        self.event_id
    }

    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    #[must_use]
    pub fn snapshot_kind(&self) -> SnapshotKind {
        self.snapshot_kind
    }

    #[must_use]
    pub fn preview_text(&self) -> &str {
        &self.preview_text
    }

    #[must_use]
    pub fn search_text(&self) -> &str {
        &self.search_text
    }

    #[must_use]
    pub fn why_matched(&self) -> Option<&str> {
        self.why_matched.as_deref()
    }

    #[must_use]
    pub fn matched_fields(&self) -> &[String] {
        &self.matched_fields
    }

    #[must_use]
    pub fn capture_count(&self) -> usize {
        self.capture_count
    }

    #[must_use]
    pub fn first_observed_at(&self) -> &str {
        &self.first_observed_at
    }

    #[must_use]
    pub fn last_observed_at(&self) -> &str {
        &self.last_observed_at
    }

    #[must_use]
    pub fn last_frontmost_app_name(&self) -> Option<&str> {
        self.last_frontmost_app_name.as_deref()
    }

    #[must_use]
    pub fn last_frontmost_app_bundle_id(&self) -> Option<&str> {
        self.last_frontmost_app_bundle_id.as_deref()
    }

    #[must_use]
    pub fn urls(&self) -> &[String] {
        &self.urls
    }

    #[must_use]
    pub fn file_paths(&self) -> &[String] {
        &self.file_paths
    }

    #[must_use]
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    #[must_use]
    pub fn item_count(&self) -> usize {
        self.item_count
    }

    #[must_use]
    pub fn score(&self) -> Option<f64> {
        self.score
    }

    #[must_use]
    pub fn match_evidence(&self) -> Option<&MatchEvidence> {
        self.match_evidence.as_ref()
    }

    #[must_use]
    pub fn match_quality(&self) -> Option<f64> {
        self.match_evidence
            .as_ref()
            .and_then(MatchEvidence::match_quality)
    }

    #[must_use]
    pub(crate) fn search_order_key(&self) -> Option<SearchOrderKey> {
        self.search_order_key
    }
}

impl CaptureEvent {
    #[must_use]
    pub(crate) fn new(
        event_id: i64,
        observed_at: String,
        change_count: i64,
        frontmost_app_name: Option<String>,
        frontmost_app_bundle_id: Option<String>,
    ) -> Self {
        Self {
            event_id,
            observed_at,
            change_count,
            frontmost_app_name,
            frontmost_app_bundle_id,
        }
    }

    #[must_use]
    pub fn event_id(&self) -> i64 {
        self.event_id
    }

    #[must_use]
    pub fn observed_at(&self) -> &str {
        &self.observed_at
    }

    #[must_use]
    pub fn change_count(&self) -> i64 {
        self.change_count
    }

    #[must_use]
    pub fn frontmost_app_name(&self) -> Option<&str> {
        self.frontmost_app_name.as_deref()
    }

    #[must_use]
    pub fn frontmost_app_bundle_id(&self) -> Option<&str> {
        self.frontmost_app_bundle_id.as_deref()
    }
}

impl TimelineEvent {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub(crate) fn new(
        event_id: i64,
        snapshot_id: i64,
        observed_at: String,
        change_count: i64,
        sha256: String,
        snapshot_kind: SnapshotKind,
        best_text: String,
        preview_text: String,
        frontmost_app_name: Option<String>,
        frontmost_app_bundle_id: Option<String>,
        urls: Vec<String>,
        file_paths: Vec<String>,
        total_bytes: usize,
        item_count: usize,
    ) -> Self {
        Self {
            event_id,
            snapshot_id,
            observed_at,
            change_count,
            sha256,
            snapshot_kind,
            best_text,
            preview_text,
            frontmost_app_name,
            frontmost_app_bundle_id,
            urls,
            file_paths,
            total_bytes,
            item_count,
        }
    }

    #[must_use]
    pub fn event_id(&self) -> i64 {
        self.event_id
    }

    #[must_use]
    pub fn snapshot_id(&self) -> i64 {
        self.snapshot_id
    }

    #[must_use]
    pub fn observed_at(&self) -> &str {
        &self.observed_at
    }

    #[must_use]
    pub fn change_count(&self) -> i64 {
        self.change_count
    }

    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    #[must_use]
    pub fn snapshot_kind(&self) -> SnapshotKind {
        self.snapshot_kind
    }

    #[must_use]
    pub fn best_text(&self) -> &str {
        &self.best_text
    }

    #[must_use]
    pub fn preview_text(&self) -> &str {
        &self.preview_text
    }

    #[must_use]
    pub fn frontmost_app_name(&self) -> Option<&str> {
        self.frontmost_app_name.as_deref()
    }

    #[must_use]
    pub fn frontmost_app_bundle_id(&self) -> Option<&str> {
        self.frontmost_app_bundle_id.as_deref()
    }

    #[must_use]
    pub fn urls(&self) -> &[String] {
        &self.urls
    }

    #[must_use]
    pub fn file_paths(&self) -> &[String] {
        &self.file_paths
    }

    #[must_use]
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    #[must_use]
    pub fn item_count(&self) -> usize {
        self.item_count
    }
}

impl SnapshotDetails {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub(crate) fn new(
        snapshot_id: i64,
        sha256: String,
        snapshot_kind: SnapshotKind,
        preview_text: String,
        search_text: String,
        item_count: usize,
        total_bytes: usize,
        created_at: String,
        capture_count: usize,
        first_observed_at: String,
        last_observed_at: String,
        last_frontmost_app_name: Option<String>,
        last_frontmost_app_bundle_id: Option<String>,
        ocr_text: Option<String>,
        ocr_status: Option<String>,
        recent_events: Vec<CaptureEvent>,
        items: Vec<ClipboardItem>,
    ) -> Self {
        let projection = FlattenedTextProjection::from_items(&items).with_ocr(ocr_text, ocr_status);

        Self {
            snapshot_id,
            sha256,
            snapshot_kind,
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
            preview_text,
            search_text,
            item_count,
            total_bytes,
            created_at,
            capture_count,
            first_observed_at,
            last_observed_at,
            last_frontmost_app_name,
            last_frontmost_app_bundle_id,
            recent_events,
            items,
        }
    }

    #[must_use]
    pub fn snapshot_id(&self) -> i64 {
        self.snapshot_id
    }

    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    #[must_use]
    pub fn snapshot_kind(&self) -> SnapshotKind {
        self.snapshot_kind
    }

    #[must_use]
    pub fn best_text(&self) -> &str {
        &self.best_text
    }

    #[must_use]
    pub fn best_text_uti(&self) -> Option<&str> {
        self.best_text_uti.as_deref()
    }

    #[must_use]
    pub fn text_fragments(&self) -> &[TextFragment] {
        &self.text_fragments
    }

    #[must_use]
    pub fn urls(&self) -> &[String] {
        &self.urls
    }

    #[must_use]
    pub fn file_paths(&self) -> &[String] {
        &self.file_paths
    }

    #[must_use]
    pub fn html_text(&self) -> Option<&str> {
        self.html_text.as_deref()
    }

    #[must_use]
    pub fn rtf_text(&self) -> Option<&str> {
        self.rtf_text.as_deref()
    }

    #[must_use]
    pub fn text_summary(&self) -> &str {
        &self.text_summary
    }

    #[must_use]
    pub fn ocr_text(&self) -> Option<&str> {
        self.ocr_text.as_deref()
    }

    #[must_use]
    pub fn ocr_status(&self) -> Option<&str> {
        self.ocr_status.as_deref()
    }

    #[must_use]
    pub fn preview_text(&self) -> &str {
        &self.preview_text
    }

    #[must_use]
    pub fn search_text(&self) -> &str {
        &self.search_text
    }

    #[must_use]
    pub fn item_count(&self) -> usize {
        self.item_count
    }

    #[must_use]
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    #[must_use]
    pub fn created_at(&self) -> &str {
        &self.created_at
    }

    #[must_use]
    pub fn capture_count(&self) -> usize {
        self.capture_count
    }

    #[must_use]
    pub fn first_observed_at(&self) -> &str {
        &self.first_observed_at
    }

    #[must_use]
    pub fn last_observed_at(&self) -> &str {
        &self.last_observed_at
    }

    #[must_use]
    pub fn last_frontmost_app_name(&self) -> Option<&str> {
        self.last_frontmost_app_name.as_deref()
    }

    #[must_use]
    pub fn last_frontmost_app_bundle_id(&self) -> Option<&str> {
        self.last_frontmost_app_bundle_id.as_deref()
    }

    #[must_use]
    pub fn recent_events(&self) -> &[CaptureEvent] {
        &self.recent_events
    }

    #[must_use]
    pub fn items(&self) -> &[ClipboardItem] {
        &self.items
    }

    pub(crate) fn set_recent_events(&mut self, recent_events: Vec<CaptureEvent>) {
        self.recent_events = recent_events;
    }

    pub(crate) fn set_items(&mut self, items: Vec<ClipboardItem>) {
        let projection = FlattenedTextProjection::from_items(&items)
            .with_ocr(self.ocr_text.clone(), self.ocr_status.clone());
        self.best_text = projection.best_text().to_string();
        self.best_text_uti = projection.best_text_uti().map(ToOwned::to_owned);
        self.text_fragments = projection.text_fragments().to_vec();
        self.urls = projection.urls().to_vec();
        self.file_paths = projection.file_paths().to_vec();
        self.html_text = projection.html_text().map(ToOwned::to_owned);
        self.rtf_text = projection.rtf_text().map(ToOwned::to_owned);
        self.text_summary = projection.text_summary().to_string();
        self.ocr_text = projection.ocr_text().map(ToOwned::to_owned);
        self.ocr_status = projection.ocr_status().map(ToOwned::to_owned);
        self.items = items;
    }
}
