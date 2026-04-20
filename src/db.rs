mod read;
mod store;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::ValueEnum;
use rusqlite::{Connection, OpenFlags, Row};
use serde::{Deserialize, Serialize};

use crate::model::{
    dedupe_text_fragments, html_to_text_lossy, is_searchable_text_fragment, normalise_whitespace,
    rtf_to_text_lossy, truncate_chars, ClipboardKind, SearchHit,
};

const SCHEMA: &str = include_str!("db/schema.sql");
const CURRENT_SCHEMA_VERSION: i64 = 11;
const LEGACY_PRERELEASE_COLUMNS: &[&str] = &["classification", "is_text"];

pub struct Database {
    pub(super) conn: Connection,
    pub(super) path: PathBuf,
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
    since: Option<String>,
    until: Option<String>,
    hours: Option<u32>,
    app: Option<String>,
    bundle_id: Option<String>,
    kind: Option<RetrievalKind>,
    has_text: bool,
    has_url: bool,
    has_file_url: bool,
    has_image: bool,
    has_pdf: bool,
    min_bytes: Option<usize>,
    max_bytes: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) struct CaptureSettings {
    paused: bool,
    retention_seconds: Option<u64>,
    api_key_filter_enabled: bool,
    ocr_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub(crate) struct CapturePolicy {
    settings: CaptureSettings,
    ignored_bundle_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CaptureSkipReason {
    ApiKeyFilter,
}

#[derive(Debug, Clone)]
pub(crate) enum CaptureStoreOutcome {
    Stored(crate::model::CaptureStoreResult),
    Skipped(CaptureSkipReason),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SnapshotDeletionReport {
    snapshot_id: i64,
    item_count: usize,
    representation_count: usize,
    capture_event_count: usize,
    total_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PurgeReport {
    older_than_seconds: u64,
    dry_run: bool,
    snapshot_count: usize,
    item_count: usize,
    representation_count: usize,
    capture_event_count: usize,
    total_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct OcrCandidate {
    raw_sha256: String,
    blob_value: Vec<u8>,
    snapshot_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct OcrStatusReport {
    pending: usize,
    ready: usize,
    failed: usize,
    skipped: usize,
    snapshots_with_ocr_text: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct OcrRunReport {
    processed: usize,
    ready: usize,
    failed: usize,
    skipped: usize,
    remaining_pending: usize,
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
    mode_used: SearchMode,
    hits: Vec<SearchHit>,
    has_more: bool,
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
    items: Vec<T>,
    has_more: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RecentCursorState {
    last_seen_at: String,
    snapshot_id: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SearchCursorState {
    mode_used: SearchMode,
    score: Option<f64>,
    last_seen_at: String,
    snapshot_id: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TimelineCursorState {
    observed_at: String,
    event_id: i64,
}

impl SearchResults {
    #[must_use]
    pub(crate) fn new(mode_used: SearchMode, hits: Vec<SearchHit>, has_more: bool) -> Self {
        Self {
            mode_used,
            hits,
            has_more,
        }
    }

    #[must_use]
    pub fn mode_used(&self) -> SearchMode {
        self.mode_used
    }

    #[must_use]
    pub fn hits(&self) -> &[SearchHit] {
        &self.hits
    }

    #[must_use]
    pub fn has_more(&self) -> bool {
        self.has_more
    }
}

impl<T> Page<T> {
    #[must_use]
    pub(crate) fn new(items: Vec<T>, has_more: bool) -> Self {
        Self { items, has_more }
    }

    #[must_use]
    pub(crate) fn items(&self) -> &[T] {
        &self.items
    }

    #[must_use]
    pub(crate) fn into_items(self) -> Vec<T> {
        self.items
    }

    #[must_use]
    pub(crate) fn has_more(&self) -> bool {
        self.has_more
    }
}

impl RecentCursorState {
    #[must_use]
    pub(crate) fn new(last_seen_at: String, snapshot_id: i64) -> Self {
        Self {
            last_seen_at,
            snapshot_id,
        }
    }

    #[must_use]
    pub(crate) fn last_seen_at(&self) -> &str {
        &self.last_seen_at
    }

    #[must_use]
    pub(crate) fn snapshot_id(&self) -> i64 {
        self.snapshot_id
    }
}

impl SearchCursorState {
    #[must_use]
    pub(crate) fn new(
        mode_used: SearchMode,
        score: Option<f64>,
        last_seen_at: String,
        snapshot_id: i64,
    ) -> Self {
        Self {
            mode_used,
            score,
            last_seen_at,
            snapshot_id,
        }
    }

    #[must_use]
    pub(crate) fn mode_used(&self) -> SearchMode {
        self.mode_used
    }

    #[must_use]
    pub(crate) fn score(&self) -> Option<f64> {
        self.score
    }

    #[must_use]
    pub(crate) fn last_seen_at(&self) -> &str {
        &self.last_seen_at
    }

    #[must_use]
    pub(crate) fn snapshot_id(&self) -> i64 {
        self.snapshot_id
    }
}

impl SearchMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Fts => "fts",
            Self::Literal => "literal",
        }
    }
}

impl TimelineSort {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

impl RetrievalKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Html => "html",
            Self::Rtf => "rtf",
            Self::Url => "url",
            Self::File => "file",
            Self::Image => "image",
            Self::Pdf => "pdf",
            Self::Binary => "binary",
            Self::Other => "other",
        }
    }
}

impl RetrievalFilters {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        since: Option<String>,
        until: Option<String>,
        hours: Option<u32>,
        app: Option<String>,
        bundle_id: Option<String>,
        kind: Option<RetrievalKind>,
        has_text: bool,
        has_url: bool,
        has_file_url: bool,
        has_image: bool,
        has_pdf: bool,
        min_bytes: Option<usize>,
        max_bytes: Option<usize>,
    ) -> Self {
        Self {
            since,
            until,
            hours,
            app,
            bundle_id,
            kind,
            has_text,
            has_url,
            has_file_url,
            has_image,
            has_pdf,
            min_bytes,
            max_bytes,
        }
    }

    #[must_use]
    pub fn since(&self) -> Option<&str> {
        self.since.as_deref()
    }

    #[must_use]
    pub fn until(&self) -> Option<&str> {
        self.until.as_deref()
    }

    #[must_use]
    pub fn hours(&self) -> Option<u32> {
        self.hours
    }

    #[must_use]
    pub fn app(&self) -> Option<&str> {
        self.app.as_deref()
    }

    #[must_use]
    pub fn bundle_id(&self) -> Option<&str> {
        self.bundle_id.as_deref()
    }

    #[must_use]
    pub fn kind(&self) -> Option<RetrievalKind> {
        self.kind
    }

    #[must_use]
    pub fn has_text(&self) -> bool {
        self.has_text
    }

    #[must_use]
    pub fn has_url(&self) -> bool {
        self.has_url
    }

    #[must_use]
    pub fn has_file_url(&self) -> bool {
        self.has_file_url
    }

    #[must_use]
    pub fn has_image(&self) -> bool {
        self.has_image
    }

    #[must_use]
    pub fn has_pdf(&self) -> bool {
        self.has_pdf
    }

    #[must_use]
    pub fn min_bytes(&self) -> Option<usize> {
        self.min_bytes
    }

    #[must_use]
    pub fn max_bytes(&self) -> Option<usize> {
        self.max_bytes
    }
}

impl CaptureSettings {
    #[must_use]
    pub(crate) fn new(
        paused: bool,
        retention_seconds: Option<u64>,
        api_key_filter_enabled: bool,
        ocr_enabled: bool,
    ) -> Self {
        Self {
            paused,
            retention_seconds,
            api_key_filter_enabled,
            ocr_enabled,
        }
    }

    #[must_use]
    pub(crate) fn paused(&self) -> bool {
        self.paused
    }

    #[must_use]
    pub(crate) fn retention_seconds(&self) -> Option<u64> {
        self.retention_seconds
    }

    #[must_use]
    pub(crate) fn api_key_filter_enabled(&self) -> bool {
        self.api_key_filter_enabled
    }

    #[must_use]
    pub(crate) fn ocr_enabled(&self) -> bool {
        self.ocr_enabled
    }
}

impl CapturePolicy {
    #[must_use]
    pub(crate) fn new(settings: CaptureSettings, ignored_bundle_ids: Vec<String>) -> Self {
        Self {
            settings,
            ignored_bundle_ids,
        }
    }

    #[must_use]
    pub(crate) fn settings(&self) -> &CaptureSettings {
        &self.settings
    }

    #[must_use]
    pub(crate) fn ignored_bundle_ids(&self) -> &[String] {
        &self.ignored_bundle_ids
    }

    #[must_use]
    pub(crate) fn ignored_bundle_id_count(&self) -> usize {
        self.ignored_bundle_ids.len()
    }
}

impl CaptureSkipReason {
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ApiKeyFilter => "api_key_filter",
        }
    }
}

impl SnapshotDeletionReport {
    #[must_use]
    pub(crate) fn new(
        snapshot_id: i64,
        item_count: usize,
        representation_count: usize,
        capture_event_count: usize,
        total_bytes: usize,
    ) -> Self {
        Self {
            snapshot_id,
            item_count,
            representation_count,
            capture_event_count,
            total_bytes,
        }
    }

    #[must_use]
    pub(crate) fn snapshot_id(&self) -> i64 {
        self.snapshot_id
    }

    #[must_use]
    pub(crate) fn item_count(&self) -> usize {
        self.item_count
    }

    #[must_use]
    pub(crate) fn representation_count(&self) -> usize {
        self.representation_count
    }

    #[must_use]
    pub(crate) fn capture_event_count(&self) -> usize {
        self.capture_event_count
    }

    #[must_use]
    pub(crate) fn total_bytes(&self) -> usize {
        self.total_bytes
    }
}

impl PurgeReport {
    #[must_use]
    pub(crate) fn new(
        older_than_seconds: u64,
        dry_run: bool,
        snapshot_count: usize,
        item_count: usize,
        representation_count: usize,
        capture_event_count: usize,
        total_bytes: usize,
    ) -> Self {
        Self {
            older_than_seconds,
            dry_run,
            snapshot_count,
            item_count,
            representation_count,
            capture_event_count,
            total_bytes,
        }
    }

    #[must_use]
    pub(crate) fn older_than_seconds(&self) -> u64 {
        self.older_than_seconds
    }

    #[must_use]
    pub(crate) fn dry_run(&self) -> bool {
        self.dry_run
    }

    #[must_use]
    pub(crate) fn snapshot_count(&self) -> usize {
        self.snapshot_count
    }

    #[must_use]
    pub(crate) fn item_count(&self) -> usize {
        self.item_count
    }

    #[must_use]
    pub(crate) fn representation_count(&self) -> usize {
        self.representation_count
    }

    #[must_use]
    pub(crate) fn capture_event_count(&self) -> usize {
        self.capture_event_count
    }

    #[must_use]
    pub(crate) fn total_bytes(&self) -> usize {
        self.total_bytes
    }
}

impl OcrCandidate {
    #[must_use]
    pub(crate) fn new(raw_sha256: String, blob_value: Vec<u8>, snapshot_count: usize) -> Self {
        Self {
            raw_sha256,
            blob_value,
            snapshot_count,
        }
    }

    #[must_use]
    pub(crate) fn raw_sha256(&self) -> &str {
        &self.raw_sha256
    }

    #[must_use]
    pub(crate) fn blob_value(&self) -> &[u8] {
        &self.blob_value
    }
}

impl OcrStatusReport {
    #[must_use]
    pub(crate) fn new(
        pending: usize,
        ready: usize,
        failed: usize,
        skipped: usize,
        snapshots_with_ocr_text: usize,
    ) -> Self {
        Self {
            pending,
            ready,
            failed,
            skipped,
            snapshots_with_ocr_text,
        }
    }

    #[must_use]
    pub(crate) fn pending(&self) -> usize {
        self.pending
    }

    #[must_use]
    pub(crate) fn ready(&self) -> usize {
        self.ready
    }

    #[must_use]
    pub(crate) fn failed(&self) -> usize {
        self.failed
    }

    #[must_use]
    pub(crate) fn skipped(&self) -> usize {
        self.skipped
    }

    #[must_use]
    pub(crate) fn snapshots_with_ocr_text(&self) -> usize {
        self.snapshots_with_ocr_text
    }
}

impl OcrRunReport {
    #[must_use]
    pub(crate) fn new(
        processed: usize,
        ready: usize,
        failed: usize,
        skipped: usize,
        remaining_pending: usize,
    ) -> Self {
        Self {
            processed,
            ready,
            failed,
            skipped,
            remaining_pending,
        }
    }

    #[must_use]
    pub(crate) fn processed(&self) -> usize {
        self.processed
    }

    #[must_use]
    pub(crate) fn ready(&self) -> usize {
        self.ready
    }

    #[must_use]
    pub(crate) fn failed(&self) -> usize {
        self.failed
    }

    #[must_use]
    pub(crate) fn skipped(&self) -> usize {
        self.skipped
    }

    #[must_use]
    pub(crate) fn remaining_pending(&self) -> usize {
        self.remaining_pending
    }
}

impl TimelineCursorState {
    #[must_use]
    pub(crate) fn new(observed_at: String, event_id: i64) -> Self {
        Self {
            observed_at,
            event_id,
        }
    }

    #[must_use]
    pub(crate) fn observed_at(&self) -> &str {
        &self.observed_at
    }

    #[must_use]
    pub(crate) fn event_id(&self) -> i64 {
        self.event_id
    }
}

impl Database {
    /// Open the archive database at `path`, creating parent directories and schema state as needed.
    ///
    /// This method also hardens parent-directory and database-file permissions after opening.
    ///
    /// # Errors
    ///
    /// Returns an error if the parent directory cannot be created, the database cannot be opened,
    /// or the connection cannot be configured and bootstrapped.
    pub fn open_or_init(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            Context::with_context(std::fs::create_dir_all(parent), || {
                format!("failed to create {}", parent.display())
            })?;
            harden_path_permissions(parent, 0o700)?;
        }

        let mut conn = open_connection(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        prepare_connection(&mut conn)?;
        harden_path_permissions(path, 0o600)?;

        Ok(Self {
            conn,
            path: path.to_path_buf(),
        })
    }

    /// Open an existing archive database without creating parent directories or schema state.
    ///
    /// This method also hardens parent-directory and database-file permissions after opening.
    ///
    /// # Errors
    ///
    /// Returns an error if the database file does not already exist, cannot be opened, or the
    /// connection cannot be configured.
    pub fn open_existing(path: &Path) -> Result<Self> {
        let mut conn = open_connection(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        prepare_connection(&mut conn)?;
        if let Some(parent) = path.parent() {
            harden_path_permissions(parent, 0o700)?;
        }
        harden_path_permissions(path, 0o600)?;

        Ok(Self {
            conn,
            path: path.to_path_buf(),
        })
    }

    pub(crate) fn ensure_supported_schema_shape(&self) -> Result<()> {
        if legacy_prerelease_schema_detected(&self.conn)?
            || self
                .conn
                .prepare("SELECT kind FROM item_representations LIMIT 0")
                .is_err_and(|error| error.to_string().contains("no such column: kind"))
        {
            bail!(
                "database uses an incompatible prerelease schema; move it aside and run `clipmem setup` to initialize a fresh archive"
            );
        }
        Ok(())
    }

    #[must_use]
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn compact_storage(&mut self, dry_run: bool) -> Result<StorageCompactReport> {
        let before = storage_file_sizes(&self.path)?;
        let total_before_bytes = before.total_bytes();

        let checkpoint = if dry_run {
            run_wal_checkpoint(&self.conn, "PASSIVE")?
        } else {
            let _ = run_wal_checkpoint(&self.conn, "TRUNCATE")?;
            self.conn
                .execute_batch("VACUUM")
                .context("vacuum database")?;
            self.conn
                .execute_batch("PRAGMA optimize")
                .context("optimize database")?;
            run_wal_checkpoint(&self.conn, "TRUNCATE")?
        };

        let page_count = pragma_usize(&self.conn, "page_count")?;
        let freelist_count = pragma_usize(&self.conn, "freelist_count")?;
        let page_size = pragma_usize(&self.conn, "page_size")?;
        let after = storage_file_sizes(&self.path)?;
        let total_after_bytes = after.total_bytes();

        Ok(StorageCompactReport {
            db_path: self.path.display().to_string(),
            before,
            after,
            total_before_bytes,
            total_after_bytes,
            reclaimed_bytes: total_before_bytes.saturating_sub(total_after_bytes),
            estimated_reclaimable_bytes: (page_size as u64).saturating_mul(freelist_count as u64),
            page_count,
            freelist_count,
            checkpoint,
            dry_run,
            completed: !dry_run,
        })
    }

    #[cfg(test)]
    pub(crate) fn open_in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        prepare_connection(&mut conn)?;
        Ok(Self {
            conn,
            path: PathBuf::from(":memory:"),
        })
    }
}

impl StorageFileSizes {
    #[must_use]
    pub(crate) const fn total_bytes(&self) -> u64 {
        self.db + self.wal + self.shm
    }
}

fn storage_file_sizes(path: &Path) -> Result<StorageFileSizes> {
    Ok(StorageFileSizes {
        db: metadata_len(path)?,
        wal: metadata_len(&sidecar_path(path, "-wal"))?,
        shm: metadata_len(&sidecar_path(path, "-shm"))?,
    })
}

fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{suffix}", path.display()))
}

fn metadata_len(path: &Path) -> Result<u64> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error).with_context(|| format!("read size of {}", path.display())),
    }
}

