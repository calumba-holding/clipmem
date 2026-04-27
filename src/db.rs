mod core;
mod impls;
mod read;
mod schema;
mod store;
mod types;

#[cfg(test)]
mod tests;

#[cfg(test)]
use self::core::{
    collect_rows, configure_connection, harden_existing_sqlite_sidecar_permissions, sidecar_path,
    storage_file_sizes,
};
#[cfg(test)]
use self::schema::{explain_query_plan, CURRENT_SCHEMA_VERSION, SCHEMA};
pub(crate) use self::types::{
    CapturePolicy, CaptureSettings, CaptureSkipReason, CaptureStoreOutcome,
    ImageOptimizationReport, OcrRunReport, OcrStatusReport, PurgeReport, RecentCursorState,
    SearchCursorState, SnapshotDeletionReport, StorageCompactReport, TimelineCursorState,
};
pub use self::types::{
    Database, RecentResults, RetrievalFilters, RetrievalKind, SearchMode, SearchResults,
    StatsReport, StatsSnapshotLeaderboardEntry, StatsTimeBucketEntry, TimelineResults,
    TimelineSort,
};
