use std::fmt::Write;
use std::io::IsTerminal;

use colored::Colorize;

use crate::db::{
    ImageOptimizationReport, OcrRunReport, OcrStatusReport, PurgeReport, SnapshotDeletionReport,
    StatsReport, StatsSnapshotLeaderboardEntry, StatsTimeBucketEntry, StorageCompactReport,
};
use crate::model::{DoctorReport, SnapshotDetails};

use super::commands::{
    CaptureOnceOutput, ExportOutput, RestoreOutput, SettingsIgnoreListOutput, SettingsView,
};
use super::output::{
    GetEnvelope, ListEnvelope, ListRow, RecallEnvelope, RecallMatchConfidence, StatsEnvelope,
};
use super::service::{ServiceProviderStatus, ServiceStatusReport};

mod actions;
mod detail;
mod fmt;
mod list;
mod stats;
mod theme;

pub(super) use self::actions::*;
pub(super) use self::detail::*;
pub(super) use self::fmt::*;
pub(super) use self::list::*;
pub(super) use self::stats::*;
pub(super) use self::theme::*;
