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

#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct ClipboardSnapshot {
    pub change_count: i64,
    pub frontmost_app_bundle_id: Option<String>,
    pub frontmost_app_name: Option<String>,
    pub fingerprint: String,
    pub snapshot_kind: SnapshotKind,
    pub preview_text: String,
    pub search_text: String,
    pub item_count: usize,
    pub total_bytes: usize,
    pub items: Vec<ClipboardItem>,
}

#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct ClipboardItem {
    pub item_index: usize,
    pub primary_kind: ClipboardKind,
    pub primary_uti: Option<String>,
    pub preview_text: String,
    pub search_text: String,
    pub total_bytes: usize,
    pub representations: Vec<ClipboardRepresentation>,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ClipboardRepresentation {
    pub uti: String,
    pub kind: ClipboardKind,
    pub byte_len: usize,
    pub raw_sha256: String,
    pub text_value: Option<String>,
    pub raw_bytes: Vec<u8>,
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
    pub snapshot_id: i64,
    pub event_id: i64,
    pub inserted_new_snapshot: bool,
}

#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct SearchHit {
    pub snapshot_id: i64,
    pub sha256: String,
    pub snapshot_kind: SnapshotKind,
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
#[non_exhaustive]
pub struct CaptureEvent {
    pub event_id: i64,
    pub observed_at: String,
    pub change_count: i64,
    pub frontmost_app_name: Option<String>,
    pub frontmost_app_bundle_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct SnapshotDetails {
    pub snapshot_id: i64,
    pub sha256: String,
    pub snapshot_kind: SnapshotKind,
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
#[non_exhaustive]
pub struct DoctorReport {
    pub db_path: String,
    pub sqlite_version: String,
    pub journal_mode: String,
    pub fts5_compile_option_present: bool,
    pub fts5_create_virtual_table_ok: bool,
    pub compile_options: Vec<String>,
}

impl ClipboardSnapshot {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        change_count: i64,
        frontmost_app_name: Option<String>,
        frontmost_app_bundle_id: Option<String>,
        fingerprint: String,
        snapshot_kind: SnapshotKind,
        preview_text: String,
        search_text: String,
        items: Vec<ClipboardItem>,
    ) -> Self {
        let item_count = items.len();
        let total_bytes = items.iter().map(|item| item.total_bytes).sum();

        Self {
            change_count,
            frontmost_app_bundle_id,
            frontmost_app_name,
            fingerprint,
            snapshot_kind,
            preview_text,
            search_text,
            item_count,
            total_bytes,
            items,
        }
    }
}

impl ClipboardItem {
    #[must_use]
    pub fn new(
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
}

impl ClipboardRepresentation {
    #[must_use]
    pub fn new(
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
}

#[cfg(test)]
mod tests {
    use super::{
        ClipboardItem, ClipboardKind, ClipboardRepresentation, ClipboardSnapshot, SnapshotKind,
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
        let snapshot = ClipboardSnapshot::new(
            1,
            Some("Editor".to_string()),
            Some("com.example.Editor".to_string()),
            "fingerprint".to_string(),
            SnapshotKind::PlainText,
            "hello".to_string(),
            "hello".to_string(),
            vec![item],
        );

        assert_eq!(snapshot.item_count, 1);
        assert_eq!(snapshot.total_bytes, 5);
        assert_eq!(snapshot.items[0].total_bytes, 5);
        assert!(snapshot.items[0].representations[0].is_text());
    }
}