fn pragma_usize(conn: &Connection, pragma: &str) -> Result<usize> {
    let sql = format!("PRAGMA {pragma}");
    conn.query_row(&sql, [], |row| row_usize(row, 0))
        .with_context(|| format!("read PRAGMA {pragma}"))
}

fn run_wal_checkpoint(conn: &Connection, mode: &str) -> Result<StorageCheckpointReport> {
    let sql = format!("PRAGMA wal_checkpoint({mode})");
    conn.query_row(&sql, [], |row| {
        Ok(StorageCheckpointReport {
            busy: row.get(0)?,
            log: row.get(1)?,
            checkpointed: row.get(2)?,
        })
    })
    .with_context(|| format!("run WAL checkpoint {mode}"))
}

pub(super) fn sanitise_limit(limit: usize) -> usize {
    limit.clamp(1, 250)
}

pub(super) fn collect_rows<T, I>(rows: I) -> Result<Vec<T>>
where
    I: Iterator<Item = rusqlite::Result<T>>,
{
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub(super) fn usize_to_i64(value: usize) -> Result<i64> {
    anyhow::Context::context(i64::try_from(value), "value exceeds SQLite INTEGER range")
}

pub(super) fn row_usize(row: &Row<'_>, index: usize) -> rusqlite::Result<usize> {
    let value = row.get::<_, i64>(index)?;
    usize::try_from(value).map_err(|source| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            Box::new(source),
        )
    })
}

