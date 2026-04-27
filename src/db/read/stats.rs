use anyhow::{Context, Result};
use rusqlite::{named_params, Row};

use super::filter_sql::{
    app_like_pattern, can_use_snapshot_stats_for_stats, effective_since_param, event_filter_clause,
    snapshot_filter_clause,
};
use crate::db::sqlite_helpers::{collect_rows, row_enum, row_usize, usize_to_i64};
use crate::db::types::{
    Database, RetrievalFilters, RetrievalKind, StatsAppEntry, StatsKindBreakdownEntry, StatsReport,
    StatsSnapshotLeaderboardEntry, StatsTimeBucketEntry,
};

pub(in crate::db) struct StatsQueryParams {
    pub(in crate::db) since: Option<String>,
    pub(in crate::db) until: Option<String>,
    pub(in crate::db) app_like: Option<String>,
    pub(in crate::db) bundle_id: Option<String>,
    pub(in crate::db) kind: Option<&'static str>,
    pub(in crate::db) has_text: bool,
    pub(in crate::db) has_url: bool,
    pub(in crate::db) has_file_url: bool,
    pub(in crate::db) has_image: bool,
    pub(in crate::db) has_pdf: bool,
    pub(in crate::db) min_bytes: Option<i64>,
    pub(in crate::db) max_bytes: Option<i64>,
}

pub(in crate::db) fn stats_params(filters: &RetrievalFilters) -> Result<StatsQueryParams> {
    Ok(StatsQueryParams {
        since: effective_since_param(filters)?,
        until: filters.until().map(ToOwned::to_owned),
        app_like: app_like_pattern(filters),
        bundle_id: filters.bundle_id().map(|value| value.to_ascii_lowercase()),
        kind: filters.kind().map(RetrievalKind::as_str),
        has_text: filters.has_text(),
        has_url: filters.has_url(),
        has_file_url: filters.has_file_url(),
        has_image: filters.has_image(),
        has_pdf: filters.has_pdf(),
        min_bytes: filters.min_bytes().map(usize_to_i64).transpose()?,
        max_bytes: filters.max_bytes().map(usize_to_i64).transpose()?,
    })
}

#[derive(Debug)]
pub(in crate::db) struct StatsOverview {
    pub(in crate::db) snapshot_count: usize,
    pub(in crate::db) capture_event_count: usize,
    pub(in crate::db) unique_app_count: usize,
    pub(in crate::db) total_bytes: usize,
    pub(in crate::db) first_observed_at: Option<String>,
    pub(in crate::db) last_observed_at: Option<String>,
    pub(in crate::db) archive_span_seconds: Option<i64>,
}

impl StatsOverview {
    pub(in crate::db) fn average_bytes_per_snapshot(&self) -> f64 {
        if self.snapshot_count == 0 {
            0.0
        } else {
            self.total_bytes as f64 / self.snapshot_count as f64
        }
    }

    pub(in crate::db) fn average_captures_per_snapshot(&self) -> f64 {
        if self.snapshot_count == 0 {
            0.0
        } else {
            self.capture_event_count as f64 / self.snapshot_count as f64
        }
    }

    pub(in crate::db) fn dedupe_ratio(&self) -> f64 {
        if self.capture_event_count == 0 {
            0.0
        } else {
            1.0 - (self.snapshot_count as f64 / self.capture_event_count as f64)
        }
    }
}

fn weekday_name(index: usize) -> &'static str {
    match index {
        0 => "Sunday",
        1 => "Monday",
        2 => "Tuesday",
        3 => "Wednesday",
        4 => "Thursday",
        5 => "Friday",
        6 => "Saturday",
        _ => "Unknown",
    }
}

#[derive(Debug, Clone, Copy)]
pub(in crate::db) enum StatsReadSource {
    Unfiltered,
    Filtered,
}

