use serde::Serialize;

use super::{ClipboardItem, SnapshotKind};

#[derive(Debug, Clone, Serialize)]
pub struct CaptureStoreResult {
    snapshot_id: i64,
    event_id: i64,
    inserted_new_snapshot: bool,
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    snapshot_id: i64,
    event_id: i64,
    sha256: String,
    snapshot_kind: SnapshotKind,
    preview_text: String,
    why_matched: Option<String>,
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
pub struct SnapshotDetails {
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
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub(crate) fn new(
        snapshot_id: i64,
        event_id: i64,
        sha256: String,
        snapshot_kind: SnapshotKind,
        preview_text: String,
        why_matched: Option<String>,
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
    ) -> Self {
        Self {
            snapshot_id,
            event_id,
            sha256,
            snapshot_kind,
            preview_text,
            why_matched,
            capture_count,
            first_observed_at,
            last_observed_at,
            last_frontmost_app_name,
            last_frontmost_app_bundle_id,
            urls,
            file_paths,
            total_bytes,
            item_count,
            score,
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
    pub fn why_matched(&self) -> Option<&str> {
        self.why_matched.as_deref()
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
        recent_events: Vec<CaptureEvent>,
        items: Vec<ClipboardItem>,
    ) -> Self {
        Self {
            snapshot_id,
            sha256,
            snapshot_kind,
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
        self.items = items;
    }
}

impl DoctorReport {
    #[must_use]
    pub(crate) fn new(
        db_path: String,
        sqlite_version: String,
        journal_mode: String,
        fts5_compile_option_present: bool,
        fts5_create_virtual_table_ok: bool,
        compile_options: Vec<String>,
    ) -> Self {
        Self {
            db_path,
            sqlite_version,
            journal_mode,
            fts5_compile_option_present,
            fts5_create_virtual_table_ok,
            compile_options,
        }
    }

    #[must_use]
    pub fn db_path(&self) -> &str {
        &self.db_path
    }

    #[must_use]
    pub fn sqlite_version(&self) -> &str {
        &self.sqlite_version
    }

    #[must_use]
    pub fn journal_mode(&self) -> &str {
        &self.journal_mode
    }

    #[must_use]
    pub fn fts5_compile_option_present(&self) -> bool {
        self.fts5_compile_option_present
    }

    #[must_use]
    pub fn fts5_create_virtual_table_ok(&self) -> bool {
        self.fts5_create_virtual_table_ok
    }

    #[must_use]
    pub fn compile_options(&self) -> &[String] {
        &self.compile_options
    }
}
