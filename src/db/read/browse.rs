use anyhow::{Context, Result};
use rusqlite::named_params;

use crate::db::core::{collect_rows, sanitise_limit, usize_to_i64};
use crate::db::read::mapping::{
    app_like_pattern, can_use_snapshot_event_cache, can_use_snapshot_stats_for_stats,
    can_use_snapshot_stats_since_filter, effective_since_param, event_filter_clause,
    map_search_hit_row, map_timeline_event_row, paginate_rows, paginate_timeline_rows,
    requires_matching_events, snapshot_filter_clause,
};
use crate::db::read::queries::{recent_query, timeline_query};
use crate::db::read::stats::stats_params;
use crate::db::types::{
    Database, Page, RecentCursorState, RetrievalFilters, StatsReport, TimelineCursorState,
    TimelineSort,
};
use crate::model::{SearchHit, TimelineEvent};

impl Database {
    /// Return the most recent archived snapshots that match the supplied filters.
    ///
    /// # Errors
    ///
    /// Returns an error if filter values cannot be converted for SQLite, if relative hour filters
    /// cannot be formatted as timestamps, or if the recent query cannot be prepared, executed, or
    /// collected.
    pub fn recent(&self, limit: usize, filters: &RetrievalFilters) -> Result<Vec<SearchHit>> {
        self.recent_page(limit, filters, None).map(Page::into_items)
    }

    pub(crate) fn recent_page(
        &self,
        limit: usize,
        filters: &RetrievalFilters,
        cursor: Option<&RecentCursorState>,
    ) -> Result<Page<SearchHit>> {
        let limit = sanitise_limit(limit);
        let fetch_limit = usize_to_i64(limit.saturating_add(1))?;
        let use_snapshot_stats_since_filter = can_use_snapshot_stats_since_filter(filters);
        let include_matching_events =
            requires_matching_events(filters) && !use_snapshot_stats_since_filter;
        let use_snapshot_event_cache = can_use_snapshot_event_cache(filters);
        let sql = recent_query(
            include_matching_events,
            use_snapshot_event_cache,
            use_snapshot_stats_since_filter,
        );
        let app_like = app_like_pattern(filters);
        let bundle_id = filters.bundle_id().map(|value| value.to_ascii_lowercase());
        let kind = filters.kind().map(|value| value.as_str());
        let since = effective_since_param(filters)?;

        let mut stmt = self.conn.prepare(&sql).context("prepare recent query")?;
        let rows = stmt
            .query_map(
                named_params! {
                    ":since" : since.as_deref(),
                    ":until" : filters.until(),
                    ":app_like" : app_like.as_deref(),
                    ":bundle_id" : bundle_id.as_deref(),
                    ":kind" : kind,
                    ":has_text" : filters.has_text(),
                    ":has_url" : filters.has_url(),
                    ":has_file_url" : filters.has_file_url(),
                    ":has_image" : filters.has_image(),
                    ":has_pdf" : filters.has_pdf(),
                    ":min_bytes" : filters.min_bytes().map(usize_to_i64).transpose()?,
                    ":max_bytes" : filters.max_bytes().map(usize_to_i64).transpose()?,
                    ":cursor_last_seen_at" : cursor.map(RecentCursorState::last_seen_at),
                    ":cursor_snapshot_id" : cursor.map(RecentCursorState::snapshot_id),
                    ":limit" : fetch_limit,
                },
                |row| map_search_hit_row(row, false),
            )
            .context("execute recent query")?;

        collect_rows(rows)
            .map(|hits| paginate_rows(hits, limit))
            .context("collect recent query rows")
    }

    /// Return capture events in stable chronological order.
    ///
    /// # Errors
    ///
    /// Returns an error if the query cannot be prepared or executed.
    pub(crate) fn timeline_page(
        &self,
        limit: usize,
        filters: &RetrievalFilters,
        sort: TimelineSort,
        cursor: Option<&TimelineCursorState>,
    ) -> Result<Page<TimelineEvent>> {
        let limit = sanitise_limit(limit);
        let fetch_limit = usize_to_i64(limit.saturating_add(1))?;
        let sql = timeline_query(sort);
        let app_like = app_like_pattern(filters);
        let bundle_id = filters.bundle_id().map(|value| value.to_ascii_lowercase());
        let kind = filters.kind().map(|value| value.as_str());
        let since = effective_since_param(filters)?;

        let mut stmt = self.conn.prepare(&sql).context("prepare timeline query")?;
        let rows = stmt
            .query_map(
                named_params! {
                    ":since" : since.as_deref(),
                    ":until" : filters.until(),
                    ":app_like" : app_like.as_deref(),
                    ":bundle_id" : bundle_id.as_deref(),
                    ":kind" : kind,
                    ":has_text" : filters.has_text(),
                    ":has_url" : filters.has_url(),
                    ":has_file_url" : filters.has_file_url(),
                    ":has_image" : filters.has_image(),
                    ":has_pdf" : filters.has_pdf(),
                    ":min_bytes" : filters.min_bytes().map(usize_to_i64).transpose()?,
                    ":max_bytes" : filters.max_bytes().map(usize_to_i64).transpose()?,
                    ":cursor_observed_at" : cursor.map(TimelineCursorState::observed_at),
                    ":cursor_event_id" : cursor.map(TimelineCursorState::event_id),
                    ":limit" : fetch_limit,
                },
                map_timeline_event_row,
            )
            .context("execute timeline query")?;

        collect_rows(rows)
            .map(|events| paginate_timeline_rows(events, limit))
            .context("collect timeline query rows")
    }

