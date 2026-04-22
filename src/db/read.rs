use anyhow::{Context, Result};
use rusqlite::{named_params, params, Error as SqlError, ErrorCode, OptionalExtension, Row};

use crate::model::{
    CaptureEvent, ClipboardItem, ClipboardRepresentation, DoctorReport, FlattenedTextProjection,
    SearchHit, SnapshotDetails, TimelineEvent,
};

use super::{
    collect_rows, row_enum, row_usize, sanitise_limit, usize_to_i64, Database, Page,
    RecentCursorState, RetrievalFilters, RetrievalKind, SearchCursorState, SearchMode,
    SearchResults, StatsAppEntry, StatsKindBreakdownEntry, StatsReport,
    StatsSnapshotLeaderboardEntry, StatsTimeBucketEntry, TimelineCursorState, TimelineSort,
};

mod browse;
mod mapping;
mod queries;
mod search;
mod snapshot;
mod stats;

pub(super) use self::mapping::*;
pub(super) use self::queries::*;
pub(super) use self::search::*;
pub(super) use self::stats::*;

#[cfg(test)]
mod tests;