pub(super) fn row_enum<T>(row: &Row<'_>, index: usize) -> rusqlite::Result<T>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    let value = row.get::<_, String>(index)?;
    value.parse().map_err(|source| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(source),
        )
    })
}

fn open_connection(path: &Path, flags: OpenFlags) -> Result<Connection> {
    anyhow::Context::with_context(Connection::open_with_flags(path, flags), || {
        format!("failed to open {}", path.display())
    })
}

fn prepare_connection(conn: &mut Connection) -> Result<()> {
    Context::context(configure_connection(conn), "configure database connection")?;
    Context::context(prepare_schema(conn), "prepare database schema")?;
    Ok(())
}

#[cfg(unix)]
fn harden_path_permissions(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    anyhow::Context::with_context(
        std::fs::set_permissions(path, PermissionsExt::from_mode(mode)),
        || format!("failed to set permissions on {}", path.display()),
    )
}

#[cfg(not(unix))]
fn harden_path_permissions(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

fn configure_connection(conn: &Connection) -> Result<()> {
    configure_pragma(conn, "journal_mode", "WAL")?;
    configure_pragma(conn, "synchronous", "NORMAL")?;
    configure_pragma(conn, "foreign_keys", "ON")?;
    configure_pragma(conn, "temp_store", "MEMORY")?;
    conn.busy_timeout(Duration::from_millis(1_500))
        .context("configure SQLite busy timeout")?;
    Ok(())
}

fn configure_pragma(conn: &Connection, pragma: &str, value: &str) -> Result<()> {
    Context::with_context(conn.pragma_update(None, pragma, value), || {
        format!("configure {pragma} pragma")
    })?;
    Ok(())
}

fn prepare_schema(conn: &mut Connection) -> Result<()> {
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .context("begin schema transaction")?;

    tx.execute_batch(SCHEMA).context("apply database schema")?;

    let user_version: i64 = tx
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .context("read PRAGMA user_version")?;

    match user_version {
        0 => {
            tx.execute(
                "INSERT INTO snapshots_fts(snapshots_fts) VALUES ('rebuild')",
                [],
            )
            .context("rebuild FTS5 index")?;
            rebuild_snapshot_stats(&tx)?;
            rebuild_snapshot_projection_cache(&tx)?;
            rebuild_snapshot_event_filter_cache(&tx)?;
            rebuild_snapshot_literal_cache(&tx)?;
            rebuild_snapshot_file_url_fts(&tx)?;
            ensure_api_key_filter_setting_column(&tx)?;
            tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
                .context("set PRAGMA user_version")?;
        }
        1 => {
            if legacy_prerelease_schema_detected(&tx)? {
                bail!(
                    "database at the current user_version uses an incompatible prerelease schema; move it aside and run `clipmem setup` to initialize a fresh archive"
                );
            }
            rebuild_snapshot_stats(&tx)?;
            rebuild_snapshot_projection_cache(&tx)?;
            rebuild_snapshot_event_filter_cache(&tx)?;
            rebuild_snapshot_literal_cache(&tx)?;
            rebuild_snapshot_file_url_fts(&tx)?;
            ensure_api_key_filter_setting_column(&tx)?;
            tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
                .context("set PRAGMA user_version")?;
        }
        2 => {
            if legacy_prerelease_schema_detected(&tx)? {
                bail!(
                    "database at the current user_version uses an incompatible prerelease schema; move it aside and run `clipmem setup` to initialize a fresh archive"
                );
            }
            rebuild_snapshot_projection_cache(&tx)?;
            rebuild_snapshot_event_filter_cache(&tx)?;
            rebuild_snapshot_literal_cache(&tx)?;
            rebuild_snapshot_file_url_fts(&tx)?;
            ensure_api_key_filter_setting_column(&tx)?;
            tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
                .context("set PRAGMA user_version")?;
        }
        3 => {
            if legacy_prerelease_schema_detected(&tx)? {
                bail!(
                    "database at the current user_version uses an incompatible prerelease schema; move it aside and run `clipmem setup` to initialize a fresh archive"
                );
            }
            rebuild_snapshot_event_filter_cache(&tx)?;
            rebuild_snapshot_literal_cache(&tx)?;
            rebuild_snapshot_file_url_fts(&tx)?;
            ensure_api_key_filter_setting_column(&tx)?;
            tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
                .context("set PRAGMA user_version")?;
        }
        4 => {
            if legacy_prerelease_schema_detected(&tx)? {
                bail!(
                    "database at the current user_version uses an incompatible prerelease schema; move it aside and run `clipmem setup` to initialize a fresh archive"
                );
            }
            rebuild_snapshot_literal_cache(&tx)?;
            rebuild_snapshot_file_url_fts(&tx)?;
            ensure_api_key_filter_setting_column(&tx)?;
            tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
                .context("set PRAGMA user_version")?;
        }
        5 => {
            if legacy_prerelease_schema_detected(&tx)? {
                bail!(
                    "database at the current user_version uses an incompatible prerelease schema; move it aside and run `clipmem setup` to initialize a fresh archive"
                );
            }
            rebuild_snapshot_file_url_fts(&tx)?;
            ensure_api_key_filter_setting_column(&tx)?;
            tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
                .context("set PRAGMA user_version")?;
        }
        6 => {
            if legacy_prerelease_schema_detected(&tx)? {
                bail!(
                    "database at the current user_version uses an incompatible prerelease schema; move it aside and run `clipmem setup` to initialize a fresh archive"
                );
            }
            ensure_api_key_filter_setting_column(&tx)?;
            tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
                .context("set PRAGMA user_version")?;
        }
        7 => {
            if legacy_prerelease_schema_detected(&tx)? {
                bail!(
                    "database at the current user_version uses an incompatible prerelease schema; move it aside and run `clipmem setup` to initialize a fresh archive"
                );
            }
            ensure_api_key_filter_setting_column(&tx)?;
            tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
                .context("set PRAGMA user_version")?;
        }
        8 => {
            if legacy_prerelease_schema_detected(&tx)? {
                bail!(
                    "database at the current user_version uses an incompatible prerelease schema; move it aside and run `clipmem setup` to initialize a fresh archive"
                );
            }
            ensure_api_key_filter_setting_column(&tx)?;
            rebuild_snapshot_text_from_representations(&tx)?;
            rebuild_snapshot_literal_cache(&tx)?;
            tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
                .context("set PRAGMA user_version")?;
        }
        9 => {
            if legacy_prerelease_schema_detected(&tx)? {
                bail!(
                    "database at the current user_version uses an incompatible prerelease schema; move it aside and run `clipmem setup` to initialize a fresh archive"
                );
            }
            ensure_api_key_filter_setting_column(&tx)?;
            ensure_ocr_enabled_setting_column(&tx)?;
            rebuild_snapshot_text_from_representations(&tx)?;
            rebuild_snapshot_literal_cache(&tx)?;
            tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
                .context("set PRAGMA user_version")?;
        }
        10 => {
            if legacy_prerelease_schema_detected(&tx)? {
                bail!(
                    "database at the current user_version uses an incompatible prerelease schema; move it aside and run `clipmem setup` to initialize a fresh archive"
                );
            }
            ensure_image_compression_columns(&tx)?;
            tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
                .context("set PRAGMA user_version")?;
        }
        CURRENT_SCHEMA_VERSION => {
            if legacy_prerelease_schema_detected(&tx)? {
                bail!(
                    "database at the current user_version uses an incompatible prerelease schema; move it aside and run `clipmem setup` to initialize a fresh archive"
                );
            }
        }
        version if version > CURRENT_SCHEMA_VERSION => {
            bail!(
                "database schema version {version} is newer than supported version {}",
                CURRENT_SCHEMA_VERSION
            );
        }
        version => {
            bail!("unsupported database schema version {version}");
        }
    }

    ensure_ocr_enabled_setting_column(&tx)?;
    ensure_image_compression_columns(&tx)?;
    tx.execute(
        "INSERT OR IGNORE INTO clipmem_settings (id, paused, retention_seconds, api_key_filter_enabled, ocr_enabled) VALUES (1, 0, NULL, 0, 0)",
        [],
    )
    .context("seed clipmem settings row")?;

    tx.commit().context("commit schema transaction")?;
    Ok(())
}

fn ensure_api_key_filter_setting_column(conn: &Connection) -> Result<()> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(clipmem_settings)")
        .context("prepare clipmem_settings table info query")?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .context("query clipmem_settings columns")?;
    let columns = collect_rows(rows).context("collect clipmem_settings columns")?;

    if columns
        .iter()
        .any(|column| column == "api_key_filter_enabled")
    {
        return Ok(());
    }

    conn.execute(
        "ALTER TABLE clipmem_settings ADD COLUMN api_key_filter_enabled INTEGER NOT NULL DEFAULT 0 CHECK (api_key_filter_enabled IN (0, 1))",
        [],
    )
    .context("add api_key_filter_enabled column")?;
    Ok(())
}

