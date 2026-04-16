use std::error::Error;
use std::fmt;
use std::str::FromStr;

use serde::ser::{SerializeStruct, Serializer};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardKind {
    PlainText,
    Url,
    FileUrl,
    Html,
    Json,
    Xml,
    Rtf,
    Pdf,
    Image,
    Binary,
    Empty,
}

impl ClipboardKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PlainText => "plain_text",
            Self::Url => "url",
            Self::FileUrl => "file_url",
            Self::Html => "html",
            Self::Json => "json",
            Self::Xml => "xml",
            Self::Rtf => "rtf",
            Self::Pdf => "pdf",
            Self::Image => "image",
            Self::Binary => "binary",
            Self::Empty => "empty",
        }
    }

    #[must_use]
    pub const fn is_textual(self) -> bool {
        matches!(
            self,
            Self::PlainText
                | Self::Url
                | Self::FileUrl
                | Self::Html
                | Self::Rtf
                | Self::Json
                | Self::Xml
        )
    }

    #[must_use]
    pub const fn priority(self) -> u8 {
        match self {
            Self::PlainText => 0,
            Self::Url => 1,
            Self::FileUrl => 2,
            Self::Html => 3,
            Self::Json => 4,
            Self::Xml => 5,
            Self::Rtf => 6,
            Self::Pdf => 7,
            Self::Image => 8,
            Self::Binary => 9,
            Self::Empty => 10,
        }
    }
}

impl fmt::Display for ClipboardKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ClipboardKind {
    type Err = ParseDomainValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "plain_text" => Ok(Self::PlainText),
            "url" => Ok(Self::Url),
            "file_url" => Ok(Self::FileUrl),
            "html" => Ok(Self::Html),
            "json" => Ok(Self::Json),
            "xml" => Ok(Self::Xml),
            "rtf" => Ok(Self::Rtf),
            "pdf" => Ok(Self::Pdf),
            "image" => Ok(Self::Image),
            "binary" => Ok(Self::Binary),
            "empty" => Ok(Self::Empty),
            _ => Err(ParseDomainValueError::new("clipboard kind", value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotKind {
    Empty,
    Mixed,
    PlainText,
    Url,
    FileUrl,
    Html,
    Json,
    Xml,
    Rtf,
    Pdf,
    Image,
    Binary,
}

impl SnapshotKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Mixed => "mixed",
            Self::PlainText => "plain_text",
            Self::Url => "url",
            Self::FileUrl => "file_url",
            Self::Html => "html",
            Self::Json => "json",
            Self::Xml => "xml",
            Self::Rtf => "rtf",
            Self::Pdf => "pdf",
            Self::Image => "image",
            Self::Binary => "binary",
        }
    }
}

impl fmt::Display for SnapshotKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl From<ClipboardKind> for SnapshotKind {
    fn from(value: ClipboardKind) -> Self {
        match value {
            ClipboardKind::PlainText => Self::PlainText,
            ClipboardKind::Url => Self::Url,
            ClipboardKind::FileUrl => Self::FileUrl,
            ClipboardKind::Html => Self::Html,
            ClipboardKind::Json => Self::Json,
            ClipboardKind::Xml => Self::Xml,
            ClipboardKind::Rtf => Self::Rtf,
            ClipboardKind::Pdf => Self::Pdf,
            ClipboardKind::Image => Self::Image,
            ClipboardKind::Binary => Self::Binary,
            ClipboardKind::Empty => Self::Empty,
        }
    }
}

