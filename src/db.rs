use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::ValueEnum;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::model::SearchHit;

mod core;
mod impls;
mod read;
mod schema;
mod store;
mod types;

#[cfg(test)]
mod tests;

use self::core::*;
use self::schema::*;
pub(crate) use self::types::{
    CapturePolicy, CaptureSettings, CaptureSkipReason, CaptureStoreOutcome,
    ImageOptimizationProgressEvent, ImageOptimizationReport, OcrCandidate, OcrRunReport,
    OcrStatusReport, Page, PurgeReport, RecentCursorState, SearchCursorState,
    SnapshotDeletionReport, StorageCheckpointReport, StorageCompactReport, StorageFileSizes,
    TimelineCursorState,
};
pub use self::types::{
    Database, RetrievalFilters, RetrievalKind, SearchMode, SearchResults, StatsAppEntry,
    StatsKindBreakdownEntry, StatsReport, StatsSnapshotLeaderboardEntry, StatsTimeBucketEntry,
    TimelineSort,
};
#[cfg(test)]
use self::types::{CURRENT_SCHEMA_VERSION, SCHEMA};