fn ensure_ocr_enabled_setting_column(conn: &Connection) -> Result<()> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(clipmem_settings)")
        .context("prepare clipmem_settings table info query")?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .context("query clipmem_settings columns")?;
    let columns = collect_rows(rows).context("collect clipmem_settings columns")?;

    if columns.iter().any(|column| column == "ocr_enabled") {
        return Ok(());
    }

    conn.execute(
        "ALTER TABLE clipmem_settings ADD COLUMN ocr_enabled INTEGER NOT NULL DEFAULT 0 CHECK (ocr_enabled IN (0, 1))",
        [],
    )
    .context("add ocr_enabled column")?;
    Ok(())
}

fn ensure_image_compression_columns(conn: &Connection) -> Result<()> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(item_representations)")
        .context("prepare item_representations table info query")?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .context("query item_representations columns")?;
    let columns = collect_rows(rows).context("collect item_representations columns")?;

    let add_column = |name: &str, sql: &str| -> Result<()> {
        if columns.iter().any(|column| column == name) {
            return Ok(());
        }
        conn.execute(sql, [])
            .with_context(|| format!("add {name} column"))?;
        Ok(())
    };

    add_column(
        "image_compression_status",
        "ALTER TABLE item_representations ADD COLUMN image_compression_status TEXT NOT NULL DEFAULT 'uncompressed' CHECK (image_compression_status IN ('uncompressed', 'compressed', 'skipped'))",
    )?;
    add_column(
        "image_compression_format",
        "ALTER TABLE item_representations ADD COLUMN image_compression_format TEXT",
    )?;
    add_column(
        "image_compressed_at",
        "ALTER TABLE item_representations ADD COLUMN image_compressed_at TEXT",
    )?;
    add_column(
        "image_original_byte_len",
        "ALTER TABLE item_representations ADD COLUMN image_original_byte_len INTEGER",
    )?;
    add_column(
        "image_original_raw_sha256",
        "ALTER TABLE item_representations ADD COLUMN image_original_raw_sha256 TEXT",
    )?;
    add_column(
        "image_compression_reason",
        "ALTER TABLE item_representations ADD COLUMN image_compression_reason TEXT",
    )?;
    Ok(())
}

