mod archive;
mod builders;
mod clipboard;
mod kinds;
mod text_projection;

pub use archive::{
    CaptureEvent, CaptureStoreResult, DoctorReport, SearchHit, SnapshotDetails, TimelineEvent,
};
pub use builders::{build_item, build_representation, build_snapshot};
pub(crate) use builders::{
    dedupe_text_fragments, hash_bytes, html_to_text_lossy, is_searchable_text_fragment,
    normalise_whitespace, rtf_to_text_lossy, truncate_chars,
};
pub use clipboard::{CaptureContext, ClipboardItem, ClipboardRepresentation, ClipboardSnapshot};
pub use kinds::{ClipboardKind, ParseDomainValueError, SnapshotKind};
pub use text_projection::{FlattenedTextProjection, TextFragment};
