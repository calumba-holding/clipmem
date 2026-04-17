mod archive;
mod builders;
mod clipboard;
mod kinds;
mod text_projection;

pub use archive::{
    CaptureEvent, CaptureStoreResult, DoctorReport, SearchHit, SnapshotDetails, TimelineEvent,
};
pub use builders::{build_item, build_representation, build_snapshot};
pub use clipboard::{CaptureContext, ClipboardItem, ClipboardRepresentation, ClipboardSnapshot};
pub use kinds::{ClipboardKind, ParseDomainValueError, SnapshotKind};
pub use text_projection::{FlattenedTextProjection, TextFragment};