fn rebuild_snapshot_stats(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM snapshot_stats", [])
        .context("clear snapshot stats")?;
    conn.execute_batch(
        r"
        INSERT INTO snapshot_stats (
            snapshot_id,
            capture_count,
            first_observed_at,
            last_observed_at,
            last_event_id,
            last_frontmost_app_bundle_id,
            last_frontmost_app_name
        )
        SELECT
            ce.snapshot_id,
            COUNT(*) AS capture_count,
            MIN(ce.observed_at) AS first_observed_at,
            MAX(ce.observed_at) AS last_observed_at,
            (
                SELECT latest.id
                FROM capture_events latest
                WHERE latest.snapshot_id = ce.snapshot_id
                ORDER BY latest.observed_at DESC, latest.id DESC
                LIMIT 1
            ) AS last_event_id,
            (
                SELECT latest.frontmost_app_bundle_id
                FROM capture_events latest
                WHERE latest.snapshot_id = ce.snapshot_id
                ORDER BY latest.observed_at DESC, latest.id DESC
                LIMIT 1
            ) AS last_frontmost_app_bundle_id,
            (
                SELECT latest.frontmost_app_name
                FROM capture_events latest
                WHERE latest.snapshot_id = ce.snapshot_id
                ORDER BY latest.observed_at DESC, latest.id DESC
                LIMIT 1
            ) AS last_frontmost_app_name
        FROM capture_events ce
        GROUP BY ce.snapshot_id;
        ",
    )
    .context("rebuild snapshot stats")?;
    Ok(())
}

