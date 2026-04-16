use serde::Serialize;

use super::{ClipboardItem, SnapshotKind};

#[derive(Debug, Clone, Serialize)]
pub struct CaptureStoreResult {
    pub(crate) snapshot_id: i64,
    pub(crate) event_id: i64,
    pub(crate) inserted_new_snapshot: bool,
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub(crate) snapshot_id: i64,
    pub(crate) sha256: String,
    pub(crate) snapshot_kind: SnapshotKind,
    pub(crate) preview_text: String,
    pub(crate) snippet: String,
    pub(crate) capture_count: usize,
    pub(crate) last_observed_at: String,
    pub(crate) last_frontmost_app_name: Option<String>,
    pub(crate) last_frontmost_app_bundle_id: Option<String>,
    pub(crate) total_bytes: usize,
    pub(crate) item_count: usize,
    pub(crate) score: Option<f64>,
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize)]
pub struct CaptureEvent {
    pub(crate) event_id: i64,
    pub(crate) observed_at: String,
    pub(crate) change_count: i64,
    pub(crate) frontmost_app_name: Option<String>,
    pub(crate) frontmost_app_bundle_id: Option<String>,
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize)]
pub struct SnapshotDetails {
    pub(crate) snapshot_id: i64,
    pub(crate) sha256: String,
    pub(crate) snapshot_kind: SnapshotKind,
    pub(crate) preview_text: String,
    pub(crate) search_text: String,
    pub(crate) item_count: usize,
    pub(crate) total_bytes: usize,
    pub(crate) created_at: String,
    pub(crate) capture_count: usize,
    pub(crate) first_observed_at: String,
    pub(crate) last_observed_at: String,
    pub(crate) last_frontmost_app_name: Option<String>,
    pub(crate) last_frontmost_app_bundle_id: Option<String>,
    pub(crate) recent_events: Vec<CaptureEvent>,
    pub(crate) items: Vec<ClipboardItem>,
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub(crate) db_path: String,
    pub(crate) sqlite_version: String,
    pub(crate) journal_mode: String,
    pub(crate) fts5_compile_option_present: bool,
    pub(crate) fts5_create_virtual_table_ok: bool,
    pub(crate) compile_options: Vec<String>,
}

impl CaptureStoreResult {
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
    pub fn snippet(&self) -> &str {
        &self.snippet
    }

    #[must_use]
    pub fn capture_count(&self) -> usize {
        self.capture_count
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
}

impl DoctorReport {
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
