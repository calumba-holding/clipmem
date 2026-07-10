use std::path::PathBuf;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::model::{
    CaptureEvent, ClipboardItem, ClipboardKind, FlattenedTextProjection, SearchHit,
    SnapshotDetails, SnapshotKind, TextFragment,
};

#[derive(Debug)]
pub enum DatabaseOpenError {
    Missing(PathBuf),
    MigrationRequired { found: i64, supported: i64 },
    NewerSchema { found: i64, supported: i64 },
    NotArchive(PathBuf),
    InvalidSchema(String),
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
}

impl std::fmt::Display for DatabaseOpenError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(path) => write!(formatter, "database does not exist at {}", path.display()),
            Self::MigrationRequired { found, supported } => write!(
                formatter,
                "database schema version {found} requires migration to version {supported}; run `clipmem setup`"
            ),
            Self::NewerSchema { found, supported } => write!(
                formatter,
                "database schema version {found} is newer than supported version {supported}"
            ),
            Self::NotArchive(path) => write!(formatter, "database at {} is not a clipmem archive", path.display()),
            Self::InvalidSchema(message) => write!(formatter, "invalid clipmem archive schema: {message}"),
            Self::Sqlite(error) => write!(formatter, "SQLite open failed: {error}"),
            Self::Io(error) => write!(formatter, "database filesystem check failed: {error}"),
        }
    }
}