fn rebuild_snapshot_projection_cache(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM snapshot_projection_cache", [])
        .context("clear snapshot projection cache")?;
    conn.execute_batch(
        r"
        WITH url_values AS (
            SELECT
                snapshot_id,
                GROUP_CONCAT(text_value, char(31)) AS urls
            FROM (
                SELECT DISTINCT snapshot_id, text_value
                FROM item_representations
                WHERE kind = 'url' AND text_value IS NOT NULL AND text_value != ''
                ORDER BY text_value
            )
            GROUP BY snapshot_id
        ),
        file_url_values AS (
            SELECT
                snapshot_id,
                GROUP_CONCAT(text_value, char(31)) AS file_urls
            FROM (
                SELECT DISTINCT snapshot_id, text_value
                FROM item_representations
                WHERE kind = 'file_url' AND text_value IS NOT NULL AND text_value != ''
                ORDER BY text_value
            )
            GROUP BY snapshot_id
        )
        INSERT INTO snapshot_projection_cache (snapshot_id, urls, file_urls)
        SELECT
            s.id,
            COALESCE(uv.urls, ''),
            COALESCE(fv.file_urls, '')
        FROM snapshots s
        LEFT JOIN url_values uv ON uv.snapshot_id = s.id
        LEFT JOIN file_url_values fv ON fv.snapshot_id = s.id;
        ",
    )
    .context("rebuild snapshot projection cache")?;
    Ok(())
}

