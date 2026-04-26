use super::*;

pub(super) const SCHEMA: &str = include_str!("schema.sql");
pub(super) const CURRENT_SCHEMA_VERSION: i64 = 15;
pub(super) const LEGACY_PRERELEASE_COLUMNS: &[&str] = &["classification", "is_text"];

pub struct Database {
    pub(crate) conn: Connection,
    pub(in crate::db) path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum SearchMode {
    Auto,
    Fts,
    Literal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum TimelineSort {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
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
    ApiKeyFilter,
    RestoredSnapshot,
}

#[derive(Debug, Clone)]
pub(crate) enum CaptureStoreOutcome {
    Stored(crate::model::CaptureStoreResult),
    Skipped(CaptureSkipReason),
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

#[derive(Debug, Clone, Serialize)]
pub struct SearchResults {
    pub(in crate::db) mode_used: SearchMode,
    pub(in crate::db) hits: Vec<SearchHit>,
    pub(in crate::db) has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatsReport {
    pub snapshot_count: usize,
    pub capture_event_count: usize,
    pub unique_app_count: usize,
    pub total_bytes: usize,
    pub average_bytes_per_snapshot: f64,
    pub average_captures_per_snapshot: f64,
    pub dedupe_ratio: f64,
    pub first_observed_at: Option<String>,
    pub last_observed_at: Option<String>,
    pub archive_span_seconds: Option<i64>,
    pub most_recopied_snapshot: Option<StatsSnapshotLeaderboardEntry>,
    pub kind_breakdown: Vec<StatsKindBreakdownEntry>,
    pub top_apps: Vec<StatsAppEntry>,
    pub busiest_hours: Vec<StatsTimeBucketEntry>,
    pub busiest_weekdays: Vec<StatsTimeBucketEntry>,
    pub largest_snapshots: Vec<StatsSnapshotLeaderboardEntry>,
    pub most_captured_snapshots: Vec<StatsSnapshotLeaderboardEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatsKindBreakdownEntry {
    pub kind: String,
    pub snapshot_count: usize,
    pub total_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatsAppEntry {
    pub app: String,
    pub capture_event_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatsTimeBucketEntry {
    pub bucket: String,
    pub capture_event_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatsSnapshotLeaderboardEntry {
    pub snapshot_id: i64,
    pub capture_count: usize,
    pub kind: String,
    pub preview_text: String,
    pub app_name: Option<String>,
    pub last_observed_at: String,
    pub total_bytes: usize,
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