impl std::error::Error for DatabaseOpenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sqlite(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepresentationManifest {
    pub(in crate::db) uti: String,
    pub(in crate::db) kind: ClipboardKind,
    pub(in crate::db) is_text: bool,
    pub(in crate::db) byte_len: usize,
    pub(in crate::db) raw_sha256: String,
    pub(in crate::db) text_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SnapshotItemManifest {
    pub(in crate::db) item_index: usize,
    pub(in crate::db) primary_kind: ClipboardKind,
    pub(in crate::db) primary_uti: Option<String>,
    pub(in crate::db) preview_text: String,
    pub(in crate::db) search_text: String,
    pub(in crate::db) total_bytes: usize,
    pub(in crate::db) representations: Vec<RepresentationManifest>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SnapshotMetadata {
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
    items: Vec<SnapshotItemManifest>,
}

impl SnapshotMetadata {
    pub(in crate::db) fn from_summary(
        summary: &SnapshotDetails,
        projection: &FlattenedTextProjection,
        recent_events: Vec<CaptureEvent>,
        items: Vec<SnapshotItemManifest>,
    ) -> Self {
        Self {
            snapshot_id: summary.snapshot_id(),
            sha256: summary.sha256().to_string(),
            snapshot_kind: summary.snapshot_kind(),
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
            preview_text: summary.preview_text().to_string(),
            search_text: summary.search_text().to_string(),
            item_count: summary.item_count(),
            total_bytes: summary.total_bytes(),
            created_at: summary.created_at().to_string(),
            capture_count: summary.capture_count(),
            first_observed_at: summary.first_observed_at().to_string(),
            last_observed_at: summary.last_observed_at().to_string(),
            last_frontmost_app_name: summary.last_frontmost_app_name().map(ToOwned::to_owned),
            last_frontmost_app_bundle_id: summary
                .last_frontmost_app_bundle_id()
                .map(ToOwned::to_owned),
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
    pub fn items(&self) -> &[SnapshotItemManifest] {
        &self.items
    }
}

impl From<SnapshotDetails> for SnapshotMetadata {
    fn from(details: SnapshotDetails) -> Self {
        let projection = FlattenedTextProjection::from_items(details.items()).with_ocr(
            details.ocr_text().map(ToOwned::to_owned),
            details.ocr_status().map(ToOwned::to_owned),
        );
        let items = details
            .items()
            .iter()
            .map(|item| SnapshotItemManifest {
                item_index: item.item_index(),
                primary_kind: item.primary_kind(),
                primary_uti: item.primary_uti().map(ToOwned::to_owned),
                preview_text: item.preview_text().to_string(),
                search_text: item.search_text().to_string(),
                total_bytes: item.total_bytes(),
                representations: item
                    .representations()
                    .iter()
                    .map(|representation| RepresentationManifest {
                        uti: representation.uti().to_string(),
                        kind: representation.kind(),
                        is_text: representation.is_text(),
                        byte_len: representation.byte_len(),
                        raw_sha256: representation.raw_sha256().to_string(),
                        text_value: representation.text_value().map(ToOwned::to_owned),
                    })
                    .collect(),
            })
            .collect();
        Self::from_summary(
            &details,
            &projection,
            details.recent_events().to_vec(),
            items,
        )
    }
}

impl RepresentationManifest {
    #[must_use]
    pub fn uti(&self) -> &str {
        &self.uti
    }
    #[must_use]
    pub fn kind(&self) -> ClipboardKind {
        self.kind
    }
    #[must_use]
    pub fn is_text(&self) -> bool {
        self.is_text
    }
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.byte_len
    }
    #[must_use]
    pub fn raw_sha256(&self) -> &str {
        &self.raw_sha256
    }
    #[must_use]
    pub fn text_value(&self) -> Option<&str> {
        self.text_value.as_deref()
    }
}

impl SnapshotItemManifest {
    #[must_use]
    pub fn item_index(&self) -> usize {
        self.item_index
    }
    #[must_use]
    pub fn primary_kind(&self) -> ClipboardKind {
        self.primary_kind
    }
    #[must_use]
    pub fn primary_uti(&self) -> Option<&str> {
        self.primary_uti.as_deref()
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
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }
    #[must_use]
    pub fn representations(&self) -> &[RepresentationManifest] {
        &self.representations
    }
}

impl RepresentationPayload {
    #[must_use]
    pub fn manifest(&self) -> &RepresentationManifest {
        &self.manifest
    }
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl RestorePayloadPlan {
    #[must_use]
    pub fn snapshot_sha256(&self) -> &str {
        &self.snapshot_sha256
    }
    #[must_use]
    pub fn items(&self) -> &[ClipboardItem] {
        &self.items
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepresentationPayload {
    pub(in crate::db) manifest: RepresentationManifest,
    pub(in crate::db) bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct RestorePayloadPlan {
    pub(in crate::db) snapshot_sha256: String,
    pub(in crate::db) items: Vec<ClipboardItem>,
}

pub struct Database {
    pub(crate) conn: Connection,
    pub(in crate::db) path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::db) enum ArchiveChangeKind {
    ArchiveContent,
    Settings,
    Ocr,
    Storage,
    Service,
    #[allow(dead_code)]
    AppPreferences,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArchiveRevision {
    pub(in crate::db) revision: u64,
    pub(in crate::db) archive_content_revision: u64,
    pub(in crate::db) settings_revision: u64,
    pub(in crate::db) ocr_revision: u64,
    pub(in crate::db) storage_revision: u64,
    pub(in crate::db) service_revision: u64,
    pub(in crate::db) app_preferences_revision: u64,
    pub(in crate::db) last_change_kind: String,
    pub(in crate::db) updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchMode {
    Auto,
    Fts,
    Literal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimelineSort {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalKind {
    Text,
    Html,
    Rtf,
    Url,
    File,
    Image,
    Pdf,
    Binary,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RetrievalFilters {
    pub(in crate::db) since: Option<String>,
    pub(in crate::db) until: Option<String>,
    pub(in crate::db) hours: Option<u32>,
    pub(in crate::db) app: Option<String>,
    pub(in crate::db) bundle_id: Option<String>,
    pub(in crate::db) kind: Option<RetrievalKind>,
    pub(in crate::db) has_text: bool,
    pub(in crate::db) has_url: bool,
    pub(in crate::db) has_file_url: bool,
    pub(in crate::db) has_image: bool,
    pub(in crate::db) has_pdf: bool,
    pub(in crate::db) min_bytes: Option<usize>,
    pub(in crate::db) max_bytes: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) struct CaptureSettings {
    pub(in crate::db) paused: bool,
    pub(in crate::db) retention_seconds: Option<u64>,
    pub(in crate::db) api_key_filter_enabled: bool,
    pub(in crate::db) ocr_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub(crate) struct CapturePolicy {
    pub(in crate::db) settings: CaptureSettings,
    pub(in crate::db) ignored_bundle_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CaptureSkipReason {
    Paused,
    IgnoredApp,
    ApiKeyFilter,
    RestoredSnapshot,
    TransientPlatformChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CaptureMode {
    Watch,
    Manual,
    #[allow(dead_code)]
    SetupSeed,
}

#[derive(Debug, Clone)]
pub(crate) enum CaptureOutcome {
    Stored {
        store: crate::model::CaptureStoreResult,
        enqueue_ocr: bool,
    },
    ObservedExisting {
        store: crate::model::CaptureStoreResult,
        enqueue_ocr: bool,
    },
    SuppressedRestore {
        operation_id: String,
    },
    SkippedPaused,
    SkippedIgnoredApp {
        bundle_id: String,
    },
    SkippedSensitive,
    #[allow(dead_code)]
    TransientPlatformChange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SnapshotDeletionReport {
    pub(in crate::db) snapshot_id: i64,
    pub(in crate::db) item_count: usize,
    pub(in crate::db) representation_count: usize,
    pub(in crate::db) capture_event_count: usize,
    pub(in crate::db) total_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PurgeReport {
    pub(in crate::db) older_than_seconds: u64,
    pub(in crate::db) dry_run: bool,
    pub(in crate::db) snapshot_count: usize,
    pub(in crate::db) item_count: usize,
    pub(in crate::db) representation_count: usize,
    pub(in crate::db) capture_event_count: usize,
    pub(in crate::db) total_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct OcrCandidate {
    pub(in crate::db) raw_sha256: String,
    pub(in crate::db) blob_value: Vec<u8>,
    pub(in crate::db) snapshot_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct OcrStatusReport {
    pub(in crate::db) pending: usize,
    pub(in crate::db) ready: usize,
    pub(in crate::db) failed: usize,
    pub(in crate::db) skipped: usize,
    pub(in crate::db) snapshots_with_ocr_text: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct OcrRunReport {
    pub(in crate::db) processed: usize,
    pub(in crate::db) ready: usize,
    pub(in crate::db) failed: usize,
    pub(in crate::db) skipped: usize,
    pub(in crate::db) remaining_pending: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct OcrCandidateSummary {
    pub(in crate::db) raw_sha256: String,
    pub(in crate::db) byte_len: usize,
    pub(in crate::db) snapshot_count: usize,
    pub(in crate::db) updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct OcrResultRecord {
    pub(in crate::db) raw_sha256: String,
    pub(in crate::db) status: String,
    pub(in crate::db) engine: Option<String>,
    pub(in crate::db) recognition_level: Option<String>,
    pub(in crate::db) text_value: Option<String>,
    pub(in crate::db) error: Option<String>,
    pub(in crate::db) attempt_count: usize,
    pub(in crate::db) updated_at: String,
    pub(in crate::db) snapshot_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct StorageFileSizes {
    pub(crate) db: u64,
    pub(crate) wal: u64,
    pub(crate) shm: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct StorageCheckpointReport {
    pub(crate) busy: i64,
    pub(crate) log: i64,
    pub(crate) checkpointed: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct StorageCompactReport {
    pub(crate) db_path: String,
    pub(crate) before: StorageFileSizes,
    pub(crate) after: StorageFileSizes,
    pub(crate) total_before_bytes: u64,
    pub(crate) total_after_bytes: u64,
    pub(crate) reclaimed_bytes: u64,
    pub(crate) estimated_reclaimable_bytes: u64,
    pub(crate) page_count: usize,
    pub(crate) freelist_count: usize,
    pub(crate) checkpoint: StorageCheckpointReport,
    pub(crate) dry_run: bool,
    pub(crate) completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ImageOptimizationReport {
    pub(crate) dry_run: bool,
    pub(crate) format: &'static str,
    pub(crate) scanned_rows: usize,
    pub(crate) compressed_rows: usize,
    pub(crate) skipped_rows: usize,
    pub(crate) conflict_count: usize,
    pub(crate) original_bytes: usize,
    pub(crate) optimized_bytes: usize,
    pub(crate) logical_saved_bytes: usize,
    pub(crate) compact_run: bool,
    pub(crate) compact: Option<StorageCompactReport>,
    pub(crate) compact_error: Option<String>,
    pub(crate) filesystem_saved_bytes: u64,
    pub(crate) filesystem_growth_bytes: u64,
    pub(crate) compact_recommended: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ImageOptimizationCandidateSummary {
    pub(in crate::db) snapshot_id: i64,
    pub(in crate::db) item_index: i64,
    pub(in crate::db) uti: String,
    pub(in crate::db) byte_len: usize,
    pub(in crate::db) raw_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ImageOptimizationProgressEvent {
    Started {
        total_rows: usize,
    },
    Scanning {
        scanned_rows: usize,
        total_rows: usize,
        compressed_rows: usize,
        skipped_rows: usize,
        conflict_count: usize,
    },
    Compacting {
        scanned_rows: usize,
        total_rows: usize,
        compressed_rows: usize,
        skipped_rows: usize,
        conflict_count: usize,
    },
    Complete {
        report: Box<ImageOptimizationReport>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResults {
    pub(in crate::db) mode_used: SearchMode,
    pub(in crate::db) hits: Vec<SearchHit>,
    pub(in crate::db) has_more: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecentResults {
    pub(in crate::db) hits: Vec<SearchHit>,
    pub(in crate::db) has_more: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineResults {
    pub(in crate::db) events: Vec<crate::model::TimelineEvent>,
    pub(in crate::db) has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatsReport {
    pub(in crate::db) snapshot_count: usize,
    pub(in crate::db) capture_event_count: usize,
    pub(in crate::db) unique_app_count: usize,
    pub(in crate::db) total_bytes: usize,
    pub(in crate::db) average_bytes_per_snapshot: f64,
    pub(in crate::db) average_captures_per_snapshot: f64,
    pub(in crate::db) dedupe_ratio: f64,
    pub(in crate::db) first_observed_at: Option<String>,
    pub(in crate::db) last_observed_at: Option<String>,
    pub(in crate::db) archive_span_seconds: Option<i64>,
    pub(in crate::db) most_recopied_snapshot: Option<StatsSnapshotLeaderboardEntry>,
    pub(in crate::db) kind_breakdown: Vec<StatsKindBreakdownEntry>,
    pub(in crate::db) top_apps: Vec<StatsAppEntry>,
    pub(in crate::db) busiest_hours: Vec<StatsTimeBucketEntry>,
    pub(in crate::db) busiest_weekdays: Vec<StatsTimeBucketEntry>,
    pub(in crate::db) largest_snapshots: Vec<StatsSnapshotLeaderboardEntry>,
    pub(in crate::db) most_captured_snapshots: Vec<StatsSnapshotLeaderboardEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatsKindBreakdownEntry {
    pub(in crate::db) kind: SnapshotKind,
    pub(in crate::db) snapshot_count: usize,
    pub(in crate::db) total_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatsAppEntry {
    pub(in crate::db) app: String,
    pub(in crate::db) capture_event_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatsTimeBucketEntry {
    pub(in crate::db) bucket: String,
    pub(in crate::db) capture_event_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatsSnapshotLeaderboardEntry {
    pub(in crate::db) snapshot_id: i64,
    pub(in crate::db) capture_count: usize,
    pub(in crate::db) kind: SnapshotKind,
    pub(in crate::db) preview_text: String,
    pub(in crate::db) app_name: Option<String>,
    pub(in crate::db) last_observed_at: String,
    pub(in crate::db) total_bytes: usize,
}

impl StatsReport {
    #[must_use]
    pub fn snapshot_count(&self) -> usize {
        self.snapshot_count
    }

    #[must_use]
    pub fn capture_event_count(&self) -> usize {
        self.capture_event_count
    }

    #[must_use]
    pub fn unique_app_count(&self) -> usize {
        self.unique_app_count
    }

    #[must_use]
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    #[must_use]
    pub fn average_bytes_per_snapshot(&self) -> f64 {
        self.average_bytes_per_snapshot
    }

    #[must_use]
    pub fn average_captures_per_snapshot(&self) -> f64 {
        self.average_captures_per_snapshot
    }

    #[must_use]
    pub fn dedupe_ratio(&self) -> f64 {
        self.dedupe_ratio
    }

    #[must_use]
    pub fn first_observed_at(&self) -> Option<&str> {
        self.first_observed_at.as_deref()
    }

    #[must_use]
    pub fn last_observed_at(&self) -> Option<&str> {
        self.last_observed_at.as_deref()
    }

    #[must_use]
    pub fn archive_span_seconds(&self) -> Option<i64> {
        self.archive_span_seconds
    }

    #[must_use]
    pub fn most_recopied_snapshot(&self) -> Option<&StatsSnapshotLeaderboardEntry> {
        self.most_recopied_snapshot.as_ref()
    }

    #[must_use]
    pub fn kind_breakdown(&self) -> &[StatsKindBreakdownEntry] {
        &self.kind_breakdown
    }

    #[must_use]
    pub fn top_apps(&self) -> &[StatsAppEntry] {
        &self.top_apps
    }

    #[must_use]
    pub fn busiest_hours(&self) -> &[StatsTimeBucketEntry] {
        &self.busiest_hours
    }

    #[must_use]
    pub fn busiest_weekdays(&self) -> &[StatsTimeBucketEntry] {
        &self.busiest_weekdays
    }

    #[must_use]
    pub fn largest_snapshots(&self) -> &[StatsSnapshotLeaderboardEntry] {
        &self.largest_snapshots
    }

    #[must_use]
    pub fn most_captured_snapshots(&self) -> &[StatsSnapshotLeaderboardEntry] {
        &self.most_captured_snapshots
    }
}

impl StatsKindBreakdownEntry {
    #[must_use]
    pub fn kind(&self) -> SnapshotKind {
        self.kind
    }

    #[must_use]
    pub fn snapshot_count(&self) -> usize {
        self.snapshot_count
    }

    #[must_use]
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }
}

impl StatsAppEntry {
    #[must_use]
    pub fn app(&self) -> &str {
        &self.app
    }

    #[must_use]
    pub fn capture_event_count(&self) -> usize {
        self.capture_event_count
    }
}

impl StatsTimeBucketEntry {
    #[must_use]
    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    #[must_use]
    pub fn capture_event_count(&self) -> usize {
        self.capture_event_count
    }
}

impl StatsSnapshotLeaderboardEntry {
    #[must_use]
    pub fn snapshot_id(&self) -> i64 {
        self.snapshot_id
    }

    #[must_use]
    pub fn capture_count(&self) -> usize {
        self.capture_count
    }

    #[must_use]
    pub fn kind(&self) -> SnapshotKind {
        self.kind
    }

    #[must_use]
    pub fn preview_text(&self) -> &str {
        &self.preview_text
    }

    #[must_use]
    pub fn app_name(&self) -> Option<&str> {
        self.app_name.as_deref()
    }

    #[must_use]
    pub fn last_observed_at(&self) -> &str {
        &self.last_observed_at
    }

    #[must_use]
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Page<T> {
    pub(in crate::db) items: Vec<T>,
    pub(in crate::db) has_more: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RecentCursorState {
    pub(in crate::db) last_seen_at: String,
    pub(in crate::db) snapshot_id: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SearchCursorState {
    pub(in crate::db) mode_used: SearchMode,
    pub(in crate::db) score: Option<f64>,
    pub(in crate::db) last_seen_at: String,
    pub(in crate::db) snapshot_id: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TimelineCursorState {
    pub(in crate::db) observed_at: String,
    pub(in crate::db) event_id: i64,
}