fn rebuild_snapshot_event_filter_cache(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM snapshot_event_filter_cache", [])
        .context("clear snapshot event filter cache")?;
    conn.execute_batch(
        r"
        INSERT INTO snapshot_event_filter_cache (snapshot_id, app_names_lower, bundle_ids_lower)
        SELECT
            s.id,
            COALESCE((
                SELECT GROUP_CONCAT(app_name, char(31))
                FROM (
                    SELECT DISTINCT lower(ce.frontmost_app_name) AS app_name
                    FROM capture_events ce
                    WHERE ce.snapshot_id = s.id
                      AND ce.frontmost_app_name IS NOT NULL
                      AND ce.frontmost_app_name != ''
                    ORDER BY app_name
                )
            ), '') AS app_names_lower,
            COALESCE((
                SELECT GROUP_CONCAT(bundle_id, char(31))
                FROM (
                    SELECT DISTINCT lower(ce.frontmost_app_bundle_id) AS bundle_id
                    FROM capture_events ce
                    WHERE ce.snapshot_id = s.id
                      AND ce.frontmost_app_bundle_id IS NOT NULL
                      AND ce.frontmost_app_bundle_id != ''
                    ORDER BY bundle_id
                )
            ), '') AS bundle_ids_lower
        FROM snapshots s;
        ",
    )
    .context("rebuild snapshot event filter cache")?;
    Ok(())
}

#[derive(Debug)]
struct StoredProjectionItem {
    item_index: i64,
    representations: Vec<StoredProjectionRepresentation>,
}

#[derive(Debug)]
struct StoredProjectionRepresentation {
    uti: String,
    kind: ClipboardKind,
    byte_len: i64,
    text_value: Option<String>,
}

fn rebuild_snapshot_text_from_representations(conn: &Connection) -> Result<()> {
    let snapshot_ids = {
        let mut stmt = conn
            .prepare("SELECT id FROM snapshots ORDER BY id ASC")
            .context("prepare snapshot ids query")?;
        let rows = stmt
            .query_map([], |row| row.get::<_, i64>(0))
            .context("query snapshot ids")?;
        collect_rows(rows).context("collect snapshot ids")?
    };

    for snapshot_id in snapshot_ids {
        let items = load_stored_projection_items(conn, snapshot_id)?;
        let mut snapshot_previews = Vec::new();
        let mut snapshot_search_fragments = Vec::new();

        for item in items {
            let search_text = rebuilt_item_search_text(&item.representations);
            let preview_text = rebuilt_item_preview_text(&item.representations, &search_text);
            conn.execute(
                "UPDATE snapshot_items
                 SET primary_kind = ?1,
                     primary_uti = ?2,
                     preview_text = ?3,
                     search_text = ?4
                 WHERE snapshot_id = ?5 AND item_index = ?6",
                rusqlite::params![
                    rebuilt_primary_kind(&item.representations).as_str(),
                    rebuilt_primary_uti(&item.representations),
                    preview_text,
                    search_text,
                    snapshot_id,
                    item.item_index,
                ],
            )
            .with_context(|| {
                format!(
                    "update text projection for snapshot {snapshot_id} item {}",
                    item.item_index
                )
            })?;

            if !preview_text.trim().is_empty() {
                snapshot_previews.push(preview_text);
            }
            if !search_text.trim().is_empty() {
                snapshot_search_fragments.push(search_text);
            }
        }

        let preview_text = if snapshot_previews.is_empty() {
            "[empty clipboard]".to_string()
        } else {
            truncate_chars(&snapshot_previews.join(" | "), 280)
        };
        let search_text = snapshot_search_fragments.join("\n\n");

        conn.execute(
            "UPDATE snapshots SET preview_text = ?1, search_text = ?2 WHERE id = ?3",
            rusqlite::params![preview_text, search_text, snapshot_id],
        )
        .with_context(|| format!("update text projection for snapshot {snapshot_id}"))?;
    }

    Ok(())
}

