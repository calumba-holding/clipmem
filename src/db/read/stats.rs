use anyhow::{Context, Result};
use rusqlite::named_params;

use crate::db::core::{collect_rows, row_usize, usize_to_i64};
use crate::db::read::mapping::{app_like_pattern, effective_since_param};
use crate::db::read::queries::weekday_name;
use crate::db::types::{
    Database, RetrievalFilters, RetrievalKind, StatsAppEntry, StatsKindBreakdownEntry,
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

impl Database {
    pub(in crate::db) fn load_unfiltered_stats_overview(&self) -> Result<StatsOverview> {
        self.conn
            .query_row(
                r"
                SELECT
                    (SELECT COUNT(*) FROM snapshot_stats) AS snapshot_count,
                    (SELECT COUNT(*) FROM capture_events) AS capture_event_count,
                    (
                        SELECT COUNT(DISTINCT COALESCE(NULLIF(frontmost_app_name, ''), NULLIF(frontmost_app_bundle_id, ''), 'Unknown'))
                        FROM capture_events
                    ) AS unique_app_count,
                    COALESCE((
                        SELECT SUM(s.total_bytes)
                        FROM snapshots s
                        JOIN snapshot_stats ss ON ss.snapshot_id = s.id
                    ), 0) AS total_bytes,
                    (SELECT MIN(first_observed_at) FROM snapshot_stats) AS first_observed_at,
                    (SELECT MAX(last_observed_at) FROM snapshot_stats) AS last_observed_at,
                    (
                        SELECT CAST(strftime('%s', MAX(last_observed_at)) AS INTEGER) - CAST(strftime('%s', MIN(first_observed_at)) AS INTEGER)
                        FROM snapshot_stats
                    ) AS archive_span_seconds
                ",
                [],
                |row| {
                    Ok(StatsOverview {
                        snapshot_count: row_usize(row, 0)?,
                        capture_event_count: row_usize(row, 1)?,
                        unique_app_count: row_usize(row, 2)?,
                        total_bytes: row_usize(row, 3)?,
                        first_observed_at: row.get(4)?,
                        last_observed_at: row.get(5)?,
                        archive_span_seconds: row.get(6)?,
                    })
                },
            )
            .context("load unfiltered stats overview")
    }

    pub(in crate::db) fn load_unfiltered_stats_kind_breakdown(
        &self,
    ) -> Result<Vec<StatsKindBreakdownEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                r"
                SELECT s.snapshot_kind, COUNT(*) AS snapshot_count, COALESCE(SUM(s.total_bytes), 0) AS total_bytes
                FROM snapshots s
                JOIN snapshot_stats ss ON ss.snapshot_id = s.id
                GROUP BY s.snapshot_kind
                ORDER BY snapshot_count DESC, s.snapshot_kind ASC
                ",
            )
            .context("prepare unfiltered stats kind breakdown")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(StatsKindBreakdownEntry {
                    kind: row.get(0)?,
                    snapshot_count: row_usize(row, 1)?,
                    total_bytes: row_usize(row, 2)?,
                })
            })
            .context("execute unfiltered stats kind breakdown")?;
        collect_rows(rows).context("collect unfiltered stats kind breakdown")
    }

    pub(in crate::db) fn load_unfiltered_stats_top_apps(&self) -> Result<Vec<StatsAppEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                r"
                SELECT
                    COALESCE(NULLIF(frontmost_app_name, ''), NULLIF(frontmost_app_bundle_id, ''), 'Unknown') AS app,
                    COUNT(*) AS capture_event_count
                FROM capture_events
                GROUP BY app
                ORDER BY capture_event_count DESC, app ASC
                LIMIT 5
                ",
            )
            .context("prepare unfiltered stats top apps")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(StatsAppEntry {
                    app: row.get(0)?,
                    capture_event_count: row_usize(row, 1)?,
                })
            })
            .context("execute unfiltered stats top apps")?;
        collect_rows(rows).context("collect unfiltered stats top apps")
    }

    pub(in crate::db) fn load_unfiltered_stats_hours(&self) -> Result<Vec<StatsTimeBucketEntry>> {
        self.load_unfiltered_stats_time_distribution(
            "00",
            24,
            "strftime('%H', observed_at)",
            |index| format!("{index:02}"),
        )
    }

    pub(in crate::db) fn load_unfiltered_stats_weekdays(
        &self,
    ) -> Result<Vec<StatsTimeBucketEntry>> {
        self.load_unfiltered_stats_time_distribution(
            "0",
            7,
            "strftime('%w', observed_at)",
            |index| weekday_name(index).to_string(),
        )
    }

    pub(in crate::db) fn load_unfiltered_stats_time_distribution(
        &self,
        first_value: &str,
        count: usize,
        bucket_expr: &str,
        render_bucket: impl Fn(usize) -> String,
    ) -> Result<Vec<StatsTimeBucketEntry>> {
        let sql = format!(
            "WITH RECURSIVE buckets(value) AS (
                 SELECT CAST(:first_value AS INTEGER)
                 UNION ALL
                 SELECT value + 1 FROM buckets WHERE value + 1 < :count
             ),
             counts AS (
                 SELECT CAST({bucket_expr} AS INTEGER) AS value, COUNT(*) AS capture_event_count
                 FROM capture_events
                 GROUP BY value
             )
             SELECT buckets.value, COALESCE(counts.capture_event_count, 0)
             FROM buckets
             LEFT JOIN counts ON counts.value = buckets.value
             ORDER BY buckets.value ASC"
        );
        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("prepare unfiltered stats time distribution")?;
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
            .context("execute unfiltered stats time distribution")?;
        collect_rows(rows).context("collect unfiltered stats time distribution")
    }

    pub(in crate::db) fn load_unfiltered_stats_snapshot_leaderboard(
        &self,
        ordering: &str,
        limit: usize,
    ) -> Result<Vec<StatsSnapshotLeaderboardEntry>> {
        let sql = format!(
            "SELECT
                 s.id,
                 ss.capture_count,
                 s.snapshot_kind,
                 s.preview_text,
                 ss.last_frontmost_app_name,
                 ss.last_frontmost_app_bundle_id,
                 ss.last_observed_at,
                 s.total_bytes
             FROM snapshots s
             JOIN snapshot_stats ss ON ss.snapshot_id = s.id
             ORDER BY {ordering}
             LIMIT :limit"
        );
        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("prepare unfiltered stats snapshot leaderboard")?;
        let rows = stmt
            .query_map(named_params! { ":limit": usize_to_i64(limit)? }, |row| {
                let app_name: Option<String> = row.get(4)?;
                let app_bundle_id: Option<String> = row.get(5)?;
                Ok(StatsSnapshotLeaderboardEntry {
                    snapshot_id: row.get(0)?,
                    capture_count: row_usize(row, 1)?,
                    kind: row.get(2)?,
                    preview_text: row.get(3)?,
                    app_name: app_name.or(app_bundle_id),
                    last_observed_at: row.get(6)?,
                    total_bytes: row_usize(row, 7)?,
                })
            })
            .context("execute unfiltered stats snapshot leaderboard")?;
        collect_rows(rows).context("collect unfiltered stats snapshot leaderboard")
    }

    pub(in crate::db) fn load_stats_overview(&self) -> Result<StatsOverview> {
        self.conn
            .query_row(
                r"
                SELECT
                    (SELECT COUNT(*) FROM clipmem_stats_matching_snapshots) AS snapshot_count,
                    (SELECT COUNT(*) FROM clipmem_stats_matching_events) AS capture_event_count,
                    (
                        SELECT COUNT(DISTINCT COALESCE(NULLIF(frontmost_app_name, ''), NULLIF(frontmost_app_bundle_id, ''), 'Unknown'))
                        FROM clipmem_stats_matching_events
                    ) AS unique_app_count,
                    COALESCE((SELECT SUM(total_bytes) FROM clipmem_stats_matching_snapshots), 0) AS total_bytes,
                    (SELECT MIN(observed_at) FROM clipmem_stats_matching_events) AS first_observed_at,
                    (SELECT MAX(observed_at) FROM clipmem_stats_matching_events) AS last_observed_at,
                    (
                        SELECT CAST(strftime('%s', MAX(observed_at)) AS INTEGER) - CAST(strftime('%s', MIN(observed_at)) AS INTEGER)
                        FROM clipmem_stats_matching_events
                    ) AS archive_span_seconds
                ",
                [],
                |row| {
                    Ok(StatsOverview {
                        snapshot_count: row_usize(row, 0)?,
                        capture_event_count: row_usize(row, 1)?,
                        unique_app_count: row_usize(row, 2)?,
                        total_bytes: row_usize(row, 3)?,
                        first_observed_at: row.get(4)?,
                        last_observed_at: row.get(5)?,
                        archive_span_seconds: row.get(6)?,
                    })
                },
            )
            .context("load stats overview")
    }

    pub(in crate::db) fn load_stats_kind_breakdown(&self) -> Result<Vec<StatsKindBreakdownEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                r"
                SELECT kind, COUNT(*) AS snapshot_count, COALESCE(SUM(total_bytes), 0) AS total_bytes
                FROM clipmem_stats_matching_snapshots
                GROUP BY kind
                ORDER BY snapshot_count DESC, kind ASC
                ",
            )
            .context("prepare stats kind breakdown")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(StatsKindBreakdownEntry {
                    kind: row.get(0)?,
                    snapshot_count: row_usize(row, 1)?,
                    total_bytes: row_usize(row, 2)?,
                })
            })
            .context("execute stats kind breakdown")?;
        collect_rows(rows).context("collect stats kind breakdown")
    }

    pub(in crate::db) fn load_stats_top_apps(&self) -> Result<Vec<StatsAppEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                r"
                SELECT
                    COALESCE(NULLIF(frontmost_app_name, ''), NULLIF(frontmost_app_bundle_id, ''), 'Unknown') AS app,
                    COUNT(*) AS capture_event_count
                FROM clipmem_stats_matching_events
                GROUP BY app
                ORDER BY capture_event_count DESC, app ASC
                LIMIT 5
                ",
            )
            .context("prepare stats top apps")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(StatsAppEntry {
                    app: row.get(0)?,
                    capture_event_count: row_usize(row, 1)?,
                })
            })
            .context("execute stats top apps")?;
        collect_rows(rows).context("collect stats top apps")
    }

    pub(in crate::db) fn load_stats_hours(&self) -> Result<Vec<StatsTimeBucketEntry>> {
        self.load_stats_time_distribution("00", 24, "strftime('%H', observed_at)", |index| {
            format!("{index:02}")
        })
    }

    pub(in crate::db) fn load_stats_weekdays(&self) -> Result<Vec<StatsTimeBucketEntry>> {
        self.load_stats_time_distribution("0", 7, "strftime('%w', observed_at)", |index| {
            weekday_name(index).to_string()
        })
    }

    pub(in crate::db) fn load_stats_time_distribution(
        &self,
        first_value: &str,
        count: usize,
        bucket_expr: &str,
        render_bucket: impl Fn(usize) -> String,
    ) -> Result<Vec<StatsTimeBucketEntry>> {
        let sql = format!(
            "WITH RECURSIVE buckets(value) AS (
                 SELECT CAST(:first_value AS INTEGER)
                 UNION ALL
                 SELECT value + 1 FROM buckets WHERE value + 1 < :count
             ),
             counts AS (
                 SELECT CAST({bucket_expr} AS INTEGER) AS value, COUNT(*) AS capture_event_count
                 FROM clipmem_stats_matching_events
                 GROUP BY value
             )
             SELECT buckets.value, COALESCE(counts.capture_event_count, 0)
             FROM buckets
             LEFT JOIN counts ON counts.value = buckets.value
             ORDER BY buckets.value ASC"
        );
        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("prepare stats time distribution")?;
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
            .context("execute stats time distribution")?;
        collect_rows(rows).context("collect stats time distribution")
    }

    pub(in crate::db) fn load_stats_snapshot_leaderboard(
        &self,
        ordering: &str,
        limit: usize,
    ) -> Result<Vec<StatsSnapshotLeaderboardEntry>> {
        let sql = format!(
            "SELECT
                 snapshot_id,
                 capture_count,
                 kind,
                 preview_text,
                 last_frontmost_app_name,
                 last_frontmost_app_bundle_id,
                 last_observed_at,
                 total_bytes
             FROM clipmem_stats_matching_snapshots
             ORDER BY {ordering}
             LIMIT :limit"
        );
        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("prepare stats snapshot leaderboard")?;
        let rows = stmt
            .query_map(named_params! { ":limit": usize_to_i64(limit)? }, |row| {
                let app_name: Option<String> = row.get(4)?;
                let app_bundle_id: Option<String> = row.get(5)?;
                Ok(StatsSnapshotLeaderboardEntry {
                    snapshot_id: row.get(0)?,
                    capture_count: row_usize(row, 1)?,
                    kind: row.get(2)?,
                    preview_text: row.get(3)?,
                    app_name: app_name.or(app_bundle_id),
                    last_observed_at: row.get(6)?,
                    total_bytes: row_usize(row, 7)?,
                })
            })
            .context("execute stats snapshot leaderboard")?;
        collect_rows(rows).context("collect stats snapshot leaderboard")
    }
}