impl FromStr for SnapshotKind {
    type Err = ParseDomainValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "empty" => Ok(Self::Empty),
            "mixed" => Ok(Self::Mixed),
            "plain_text" => Ok(Self::PlainText),
            "url" => Ok(Self::Url),
            "file_url" => Ok(Self::FileUrl),
            "html" => Ok(Self::Html),
            "json" => Ok(Self::Json),
            "xml" => Ok(Self::Xml),
            "rtf" => Ok(Self::Rtf),
            "pdf" => Ok(Self::Pdf),
            "image" => Ok(Self::Image),
            "binary" => Ok(Self::Binary),
            _ => Err(ParseDomainValueError::new("snapshot kind", value)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParseDomainValueError {
    domain: &'static str,
    value: String,
}

impl ParseDomainValueError {
    fn new(domain: &'static str, value: impl Into<String>) -> Self {
        Self {
            domain,
            value: value.into(),
        }
    }
}

impl fmt::Display for ParseDomainValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown {} value '{}'", self.domain, self.value)
    }
}

impl Error for ParseDomainValueError {}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize)]
pub struct ClipboardSnapshot {
    pub(crate) change_count: i64,
    pub(crate) frontmost_app_bundle_id: Option<String>,
    pub(crate) frontmost_app_name: Option<String>,
    pub(crate) fingerprint: String,
    pub(crate) snapshot_kind: SnapshotKind,
    pub(crate) preview_text: String,
    pub(crate) search_text: String,
    pub(crate) item_count: usize,
    pub(crate) total_bytes: usize,
    pub(crate) items: Vec<ClipboardItem>,
}

#[non_exhaustive]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CaptureContext {
    change_count: i64,
    frontmost_app_bundle_id: Option<String>,
    frontmost_app_name: Option<String>,
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize)]
pub struct ClipboardItem {
    pub(crate) item_index: usize,
    pub(crate) primary_kind: ClipboardKind,
    pub(crate) primary_uti: Option<String>,
    pub(crate) preview_text: String,
    pub(crate) search_text: String,
    pub(crate) total_bytes: usize,
    pub(crate) representations: Vec<ClipboardRepresentation>,
}

#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct ClipboardRepresentation {
    pub(crate) uti: String,
    pub(crate) kind: ClipboardKind,
    pub(crate) byte_len: usize,
    pub(crate) raw_sha256: String,
    pub(crate) text_value: Option<String>,
    pub(crate) raw_bytes: Vec<u8>,
}

impl Serialize for ClipboardRepresentation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("ClipboardRepresentation", 6)?;
        state.serialize_field("uti", &self.uti)?;
        state.serialize_field("kind", &self.kind)?;
        state.serialize_field("is_text", &self.is_text())?;
        state.serialize_field("byte_len", &self.byte_len)?;
        state.serialize_field("raw_sha256", &self.raw_sha256)?;
        state.serialize_field("text_value", &self.text_value)?;
        state.end()
    }
}

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

impl ClipboardItem {
    #[must_use]
    pub(crate) fn new(
        item_index: usize,
        primary_kind: ClipboardKind,
        primary_uti: Option<String>,
        preview_text: String,
        search_text: String,
        representations: Vec<ClipboardRepresentation>,
    ) -> Self {
        let total_bytes = representations.iter().map(|rep| rep.byte_len).sum();

        Self {
            item_index,
            primary_kind,
            primary_uti,
            preview_text,
            search_text,
            total_bytes,
            representations,
        }
    }

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
    pub fn representations(&self) -> &[ClipboardRepresentation] {
        &self.representations
    }
}

impl ClipboardRepresentation {
    #[must_use]
    pub(crate) fn new(
        uti: String,
        kind: ClipboardKind,
        raw_sha256: String,
        text_value: Option<String>,
        raw_bytes: Vec<u8>,
    ) -> Self {
        let byte_len = raw_bytes.len();

        Self {
            uti,
            kind,
            byte_len,
            raw_sha256,
            text_value,
            raw_bytes,
        }
    }

    #[must_use]
    pub fn is_text(&self) -> bool {
        self.text_value.is_some()
    }

    #[must_use]
    pub fn uti(&self) -> &str {
        &self.uti
    }