fn load_stored_projection_items(
    conn: &Connection,
    snapshot_id: i64,
) -> Result<Vec<StoredProjectionItem>> {
    let mut stmt = conn
        .prepare(
            r"
            SELECT item_index, uti, kind, byte_len, text_value
            FROM item_representations
            WHERE snapshot_id = ?1
            ORDER BY item_index ASC, uti ASC
            ",
        )
        .context("prepare stored representation projection query")?;
    let rows = stmt
        .query_map([snapshot_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                StoredProjectionRepresentation {
                    uti: row.get(1)?,
                    kind: row_enum(row, 2)?,
                    byte_len: row.get(3)?,
                    text_value: row.get(4)?,
                },
            ))
        })
        .context("query stored representation projection rows")?;

    let mut items = Vec::<StoredProjectionItem>::new();
    for row in rows {
        let (item_index, representation) = row?;
        if let Some(item) = items
            .iter_mut()
            .find(|candidate| candidate.item_index == item_index)
        {
            item.representations.push(representation);
        } else {
            items.push(StoredProjectionItem {
                item_index,
                representations: vec![representation],
            });
        }
    }
    Ok(items)
}

fn rebuilt_primary_representation(
    representations: &[StoredProjectionRepresentation],
) -> Option<&StoredProjectionRepresentation> {
    representations.iter().min_by_key(|representation| {
        (
            representation.kind.priority(),
            !representation
                .text_value
                .as_deref()
                .is_some_and(is_searchable_text_fragment),
            representation.uti.as_str(),
        )
    })
}

fn rebuilt_primary_kind(representations: &[StoredProjectionRepresentation]) -> ClipboardKind {
    rebuilt_primary_representation(representations).map_or(ClipboardKind::Empty, |rep| rep.kind)
}

fn rebuilt_primary_uti(representations: &[StoredProjectionRepresentation]) -> Option<&str> {
    rebuilt_primary_representation(representations).map(|rep| rep.uti.as_str())
}

fn rebuilt_item_search_text(representations: &[StoredProjectionRepresentation]) -> String {
    dedupe_text_fragments(
        representations
            .iter()
            .filter_map(rebuilt_search_fragment_for_representation),
    )
    .join("\n\n")
}

fn rebuilt_search_fragment_for_representation(
    representation: &StoredProjectionRepresentation,
) -> Option<String> {
    if !representation.kind.is_textual() {
        return None;
    }

    let text = representation.text_value.as_deref()?;
    if !is_searchable_text_fragment(text) {
        return None;
    }

    let projected = match representation.kind {
        ClipboardKind::Html => html_to_text_lossy(text),
        ClipboardKind::Rtf => rtf_to_text_lossy(text),
        _ => text.to_string(),
    };
    let normalized = normalise_whitespace(&projected);
    is_searchable_text_fragment(&normalized).then_some(normalized)
}

fn rebuilt_item_preview_text(
    representations: &[StoredProjectionRepresentation],
    search_text: &str,
) -> String {
    if !search_text.is_empty() {
        return truncate_chars(&search_text.replace('\n', " "), 200);
    }

    if let Some(rep) = rebuilt_primary_representation(representations) {
        return truncate_chars(
            &format!("[{} · {} bytes · {}]", rep.kind, rep.byte_len, rep.uti),
            200,
        );
    }

    "[empty clipboard item]".to_string()
}

fn rebuild_snapshot_literal_cache(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM snapshot_literal_cache", [])
        .context("clear snapshot literal cache")?;
    conn.execute("DELETE FROM snapshots_literal_fts", [])
        .context("clear literal FTS cache")?;
    conn.execute_batch(
        r"
        INSERT INTO snapshot_literal_cache (snapshot_id, haystack)
        SELECT
            s.id,
            lower(
                COALESCE(NULLIF(s.preview_text, ''), s.search_text, '') || char(31) ||
                COALESCE(s.preview_text, '') || char(31) ||
                COALESCE(s.search_text, '') || char(31) ||
                COALESCE(sp.urls, '') || char(31) ||
                COALESCE(sp.file_urls, '') || char(31) ||
                COALESCE(ss.last_frontmost_app_name, '') || char(31) ||
                COALESCE(ss.last_frontmost_app_bundle_id, '')
            )
        FROM snapshots s
        LEFT JOIN snapshot_projection_cache sp ON sp.snapshot_id = s.id
        LEFT JOIN snapshot_stats ss ON ss.snapshot_id = s.id;
        ",
    )
    .context("rebuild snapshot literal cache")?;
    Ok(())
}

fn rebuild_snapshot_file_url_fts(conn: &Connection) -> Result<()> {
    conn.execute(
        "INSERT INTO snapshot_file_url_fts(snapshot_file_url_fts) VALUES ('rebuild')",
        [],
    )
    .context("rebuild snapshot file-url FTS")?;
    Ok(())
}

fn legacy_prerelease_schema_detected(conn: &Connection) -> Result<bool> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(item_representations)")
        .context("prepare PRAGMA table_info(item_representations)")?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .context("read item_representations columns")?;
    let columns = collect_rows(rows).context("collect item_representations columns")?;
    if columns.is_empty() {
        return Ok(false);
    }

    let has_kind = columns.iter().any(|column| column == "kind");
    let has_legacy_marker = LEGACY_PRERELEASE_COLUMNS
        .iter()
        .any(|legacy| columns.iter().any(|column| column == legacy));
    Ok(!has_kind || has_legacy_marker)
}

#[cfg(test)]
pub(crate) fn explain_query_plan(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Vec<String>> {
    let explain = format!("EXPLAIN QUERY PLAN {sql}");
    let mut stmt = conn
        .prepare(&explain)
        .context("prepare EXPLAIN QUERY PLAN")?;
    let rows = stmt
        .query_map(params, |row| row.get::<_, String>(3))
        .context("execute EXPLAIN QUERY PLAN")?;
    collect_rows(rows).context("collect EXPLAIN QUERY PLAN rows")
}
