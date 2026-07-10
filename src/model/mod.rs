mod archive;
mod builders;
mod clipboard;
mod content_projection;
mod kinds;
mod text_projection;

pub use archive::{
    CaptureEvent, CaptureStoreResult, DoctorIntegrityReport, DoctorReport, MatchEvidence,
    SearchHit, SnapshotDetails, TimelineEvent,
};
pub(crate) use archive::{SearchHitParts, SearchOrderKey};
pub use builders::{build_item, build_representation, build_snapshot};
pub(crate) use builders::{
    decode_text_bytes_lossy, decode_text_bytes_strict, dedupe_text_fragments, hash_bytes,
    is_searchable_text_fragment, normalize_whitespace, truncate_chars,
};
pub use clipboard::{CaptureContext, ClipboardItem, ClipboardRepresentation, ClipboardSnapshot};
pub(crate) use content_projection::{html_to_text_lossy, rtf_to_text_lossy};
pub use content_projection::{ProjectionDiagnostic, TextProjectionResult};
pub use kinds::{ClipboardKind, ParseDomainValueError, SnapshotKind};
pub use text_projection::{FlattenedTextProjection, TextFragment};
