use serde::ser::{SerializeStruct, Serializer};
use serde::Serialize;

use super::{ClipboardKind, SnapshotKind};

#[non_exhaustive]
#[derive(Debug, Clone, Serialize)]
pub struct ClipboardSnapshot {
    change_count: i64,
    frontmost_app_bundle_id: Option<String>,
    frontmost_app_name: Option<String>,
    fingerprint: String,
    snapshot_kind: SnapshotKind,
    preview_text: String,
    search_text: String,
    item_count: usize,
    total_bytes: usize,
    items: Vec<ClipboardItem>,
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
    item_index: usize,
    primary_kind: ClipboardKind,
    primary_uti: Option<String>,
    preview_text: String,
    search_text: String,
    total_bytes: usize,
    representations: Vec<ClipboardRepresentation>,
}

#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct ClipboardRepresentation {
    uti: String,
    kind: ClipboardKind,
    byte_len: usize,
    raw_sha256: String,
    text_value: Option<String>,
    raw_bytes: Vec<u8>,
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

    pub(crate) fn set_representations(&mut self, representations: Vec<ClipboardRepresentation>) {
        self.total_bytes = representations
            .iter()
            .map(ClipboardRepresentation::byte_len)
            .sum();
        self.representations = representations;
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
    pub(crate) fn new_normalized(
        capture: CaptureContext,
        fingerprint: String,
        snapshot_kind: SnapshotKind,
        preview_text: String,
        search_text: String,
        items: Vec<ClipboardItem>,
    ) -> Self {
        let (change_count, frontmost_app_bundle_id, frontmost_app_name) = capture.into_parts();
        let item_count = items.len();
        let total_bytes = items.iter().map(ClipboardItem::total_bytes).sum();

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
        let snapshot = ClipboardSnapshot::new_normalized(
            CaptureContext::new(1)
                .with_frontmost_app_name("Editor")
                .with_frontmost_app_bundle_id("com.example.Editor"),
            "fingerprint".to_string(),
            SnapshotKind::PlainText,
            "hello".to_string(),
            "hello".to_string(),
            vec![item],
        );

        assert_eq!(snapshot.total_bytes(), 5);
        assert_eq!(snapshot.items()[0].total_bytes(), 5);
        assert!(snapshot.items()[0].representations()[0].is_text());
    }

    #[test]
    fn capture_context_builder_records_frontmost_app_metadata() {
        let context = CaptureContext::new(7)
            .with_frontmost_app_name("Editor")
            .with_frontmost_app_bundle_id("com.example.Editor");

        assert_eq!(context.change_count(), 7);
        assert_eq!(context.frontmost_app_name(), Some("Editor"));
        assert_eq!(
            context.frontmost_app_bundle_id(),
            Some("com.example.Editor")
        );
    }
}
