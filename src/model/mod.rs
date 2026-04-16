mod archive;
mod clipboard;
mod kinds;

pub mod builders {
    pub use crate::clipboard::{build_item, build_representation, build_snapshot};

    pub use super::CaptureContext;
}

pub use archive::{CaptureEvent, CaptureStoreResult, DoctorReport, SearchHit, SnapshotDetails};
pub use clipboard::{CaptureContext, ClipboardItem, ClipboardRepresentation, ClipboardSnapshot};
pub use kinds::{ClipboardKind, ParseDomainValueError, SnapshotKind};
