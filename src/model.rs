use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ClipboardSnapshot {
    pub change_count: i64,
    pub frontmost_app_bundle_id: Option<String>,
    pub frontmost_app_name: Option<String>,
    pub fingerprint: String,
    pub snapshot_kind: String,
    pub preview_text: String,
    pub search_text: String,
    pub item_count: usize,
    pub total_bytes: usize,
    pub items: Vec<ClipboardItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClipboardItem {
    pub item_index: usize,
    pub primary_kind: String,
    pub primary_uti: Option<String>,
    pub preview_text: String,
    pub search_text: String,
    pub total_bytes: usize,
    pub representations: Vec<ClipboardRepresentation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClipboardRepresentation {
    pub uti: String,
    pub classification: String,
    pub is_text: bool,
    pub byte_len: usize,
    pub raw_sha256: String,
    pub text_value: Option<String>,
    #[serde(skip_serializing)]
    pub raw_bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaptureStoreResult {
    pub snapshot_id: i64,
    pub event_id: i64,
    pub inserted_new_snapshot: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub snapshot_id: i64,
    pub sha256: String,
    pub snapshot_kind: String,
    pub preview_text: String,
    pub snippet: String,
    pub capture_count: i64,
    pub last_observed_at: String,
    pub last_frontmost_app_name: Option<String>,
    pub last_frontmost_app_bundle_id: Option<String>,
    pub total_bytes: i64,
    pub item_count: i64,
    pub score: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaptureEvent {
    pub event_id: i64,
    pub observed_at: String,
    pub change_count: i64,
    pub frontmost_app_name: Option<String>,
    pub frontmost_app_bundle_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SnapshotDetails {
    pub snapshot_id: i64,
    pub sha256: String,
    pub snapshot_kind: String,
    pub preview_text: String,
    pub search_text: String,
    pub item_count: i64,
    pub total_bytes: i64,
    pub created_at: String,
    pub capture_count: i64,
    pub first_observed_at: String,
    pub last_observed_at: String,
    pub last_frontmost_app_name: Option<String>,
    pub last_frontmost_app_bundle_id: Option<String>,
    pub recent_events: Vec<CaptureEvent>,
    pub items: Vec<ClipboardItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub db_path: String,
    pub sqlite_version: String,
    pub journal_mode: String,
    pub fts5_compile_option_present: bool,
    pub fts5_create_virtual_table_ok: bool,
    pub compile_options: Vec<String>,
}