    pub(crate) fn snapshot_matches_filters(
        &self,
        snapshot_id: i64,
        filters: &RetrievalFilters,
    ) -> Result<bool> {
        let sql = format!(
            "SELECT EXISTS(
                 SELECT 1
                 FROM snapshots s
                 WHERE s.id = :snapshot_id
                   AND EXISTS (
                       SELECT 1
                       FROM capture_events ce
                       WHERE ce.snapshot_id = s.id
                         AND {event_filter_clause}
                   )
                   AND {snapshot_filter_clause}
             )",
            event_filter_clause = event_filter_clause("ce"),
            snapshot_filter_clause = snapshot_filter_clause("s", "s.id"),
        );
        let app_like = app_like_pattern(filters);
        let bundle_id = filters.bundle_id().map(|value| value.to_ascii_lowercase());
        let kind = filters.kind().map(|value| value.as_str());
        let since = effective_since_param(filters)?;
        let exists: i64 = self
            .conn
            .query_row(
                &sql,
                named_params! {
                    ":snapshot_id" : snapshot_id,
                    ":since" : since.as_deref(),
                    ":until" : filters.until(),
                    ":app_like" : app_like.as_deref(),
                    ":bundle_id" : bundle_id.as_deref(),
                    ":kind" : kind,
                    ":has_text" : filters.has_text(),
                    ":has_url" : filters.has_url(),
                    ":has_file_url" : filters.has_file_url(),
                    ":has_image" : filters.has_image(),
                    ":has_pdf" : filters.has_pdf(),
                    ":min_bytes" : filters.min_bytes().map(usize_to_i64).transpose()?,
                    ":max_bytes" : filters.max_bytes().map(usize_to_i64).transpose()?,
                },
                |row| row.get(0),
            )
            .context("evaluate snapshot filters")?;

        Ok(exists != 0)
    }

    /// Return aggregate archive statistics for snapshots matching the supplied filters.
    ///
    /// # Errors
    ///
    /// Returns an error if filter values cannot be converted for SQLite, if relative hour filters
    /// cannot be formatted as timestamps, or if any stats query or temporary-table operation cannot
    /// be prepared or executed.
    pub fn stats(&self, filters: &RetrievalFilters) -> Result<StatsReport> {
        if *filters == RetrievalFilters::default() {
            return self.stats_unfiltered();
        }

        let params = stats_params(filters)?;
        self.conn
            .execute_batch(
                "DROP TABLE IF EXISTS temp.clipmem_stats_matching_events;
                 DROP TABLE IF EXISTS temp.clipmem_stats_matching_snapshots;",
            )
            .context("clear stats temp tables")?;

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

        self.conn
            .execute_batch(
                "CREATE INDEX temp.idx_clipmem_stats_events_snapshot_id
                    ON clipmem_stats_matching_events(snapshot_id);
                 CREATE INDEX temp.idx_clipmem_stats_events_observed_at
                    ON clipmem_stats_matching_events(observed_at);",
            )
            .context("index stats matching events")?;

        if can_use_snapshot_stats_for_stats(filters) {
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
        } else {
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
        }

        self.conn
            .execute_batch(
                "CREATE INDEX temp.idx_clipmem_stats_snapshots_capture_count
                    ON clipmem_stats_matching_snapshots(capture_count DESC, snapshot_id ASC);
                 CREATE INDEX temp.idx_clipmem_stats_snapshots_bytes
                    ON clipmem_stats_matching_snapshots(total_bytes DESC, snapshot_id ASC);",
            )
            .context("index stats matching snapshots")?;

        let overview = self.load_stats_overview()?;
        let kind_breakdown = self.load_stats_kind_breakdown()?;
        let top_apps = self.load_stats_top_apps()?;
        let busiest_hours = self.load_stats_hours()?;
        let busiest_weekdays = self.load_stats_weekdays()?;
        let largest_snapshots =
            self.load_stats_snapshot_leaderboard("total_bytes DESC, snapshot_id ASC", 5)?;
        let most_captured_snapshots =
            self.load_stats_snapshot_leaderboard("capture_count DESC, snapshot_id ASC", 5)?;
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

    fn stats_unfiltered(&self) -> Result<StatsReport> {
        let overview = self.load_unfiltered_stats_overview()?;
        let kind_breakdown = self.load_unfiltered_stats_kind_breakdown()?;
        let top_apps = self.load_unfiltered_stats_top_apps()?;
        let busiest_hours = self.load_unfiltered_stats_hours()?;
        let busiest_weekdays = self.load_unfiltered_stats_weekdays()?;
        let largest_snapshots =
            self.load_unfiltered_stats_snapshot_leaderboard("s.total_bytes DESC, s.id ASC", 5)?;
        let most_captured_snapshots =
            self.load_unfiltered_stats_snapshot_leaderboard("ss.capture_count DESC, s.id ASC", 5)?;
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
}
