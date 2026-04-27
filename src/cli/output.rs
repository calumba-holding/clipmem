use std::fmt::Write;

use anyhow::{anyhow, Result};
use serde::Serialize;
use serde_json::{json, Value};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::db::{SearchResults, StatsReport, StatsSnapshotLeaderboardEntry};
use crate::model::{
    DoctorReport, FlattenedTextProjection, SearchHit, SnapshotDetails, TextFragment, TimelineEvent,
};

use super::commands::CaptureOnceOutput;
use super::human::{render_get_human, render_list_human, render_recall_human};
use super::{format_duration_seconds, peak_bucket, OutputFormat, RecallOutputFormat};

mod emit;
mod json;
mod markdown;
mod model;
mod text;
mod toon;

pub(super) use self::emit::*;
pub(super) use self::json::*;
pub(super) use self::markdown::*;
pub(super) use self::model::*;
pub(super) use self::text::*;
pub(super) use self::toon::*;

#[cfg(test)]
mod tests;