impl StatsReadSource {
    fn label(self) -> &'static str {
        match self {
            Self::Unfiltered => "unfiltered",
            Self::Filtered => "filtered",
        }
    }

    fn snapshots(self) -> StatsSnapshotSource {
        match self {
            Self::Unfiltered => StatsSnapshotSource {
                count_from: "snapshot_stats",
                from: "snapshots s JOIN snapshot_stats ss ON ss.snapshot_id = s.id",
                id: "s.id",
                kind: "s.snapshot_kind",
                preview_text: "s.preview_text",
                total_bytes: "s.total_bytes",
                capture_count: "ss.capture_count",
                app_name: "ss.last_frontmost_app_name",
                app_bundle_id: "ss.last_frontmost_app_bundle_id",
                last_observed_at: "ss.last_observed_at",
            },
            Self::Filtered => StatsSnapshotSource {
                count_from: "clipmem_stats_matching_snapshots",
                from: "clipmem_stats_matching_snapshots s",
                id: "s.snapshot_id",
                kind: "s.kind",
                preview_text: "s.preview_text",
                total_bytes: "s.total_bytes",
                capture_count: "s.capture_count",
                app_name: "s.last_frontmost_app_name",
                app_bundle_id: "s.last_frontmost_app_bundle_id",
                last_observed_at: "s.last_observed_at",
            },
        }
    }

    fn events(self) -> StatsEventSource {
        match self {
            Self::Unfiltered => StatsEventSource {
                from: "capture_events e",
                observed_at: "e.observed_at",
                app_name: "e.frontmost_app_name",
                app_bundle_id: "e.frontmost_app_bundle_id",
            },
            Self::Filtered => StatsEventSource {
                from: "clipmem_stats_matching_events e",
                observed_at: "e.observed_at",
                app_name: "e.frontmost_app_name",
                app_bundle_id: "e.frontmost_app_bundle_id",
            },
        }
    }

    fn observation_range(self) -> StatsObservationRangeSource {
        match self {
            Self::Unfiltered => StatsObservationRangeSource {
                from: "snapshot_stats o",
                first_observed_at: "o.first_observed_at",
                last_observed_at: "o.last_observed_at",
            },
            Self::Filtered => StatsObservationRangeSource {
                from: "clipmem_stats_matching_events o",
                first_observed_at: "o.observed_at",
                last_observed_at: "o.observed_at",
            },
        }
    }

    fn snapshot_leaderboard_order_by(
        self,
        ordering: StatsSnapshotLeaderboardOrdering,
    ) -> &'static str {
        match (self, ordering) {
            (Self::Unfiltered, StatsSnapshotLeaderboardOrdering::TotalBytes) => {
                "s.total_bytes DESC, s.id ASC"
            }
            (Self::Unfiltered, StatsSnapshotLeaderboardOrdering::CaptureCount) => {
                "ss.capture_count DESC, s.id ASC"
            }
            (Self::Filtered, StatsSnapshotLeaderboardOrdering::TotalBytes) => {
                "s.total_bytes DESC, s.snapshot_id ASC"
            }
            (Self::Filtered, StatsSnapshotLeaderboardOrdering::CaptureCount) => {
                "s.capture_count DESC, s.snapshot_id ASC"
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct StatsSnapshotSource {
    count_from: &'static str,
    from: &'static str,
    id: &'static str,
    kind: &'static str,
    preview_text: &'static str,
    total_bytes: &'static str,
    capture_count: &'static str,
    app_name: &'static str,
    app_bundle_id: &'static str,
    last_observed_at: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct StatsEventSource {
    from: &'static str,
    observed_at: &'static str,
    app_name: &'static str,
    app_bundle_id: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct StatsObservationRangeSource {
    from: &'static str,
    first_observed_at: &'static str,
    last_observed_at: &'static str,
}

#[derive(Debug, Clone, Copy)]
enum StatsSnapshotLeaderboardOrdering {
    TotalBytes,
    CaptureCount,
}

fn stats_overview_from_row(row: &Row<'_>) -> rusqlite::Result<StatsOverview> {
    Ok(StatsOverview {
        snapshot_count: row_usize(row, 0)?,
        capture_event_count: row_usize(row, 1)?,
        unique_app_count: row_usize(row, 2)?,
        total_bytes: row_usize(row, 3)?,
        first_observed_at: row.get(4)?,
        last_observed_at: row.get(5)?,
        archive_span_seconds: row.get(6)?,
    })
}

fn stats_kind_breakdown_entry_from_row(row: &Row<'_>) -> rusqlite::Result<StatsKindBreakdownEntry> {
    Ok(StatsKindBreakdownEntry {
        kind: row_enum(row, 0)?,
        snapshot_count: row_usize(row, 1)?,
        total_bytes: row_usize(row, 2)?,
    })
}

fn stats_app_entry_from_row(row: &Row<'_>) -> rusqlite::Result<StatsAppEntry> {
    Ok(StatsAppEntry {
        app: row.get(0)?,
        capture_event_count: row_usize(row, 1)?,
    })
}

fn stats_snapshot_leaderboard_entry_from_row(
    row: &Row<'_>,
) -> rusqlite::Result<StatsSnapshotLeaderboardEntry> {
    let app_name: Option<String> = row.get(4)?;
    let app_bundle_id: Option<String> = row.get(5)?;
    Ok(StatsSnapshotLeaderboardEntry {
        snapshot_id: row.get(0)?,
        capture_count: row_usize(row, 1)?,
        kind: row_enum(row, 2)?,
        preview_text: row.get(3)?,
        app_name: app_name.or(app_bundle_id),
        last_observed_at: row.get(6)?,
        total_bytes: row_usize(row, 7)?,
    })
}

impl Database {
    pub(in crate::db) fn reset_stats_temp_tables(&self) -> Result<()> {
        self.conn
            .execute_batch(
                "DROP TABLE IF EXISTS temp.clipmem_stats_matching_events;
                 DROP TABLE IF EXISTS temp.clipmem_stats_matching_snapshots;",
            )
            .context("clear stats temp tables")
    }

    pub(in crate::db) fn materialize_stats_matching_events(
        &self,
        params: &StatsQueryParams,
    ) -> Result<()> {
        self.conn
            .execute(
                &format!(
                    "CREATE TEMP TABLE clipmem_stats_matching_events AS
                     SELECT ce.id,
                            ce.snapshot_id,
                            ce.observed_at,
                            ce.frontmost_app_name,
                            ce.frontmost_app_bundle_id
                     FROM capture_events ce
                     JOIN snapshots s ON s.id = ce.snapshot_id
                     WHERE {event_filter_clause}
                       AND {snapshot_filter_clause}",
                    event_filter_clause = event_filter_clause("ce"),
                    snapshot_filter_clause = snapshot_filter_clause("s", "ce.snapshot_id"),
                ),
                named_params! {
                    ":since": params.since.as_deref(),
                    ":until": params.until.as_deref(),
                    ":app_like": params.app_like.as_deref(),
                    ":bundle_id": params.bundle_id.as_deref(),
                    ":kind": params.kind,
                    ":has_text": params.has_text,
                    ":has_url": params.has_url,
                    ":has_file_url": params.has_file_url,
                    ":has_image": params.has_image,
                    ":has_pdf": params.has_pdf,
                    ":min_bytes": params.min_bytes,
                    ":max_bytes": params.max_bytes,
                },
            )
            .context("materialize stats matching events")?;
        Ok(())
    }

    pub(in crate::db) fn index_stats_matching_events(&self) -> Result<()> {
        self.conn
            .execute_batch(
                "CREATE INDEX temp.idx_clipmem_stats_events_snapshot_id
                    ON clipmem_stats_matching_events(snapshot_id);
                 CREATE INDEX temp.idx_clipmem_stats_events_observed_at
                    ON clipmem_stats_matching_events(observed_at);",
            )
            .context("index stats matching events")
    }

    pub(in crate::db) fn materialize_stats_matching_snapshots(
        &self,
        filters: &RetrievalFilters,
        params: &StatsQueryParams,
    ) -> Result<()> {
        if can_use_snapshot_stats_for_stats(filters) {
            self.materialize_stats_matching_snapshots_from_stats(params)
        } else {
            self.materialize_stats_matching_snapshots_from_events(params)
        }
    }

    fn materialize_stats_matching_snapshots_from_stats(
        &self,
        params: &StatsQueryParams,
    ) -> Result<()> {
        self.conn
            .execute(
                &format!(
                    "CREATE TEMP TABLE clipmem_stats_matching_snapshots AS
                     SELECT
                         s.id AS snapshot_id,
                         s.snapshot_kind AS kind,
                         s.preview_text AS preview_text,
                         s.total_bytes AS total_bytes,
                         ss.capture_count AS capture_count,
                         ss.first_observed_at AS first_observed_at,
                         ss.last_observed_at AS last_observed_at,
                         ss.last_frontmost_app_name AS last_frontmost_app_name,
                         ss.last_frontmost_app_bundle_id AS last_frontmost_app_bundle_id
                     FROM snapshots s
                     JOIN snapshot_stats ss ON ss.snapshot_id = s.id
                     WHERE {snapshot_filter_clause}",
                    snapshot_filter_clause = snapshot_filter_clause("s", "s.id"),
                ),
                named_params! {
                    ":kind": params.kind,
                    ":has_text": params.has_text,
                    ":has_url": params.has_url,
                    ":has_file_url": params.has_file_url,
                    ":has_image": params.has_image,
                    ":has_pdf": params.has_pdf,
                    ":min_bytes": params.min_bytes,
                    ":max_bytes": params.max_bytes,
                },
            )
            .context("materialize stats matching snapshots from snapshot stats")?;
        Ok(())
    }

    fn materialize_stats_matching_snapshots_from_events(
        &self,
        params: &StatsQueryParams,
    ) -> Result<()> {
        self.conn
            .execute(
                &format!(
                    "CREATE TEMP TABLE clipmem_stats_matching_snapshots AS
                     SELECT
                         s.id AS snapshot_id,
                         s.snapshot_kind AS kind,
                         s.preview_text AS preview_text,
                         s.total_bytes AS total_bytes,
                         COUNT(me.id) AS capture_count,
                         MIN(me.observed_at) AS first_observed_at,
                         MAX(me.observed_at) AS last_observed_at,
                         (
                             SELECT me2.frontmost_app_name
                             FROM clipmem_stats_matching_events me2
                             WHERE me2.snapshot_id = s.id
                             ORDER BY me2.observed_at DESC, me2.id DESC
                             LIMIT 1
                         ) AS last_frontmost_app_name,
                         (
                             SELECT me2.frontmost_app_bundle_id
                             FROM clipmem_stats_matching_events me2
                             WHERE me2.snapshot_id = s.id
                             ORDER BY me2.observed_at DESC, me2.id DESC
                             LIMIT 1
                         ) AS last_frontmost_app_bundle_id
                     FROM snapshots s
                     JOIN clipmem_stats_matching_events me ON me.snapshot_id = s.id
                     WHERE {snapshot_filter_clause}
                     GROUP BY s.id",
                    snapshot_filter_clause = snapshot_filter_clause("s", "s.id"),
                ),
                named_params! {
                    ":kind": params.kind,
                    ":has_text": params.has_text,
                    ":has_url": params.has_url,
                    ":has_file_url": params.has_file_url,
                    ":has_image": params.has_image,
                    ":has_pdf": params.has_pdf,
                    ":min_bytes": params.min_bytes,
                    ":max_bytes": params.max_bytes,
                },
            )
            .context("materialize stats matching snapshots")?;
        Ok(())
    }

    pub(in crate::db) fn index_stats_matching_snapshots(&self) -> Result<()> {
        self.conn
            .execute_batch(
                "CREATE INDEX temp.idx_clipmem_stats_snapshots_capture_count
                    ON clipmem_stats_matching_snapshots(capture_count DESC, snapshot_id ASC);
                 CREATE INDEX temp.idx_clipmem_stats_snapshots_bytes
                    ON clipmem_stats_matching_snapshots(total_bytes DESC, snapshot_id ASC);",
            )
            .context("index stats matching snapshots")
    }

    pub(in crate::db) fn load_stats_report(&self, source: StatsReadSource) -> Result<StatsReport> {
        let overview = self.load_stats_overview(source)?;
        let kind_breakdown = self.load_stats_kind_breakdown(source)?;
        let top_apps = self.load_stats_top_apps(source)?;
        let busiest_hours = self.load_stats_hours(source)?;
        let busiest_weekdays = self.load_stats_weekdays(source)?;
        let largest_snapshots = self.load_stats_snapshot_leaderboard(
            source,
            StatsSnapshotLeaderboardOrdering::TotalBytes,
            5,
        )?;
        let most_captured_snapshots = self.load_stats_snapshot_leaderboard(
            source,
            StatsSnapshotLeaderboardOrdering::CaptureCount,
            5,
        )?;
        let most_recopied_snapshot = most_captured_snapshots.first().cloned();

        Ok(StatsReport {
            snapshot_count: overview.snapshot_count,
            capture_event_count: overview.capture_event_count,
            unique_app_count: overview.unique_app_count,
            total_bytes: overview.total_bytes,
            average_bytes_per_snapshot: overview.average_bytes_per_snapshot(),
            average_captures_per_snapshot: overview.average_captures_per_snapshot(),
            dedupe_ratio: overview.dedupe_ratio(),
            first_observed_at: overview.first_observed_at,
            last_observed_at: overview.last_observed_at,
            archive_span_seconds: overview.archive_span_seconds,
            most_recopied_snapshot,
            kind_breakdown,
            top_apps,
            busiest_hours,
            busiest_weekdays,
            largest_snapshots,
            most_captured_snapshots,
        })
    }

    fn load_stats_overview(&self, source: StatsReadSource) -> Result<StatsOverview> {
        let snapshots = source.snapshots();
        let events = source.events();
        let observations = source.observation_range();
        let sql = format!(
            "SELECT
                 (SELECT COUNT(*) FROM {snapshot_count_from}) AS snapshot_count,
                 (SELECT COUNT(*) FROM {events_from}) AS capture_event_count,
                 (
                     SELECT COUNT(DISTINCT COALESCE(NULLIF({event_app_name}, ''), NULLIF({event_app_bundle_id}, ''), 'Unknown'))
                     FROM {events_from}
                 ) AS unique_app_count,
                 COALESCE((
                     SELECT SUM({snapshot_total_bytes})
                     FROM {snapshots_from}
                 ), 0) AS total_bytes,
                 (SELECT MIN({first_observed_at}) FROM {observations_from}) AS first_observed_at,
                 (SELECT MAX({last_observed_at}) FROM {observations_from}) AS last_observed_at,
                 (
                     SELECT CAST(strftime('%s', MAX({last_observed_at})) AS INTEGER) - CAST(strftime('%s', MIN({first_observed_at})) AS INTEGER)
                     FROM {observations_from}
                 ) AS archive_span_seconds",
            snapshot_count_from = snapshots.count_from,
            events_from = events.from,
            event_app_name = events.app_name,
            event_app_bundle_id = events.app_bundle_id,
            snapshots_from = snapshots.from,
            snapshot_total_bytes = snapshots.total_bytes,
            observations_from = observations.from,
            first_observed_at = observations.first_observed_at,
            last_observed_at = observations.last_observed_at,
        );
        self.conn
            .query_row(&sql, [], stats_overview_from_row)
            .with_context(|| format!("load {} stats overview", source.label()))
    }

    fn load_stats_kind_breakdown(
        &self,
        source: StatsReadSource,
    ) -> Result<Vec<StatsKindBreakdownEntry>> {
        let snapshots = source.snapshots();
        let sql = format!(
            "SELECT
                 {kind} AS kind,
                 COUNT(*) AS snapshot_count,
                 COALESCE(SUM({total_bytes}), 0) AS total_bytes
             FROM {snapshots_from}
             GROUP BY {kind}
             ORDER BY snapshot_count DESC, kind ASC",
            kind = snapshots.kind,
            total_bytes = snapshots.total_bytes,
            snapshots_from = snapshots.from,
        );
        let mut stmt = self
            .conn
            .prepare(&sql)
            .with_context(|| format!("prepare {} stats kind breakdown", source.label()))?;
        let rows = stmt
            .query_map([], stats_kind_breakdown_entry_from_row)
            .with_context(|| format!("execute {} stats kind breakdown", source.label()))?;
        collect_rows(rows)
            .with_context(|| format!("collect {} stats kind breakdown", source.label()))
    }

    fn load_stats_top_apps(&self, source: StatsReadSource) -> Result<Vec<StatsAppEntry>> {
        let events = source.events();
        let sql = format!(
            "SELECT
                 COALESCE(NULLIF({app_name}, ''), NULLIF({app_bundle_id}, ''), 'Unknown') AS app,
                 COUNT(*) AS capture_event_count
             FROM {events_from}
             GROUP BY app
             ORDER BY capture_event_count DESC, app ASC
             LIMIT 5",
            app_name = events.app_name,
            app_bundle_id = events.app_bundle_id,
            events_from = events.from,
        );
        let mut stmt = self
            .conn
            .prepare(&sql)
            .with_context(|| format!("prepare {} stats top apps", source.label()))?;
        let rows = stmt
            .query_map([], stats_app_entry_from_row)
            .with_context(|| format!("execute {} stats top apps", source.label()))?;
        collect_rows(rows).with_context(|| format!("collect {} stats top apps", source.label()))
    }

    fn load_stats_hours(&self, source: StatsReadSource) -> Result<Vec<StatsTimeBucketEntry>> {
        self.load_stats_time_distribution(source, "00", 24, "%H", |index| format!("{index:02}"))
    }

    fn load_stats_weekdays(&self, source: StatsReadSource) -> Result<Vec<StatsTimeBucketEntry>> {
        self.load_stats_time_distribution(source, "0", 7, "%w", |index| {
            weekday_name(index).to_string()
        })
    }

    fn load_stats_time_distribution(
        &self,
        source: StatsReadSource,
        first_value: &str,
        count: usize,
        strftime_format: &str,
        render_bucket: impl Fn(usize) -> String,
    ) -> Result<Vec<StatsTimeBucketEntry>> {
        let events = source.events();
        let sql = format!(
            "WITH RECURSIVE buckets(value) AS (
                 SELECT CAST(:first_value AS INTEGER)
                 UNION ALL
                 SELECT value + 1 FROM buckets WHERE value + 1 < :count
             ),
             counts AS (
                 SELECT CAST(strftime('{strftime_format}', {observed_at}) AS INTEGER) AS value,
                        COUNT(*) AS capture_event_count
                 FROM {events_from}
                 GROUP BY value
             )
             SELECT buckets.value, COALESCE(counts.capture_event_count, 0)
             FROM buckets
             LEFT JOIN counts ON counts.value = buckets.value
             ORDER BY buckets.value ASC",
            strftime_format = strftime_format,
            observed_at = events.observed_at,
            events_from = events.from,
        );
        let mut stmt = self
            .conn
            .prepare(&sql)
            .with_context(|| format!("prepare {} stats time distribution", source.label()))?;
        let rows = stmt
            .query_map(
                named_params! {
                    ":first_value": first_value,
                    ":count": usize_to_i64(count)?,
                },
                |row| {
                    let index = row_usize(row, 0)?;
                    Ok(StatsTimeBucketEntry {
                        bucket: render_bucket(index),
                        capture_event_count: row_usize(row, 1)?,
                    })
                },
            )
            .with_context(|| format!("execute {} stats time distribution", source.label()))?;
        collect_rows(rows)
            .with_context(|| format!("collect {} stats time distribution", source.label()))
    }

    fn load_stats_snapshot_leaderboard(
        &self,
        source: StatsReadSource,
        ordering: StatsSnapshotLeaderboardOrdering,
        limit: usize,
    ) -> Result<Vec<StatsSnapshotLeaderboardEntry>> {
        let snapshots = source.snapshots();
        let sql = format!(
            "SELECT
                 {snapshot_id} AS snapshot_id,
                 {capture_count} AS capture_count,
                 {kind} AS kind,
                 {preview_text} AS preview_text,
                 {app_name} AS app_name,
                 {app_bundle_id} AS app_bundle_id,
                 {last_observed_at} AS last_observed_at,
                 {total_bytes} AS total_bytes
             FROM {snapshots_from}
             ORDER BY {ordering}
             LIMIT :limit",
            snapshot_id = snapshots.id,
            capture_count = snapshots.capture_count,
            kind = snapshots.kind,
            preview_text = snapshots.preview_text,
            app_name = snapshots.app_name,
            app_bundle_id = snapshots.app_bundle_id,
            last_observed_at = snapshots.last_observed_at,
            total_bytes = snapshots.total_bytes,
            snapshots_from = snapshots.from,
            ordering = source.snapshot_leaderboard_order_by(ordering),
        );
        let mut stmt = self
            .conn
            .prepare(&sql)
            .with_context(|| format!("prepare {} stats snapshot leaderboard", source.label()))?;
        let rows = stmt
            .query_map(
                named_params! { ":limit": usize_to_i64(limit)? },
                stats_snapshot_leaderboard_entry_from_row,
            )
            .with_context(|| format!("execute {} stats snapshot leaderboard", source.label()))?;
        collect_rows(rows)
            .with_context(|| format!("collect {} stats snapshot leaderboard", source.label()))
    }
}
