use crate::model::{CaptureEvent, FlattenedTextProjection, SnapshotDetails};

use super::{
    RepresentationManifest, RepresentationPayload, RestorePayloadPlan, SnapshotItemManifest,
    SnapshotMetadata,
};

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
    pub fn snapshot_kind(&self) -> crate::model::SnapshotKind {
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
    pub fn text_fragments(&self) -> &[crate::model::TextFragment] {
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
    pub fn kind(&self) -> crate::model::ClipboardKind {
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

impl super::RepresentationDerivativePayload {
    #[must_use]
    pub fn source_raw_sha256(&self) -> &str {
        &self.source_raw_sha256
    }
    #[must_use]
    pub fn output_uti(&self) -> &str {
        &self.output_uti
    }
    #[must_use]
    pub fn encoder_version(&self) -> i64 {
        self.encoder_version
    }
    #[must_use]
    pub fn encoder_options_hash(&self) -> &str {
        &self.encoder_options_hash
    }
    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }
    #[must_use]
    pub fn height(&self) -> usize {
        self.height
    }
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl SnapshotItemManifest {
    #[must_use]
    pub fn item_index(&self) -> usize {
        self.item_index
    }
    #[must_use]
    pub fn primary_kind(&self) -> crate::model::ClipboardKind {
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
    pub fn items(&self) -> &[crate::model::ClipboardItem] {
        &self.items
    }
}