    #[must_use]
    pub fn kind(&self) -> ClipboardKind {
        self.kind
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

    #[must_use]
    pub fn raw_bytes(&self) -> &[u8] {
        &self.raw_bytes
    }
}

impl CaptureContext {
    #[must_use]
    pub fn new(change_count: i64) -> Self {
        Self {
            change_count,
            frontmost_app_bundle_id: None,
            frontmost_app_name: None,
        }
    }

    #[must_use]
    pub fn with_frontmost_app_name(mut self, name: impl Into<String>) -> Self {
        self.frontmost_app_name = Some(name.into());
        self
    }

    #[must_use]
    pub fn with_frontmost_app_bundle_id(mut self, bundle_id: impl Into<String>) -> Self {
        self.frontmost_app_bundle_id = Some(bundle_id.into());
        self
    }

    pub(crate) fn into_parts(self) -> (i64, Option<String>, Option<String>) {
        (
            self.change_count,
            self.frontmost_app_bundle_id,
            self.frontmost_app_name,
        )
    }
}

impl ClipboardSnapshot {
    #[must_use]
    pub fn change_count(&self) -> i64 {
        self.change_count
    }

    #[must_use]
    pub fn frontmost_app_bundle_id(&self) -> Option<&str> {
        self.frontmost_app_bundle_id.as_deref()
    }

    #[must_use]
    pub fn frontmost_app_name(&self) -> Option<&str> {
        self.frontmost_app_name.as_deref()
    }

    #[must_use]
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
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
    pub fn items(&self) -> &[ClipboardItem] {
        &self.items
    }
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

#[cfg(test)]
mod tests {
    use super::{
        CaptureContext, ClipboardItem, ClipboardKind, ClipboardRepresentation, ClipboardSnapshot,
        SnapshotKind,
    };

    #[test]
    fn raw_bytes_are_omitted_from_representation_json() {
        let representation = ClipboardRepresentation::new(
            "public.utf8-plain-text".to_string(),
            ClipboardKind::PlainText,
            "abc".to_string(),
            Some("hello".to_string()),
            b"hello".to_vec(),
        );

        let json = serde_json::to_string(&representation).unwrap();

        assert!(json.contains("\"kind\":\"plain_text\""));
        assert!(json.contains("\"is_text\":true"));
        assert!(!json.contains("raw_bytes"));
    }

    #[test]
    fn constructors_derive_lengths_and_counts_from_nested_values() {
        let representation = ClipboardRepresentation::new(
            "public.utf8-plain-text".to_string(),
            ClipboardKind::PlainText,
            "abc".to_string(),
            Some("hello".to_string()),
            b"hello".to_vec(),
        );
        let item = ClipboardItem::new(
            0,
            ClipboardKind::PlainText,
            Some("public.utf8-plain-text".to_string()),
            "hello".to_string(),
            "hello".to_string(),
            vec![representation],
        );
        let snapshot = ClipboardSnapshot {
            change_count: 1,
            frontmost_app_name: Some("Editor".to_string()),
            frontmost_app_bundle_id: Some("com.example.Editor".to_string()),
            fingerprint: "fingerprint".to_string(),
            snapshot_kind: SnapshotKind::PlainText,
            preview_text: "hello".to_string(),
            search_text: "hello".to_string(),
            item_count: 1,
            total_bytes: item.total_bytes,
            items: vec![item],
        };

        assert_eq!(snapshot.total_bytes(), 5);
        assert_eq!(snapshot.items()[0].total_bytes(), 5);
        assert!(snapshot.items()[0].representations()[0].is_text());
    }

    #[test]
    fn capture_context_builder_records_frontmost_app_metadata() {
        let context = CaptureContext::new(7)
            .with_frontmost_app_name("Editor")
            .with_frontmost_app_bundle_id("com.example.Editor");

        assert_eq!(context.change_count, 7);
        assert_eq!(context.frontmost_app_name.as_deref(), Some("Editor"));
        assert_eq!(
            context.frontmost_app_bundle_id.as_deref(),
            Some("com.example.Editor")
        );
    }
}
