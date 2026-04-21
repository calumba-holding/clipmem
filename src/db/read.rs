use anyhow::{Context, Result};
use rusqlite::{named_params, params, Error as SqlError, ErrorCode, OptionalExtension, Row};

use crate::model::{
    CaptureEvent, ClipboardItem, ClipboardRepresentation, DoctorReport, FlattenedTextProjection,
    SearchHit, SnapshotDetails, TimelineEvent,
};

use super::{
    collect_rows, row_enum, row_usize, sanitise_limit, usize_to_i64, Database, Page,
    RecentCursorState, RetrievalFilters, SearchCursorState, SearchMode, SearchResults,
    StatsAppEntry, StatsKindBreakdownEntry, StatsReport, StatsSnapshotLeaderboardEntry,
    StatsTimeBucketEntry, TimelineCursorState, TimelineSort,
};

const LIST_VALUE_SEPARATOR: char = '\u{1f}';
const MATCHED_FIELDS_SEPARATOR: char = '\u{1e}';
const FAST_FILE_PATH_LITERAL_QUERY: &str = r"
    SELECT
        s.id AS snapshot_id,
        ss.last_event_id AS event_id,
        s.sha256 AS sha256,
        s.snapshot_kind AS snapshot_kind,
        s.preview_text AS preview_text,
        s.search_text AS search_text,
        'Path fragment match in file paths' AS why_matched,
        'file_paths' AS matched_fields,
        ss.capture_count AS capture_count,
        ss.first_observed_at AS first_observed_at,
        ss.last_observed_at AS last_observed_at,
        ss.last_frontmost_app_name AS last_frontmost_app_name,
        ss.last_frontmost_app_bundle_id AS last_frontmost_app_bundle_id,
        COALESCE(sp.urls, '') AS urls,
        COALESCE(sp.file_urls, '') AS file_urls,
        s.total_bytes AS total_bytes,
        s.item_count AS item_count,
        (
            1.12 +
            CASE
                WHEN datetime(ss.last_observed_at) >= datetime('now', '-24 hours') THEN 0.05
                WHEN datetime(ss.last_observed_at) >= datetime('now', '-7 days') THEN 0.02
                ELSE 0
            END
        ) AS score
    FROM snapshot_file_url_fts
    JOIN snapshots s ON s.id = snapshot_file_url_fts.rowid
    JOIN snapshot_stats ss ON ss.snapshot_id = snapshot_file_url_fts.rowid
    JOIN snapshot_projection_cache sp ON sp.snapshot_id = snapshot_file_url_fts.rowid
    WHERE snapshot_file_url_fts MATCH :literal_match
    ORDER BY score DESC, ss.last_observed_at DESC, s.id DESC
    LIMIT :limit
";
const FAST_TEXT_LITERAL_QUERY: &str = r"
    SELECT
        s.id AS snapshot_id,
        ss.last_event_id AS event_id,
        s.sha256 AS sha256,
        s.snapshot_kind AS snapshot_kind,
        s.preview_text AS preview_text,
        s.search_text AS search_text,
        CASE
            WHEN :exact_phrase_lower IS NOT NULL AND lower(COALESCE(NULLIF(s.preview_text, ''), s.search_text, '')) LIKE ('%' || :exact_phrase_lower || '%') ESCAPE '\' THEN 'Exact phrase match in best text'
            WHEN lower(COALESCE(NULLIF(s.preview_text, ''), s.search_text, '')) = :query_lower THEN 'Exact text match in best text'
            WHEN lower(COALESCE(NULLIF(s.preview_text, ''), s.search_text, '')) LIKE :prefix_like ESCAPE '\' THEN 'Prefix match in best text'
            ELSE 'Literal text match in best text'
        END AS why_matched,
        CASE
            WHEN lower(COALESCE(NULLIF(s.preview_text, ''), '')) LIKE :like ESCAPE '\' THEN 'best_text'
            WHEN lower(COALESCE(s.preview_text, '')) LIKE :like ESCAPE '\' THEN 'preview_text'
            ELSE 'search_text'
        END AS matched_fields,
        ss.capture_count AS capture_count,
        ss.first_observed_at AS first_observed_at,
        ss.last_observed_at AS last_observed_at,
        ss.last_frontmost_app_name AS last_frontmost_app_name,
        ss.last_frontmost_app_bundle_id AS last_frontmost_app_bundle_id,
        COALESCE(sp.urls, '') AS urls,
        COALESCE(sp.file_urls, '') AS file_urls,
        s.total_bytes AS total_bytes,
        s.item_count AS item_count,
        (
            CASE WHEN :exact_phrase_lower IS NOT NULL AND lower(COALESCE(NULLIF(s.preview_text, ''), s.search_text, '')) LIKE ('%' || :exact_phrase_lower || '%') ESCAPE '\' THEN 0.98 ELSE 0 END +
            CASE WHEN lower(COALESCE(NULLIF(s.preview_text, ''), s.search_text, '')) = :query_lower THEN 0.96 ELSE 0 END +
            CASE WHEN lower(COALESCE(NULLIF(s.preview_text, ''), s.search_text, '')) LIKE :prefix_like ESCAPE '\' THEN 0.88 ELSE 0 END +
            CASE WHEN lower(COALESCE(NULLIF(s.preview_text, ''), s.search_text, '')) LIKE :like ESCAPE '\' THEN 0.78 ELSE 0 END +
            CASE WHEN lower(COALESCE(s.search_text, '')) LIKE :like ESCAPE '\' THEN 0.72 ELSE 0 END +
            CASE
                WHEN datetime(ss.last_observed_at) >= datetime('now', '-24 hours') THEN 0.05
                WHEN datetime(ss.last_observed_at) >= datetime('now', '-7 days') THEN 0.02
                ELSE 0
            END
        ) AS score
    FROM snapshots_fts
    JOIN snapshots s ON s.id = snapshots_fts.rowid
    JOIN snapshot_stats ss ON ss.snapshot_id = snapshots_fts.rowid
    LEFT JOIN snapshot_projection_cache sp ON sp.snapshot_id = snapshots_fts.rowid
    WHERE snapshots_fts MATCH :token_match
      AND (
          lower(COALESCE(s.search_text, '')) LIKE :like ESCAPE '\'
          OR lower(COALESCE(s.preview_text, '')) LIKE :like ESCAPE '\'
      )
    ORDER BY score DESC, ss.last_observed_at DESC, s.id DESC
    LIMIT :limit
";

#[derive(Debug, Clone)]
struct QueryAnalysis {
    trimmed: String,
    lower: String,
    exact_phrase: Option<String>,
    literal_preferred: bool,
    path_fragment: Option<String>,
}
impl Database {
    pub(crate) fn latest_capture_observed_at(&self) -> Result<Option<String>> {
        let observed_at = self
            .conn
            .query_row("SELECT MAX(observed_at) FROM capture_events", [], |row| {
                row.get::<_, Option<String>>(0)
            })
            .context("read latest capture timestamp")?;
        Ok(observed_at)
    }

    pub(crate) fn has_capture_within_hours(&self, hours: u32) -> Result<bool> {
        let sql = "SELECT EXISTS(
                SELECT 1
                FROM capture_events
                WHERE datetime(observed_at) >= datetime('now', printf('-%d hours', ?1))
            )";
        self.conn
            .query_row(sql, params![i64::from(hours)], |row| row.get::<_, i64>(0))
            .map(|value| value != 0)
            .context("read capture freshness")
    }

    /// Search stored snapshots in automatic mode.
    ///
    /// Automatic mode prefers FTS5, but falls back to literal `LIKE` matching for wildcard-like
    /// input, invalid FTS syntax, and empty FTS result sets.
    ///
    /// # Errors
    ///
    /// Returns an error if the query cannot be prepared or executed, or when `SQLite` returns a
    /// non-syntax FTS error.
    pub fn search_auto(
        &self,
        query: &str,
        limit: usize,
        filters: &RetrievalFilters,
    ) -> Result<SearchResults> {
        self.search_auto_page(query, limit, filters, None)
    }

    pub(crate) fn search_auto_page(
        &self,
        query: &str,
        limit: usize,
        filters: &RetrievalFilters,
        cursor: Option<&SearchCursorState>,
    ) -> Result<SearchResults> {
        let limit = sanitise_limit(limit);
        let analysis = analyze_query(query);

        if analysis.literal_preferred {
            return self.search_literal_page(query, limit, filters, cursor);
        }

        if let Some(cursor) = cursor {
            return match cursor.mode_used() {
                SearchMode::Fts => self.search_fts_page(query, limit, filters, Some(cursor)),
                SearchMode::Literal => {
                    self.search_literal_page(query, limit, filters, Some(cursor))
                }
                SearchMode::Auto => self.search_literal_page(query, limit, filters, Some(cursor)),
            };
        }

        let results = match self.search_fts_page(query, limit, filters, None) {
            Ok(results) => results,
            Err(error) => {
                if let Some(sqlite_error) = error.downcast_ref::<SqlError>() {
                    if is_invalid_fts_query(sqlite_error) {
                        self.search_literal_page(query, limit, filters, None)?
                    } else {
                        return Err(error);
                    }
                } else {
                    return Err(error);
                }
            }
        };

        if results.hits().is_empty() {
            self.search_literal_page(query, limit, filters, None)
        } else {
            Ok(results)
        }
    }

    /// Return recently observed snapshots, optionally restricted to the last `hours`.
    ///
    /// # Errors
    ///
    /// Returns an error if the query cannot be prepared or executed.
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

    /// Load one snapshot with its recent events and stored item representations.
    ///
    /// # Errors
    ///
    /// Returns an error if any lookup query fails.
    pub fn find_snapshot(
        &self,
        snapshot_id: i64,
        event_limit: usize,
    ) -> Result<Option<SnapshotDetails>> {
        let Some(mut details) = self.load_snapshot_summary(snapshot_id)? else {
            return Ok(None);
        };

        details
            .set_recent_events(self.load_recent_events(snapshot_id, sanitise_limit(event_limit))?);
        details.set_items(self.load_snapshot_items(snapshot_id)?);

        Ok(Some(details))
    }

    pub(crate) fn snapshot_projection(
        &self,
        snapshot_id: i64,
    ) -> Result<Option<FlattenedTextProjection>> {
        if !self.snapshot_exists(snapshot_id)? {
            return Ok(None);
        }

        let items = self.load_snapshot_items(snapshot_id)?;
        let ocr = self
            .conn
            .query_row(
                "SELECT ocr_text, status FROM snapshot_ocr_cache WHERE snapshot_id = ?1",
                [snapshot_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .optional()
            .context("load snapshot ocr projection")?
            .unwrap_or((None, None));
        Ok(Some(
            FlattenedTextProjection::from_items(&items).with_ocr(ocr.0, ocr.1),
        ))
    }

    fn snapshot_exists(&self, snapshot_id: i64) -> Result<bool> {
        let exists = self
            .conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM snapshots WHERE id = ?1)",
                [snapshot_id],
                |row| row.get::<_, i64>(0),
            )
            .context("check snapshot exists for projection")?;
        Ok(exists != 0)
    }

    /// Collect `SQLite` and FTS5 diagnostics for the current archive database.
    ///
    /// # Errors
    ///
    /// Returns an error if any diagnostic query fails.
    pub fn doctor(&self) -> Result<DoctorReport> {
        let sqlite_version: String = self
            .conn
            .query_row("SELECT sqlite_version()", [], |row| row.get(0))
            .context("read SQLite version")?;

        let journal_mode: String = self
            .conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .context("read journal mode")?;

        let mut compile_stmt = self
            .conn
            .prepare("PRAGMA compile_options")
            .context("prepare compile options query")?;
        let compile_rows = compile_stmt
            .query_map([], |row| row.get::<_, String>(0))
            .context("execute compile options query")?;
        let compile_options = collect_rows(compile_rows).context("collect compile options")?;

        let fts5_compile_option_present = compile_options
            .iter()
            .any(|opt| opt == "ENABLE_FTS5" || opt == "SQLITE_ENABLE_FTS5");

        self.conn
            .execute_batch(
                "CREATE VIRTUAL TABLE temp.__clipmem_fts_check USING fts5(x);
                 DROP TABLE temp.__clipmem_fts_check;",
            )
            .context("probe FTS5 temp virtual table creation")?;
        let fts5_create_virtual_table_ok = true;

        Ok(DoctorReport::new(
            self.path.display().to_string(),
            sqlite_version,
            journal_mode,
            fts5_compile_option_present,
            fts5_create_virtual_table_ok,
            compile_options,
        ))
    }

    /// Search stored snapshots with explicit FTS5 semantics.
    ///
    /// # Errors
    ///
    /// Returns an error if the FTS query cannot be prepared or executed.
    pub fn search_fts(
        &self,
        query: &str,
        limit: usize,
        filters: &RetrievalFilters,
    ) -> Result<SearchResults> {
        self.search_fts_page(query, limit, filters, None)
    }

    pub(crate) fn search_fts_page(
        &self,
        query: &str,
        limit: usize,
        filters: &RetrievalFilters,
        cursor: Option<&SearchCursorState>,
    ) -> Result<SearchResults> {
        let limit = sanitise_limit(limit);
        let fetch_limit = usize_to_i64(limit.saturating_add(1))?;
        let analysis = analyze_query(query);
        let use_snapshot_event_cache = can_use_snapshot_event_cache(filters);
        let has_temporal_event_filters = has_temporal_event_filters(filters);
        let sql = fts_query(
            use_snapshot_event_cache,
            has_temporal_event_filters,
            is_simple_fts_query(&analysis),
        );
        let app_like = app_like_pattern(filters);
        let bundle_id = filters.bundle_id().map(|value| value.to_ascii_lowercase());
        let kind = filters.kind().map(|value| value.as_str());
        let since = effective_since_param(filters)?;

        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("prepare FTS search query")?;
        let rows = stmt
            .query_map(
                named_params! {
                    ":query" : query,
                    ":query_lower" : analysis.lower.as_str(),
                    ":exact_phrase_lower" : analysis.exact_phrase.as_deref(),
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
                    ":cursor_score" : cursor.and_then(SearchCursorState::score),
                    ":cursor_last_seen_at" : cursor.map(SearchCursorState::last_seen_at),
                    ":cursor_snapshot_id" : cursor.map(SearchCursorState::snapshot_id),
                    ":limit" : fetch_limit,
                },
                |row| map_search_hit_row(row, true),
            )
            .context("execute FTS search query")?;

        let native_hits = collect_rows(rows)
            .map(|hits| paginate_rows(hits, limit))
            .map(|page| {
                let has_more = page.has_more();
                SearchResults::new(SearchMode::Fts, page.into_items(), has_more)
            })
            .context("collect FTS search rows")?;
        let ocr_hits = self.search_ocr_fts_hits(query, limit, filters, cursor)?;
        Ok(merge_scored_search_results(
            SearchMode::Fts,
            native_hits,
            ocr_hits,
            limit,
            true,
        ))
    }

    /// Search stored snapshots with literal `LIKE` matching semantics.
    ///
    /// # Errors
    ///
    /// Returns an error if the literal query cannot be prepared or executed.
    pub fn search_literal(
        &self,
        query: &str,
        limit: usize,
        filters: &RetrievalFilters,
    ) -> Result<SearchResults> {
        self.search_literal_page(query, limit, filters, None)
    }

    pub(crate) fn search_literal_page(
        &self,
        query: &str,
        limit: usize,
        filters: &RetrievalFilters,
        cursor: Option<&SearchCursorState>,
    ) -> Result<SearchResults> {
        let limit = sanitise_limit(limit);
        let fetch_limit = usize_to_i64(limit.saturating_add(1))?;
        let analysis = analyze_query(query);
        if let Some(path_fragment) = analysis.path_fragment.as_deref() {
            let results =
                self.search_file_path_literal_page(path_fragment, limit, filters, cursor)?;
            if cursor.is_some() || !results.hits().is_empty() {
                return Ok(results);
            }
        }
        if cursor.is_none() && *filters == RetrievalFilters::default() {
            if let Some(token_match) = literal_token_match_query(&analysis) {
                let results =
                    self.search_unfiltered_text_literal_page(&analysis, &token_match, limit)?;
                if !results.hits().is_empty() {
                    return Ok(results);
                }
            }
        }

        let like = format!("%{}%", escape_like_pattern(&analysis.trimmed));
        let include_matching_events = requires_matching_events(filters);
        let use_snapshot_event_cache = can_use_snapshot_event_cache(filters);
        let literal_match = literal_fts_match_query(&analysis);
        let sql = literal_query(
            include_matching_events,
            use_snapshot_event_cache,
            literal_match.is_some(),
        );
        let app_like = app_like_pattern(filters);
        let bundle_id = filters.bundle_id().map(|value| value.to_ascii_lowercase());
        let kind = filters.kind().map(|value| value.as_str());
        let since = effective_since_param(filters)?;

        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("prepare literal search query")?;
        let prefix_like = format!("{}%", escape_like_pattern(&analysis.lower));
        let path_fragment_like = analysis
            .path_fragment
            .as_ref()
            .map(|value| format!("%{}%", escape_like_pattern(value)));
        let min_bytes = filters.min_bytes().map(usize_to_i64).transpose()?;
        let max_bytes = filters.max_bytes().map(usize_to_i64).transpose()?;
        let rows = if let Some(literal_match) = literal_match.as_deref() {
            stmt.query_map(
                named_params! {
                    ":query_lower" : analysis.lower.as_str(),
                    ":like" : like,
                    ":prefix_like" : prefix_like.as_str(),
                    ":literal_match" : literal_match,
                    ":path_fragment_like" : path_fragment_like.as_deref(),
                    ":exact_phrase_lower" : analysis.exact_phrase.as_deref(),
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
                    ":min_bytes" : min_bytes,
                    ":max_bytes" : max_bytes,
                    ":cursor_score" : cursor.and_then(SearchCursorState::score),
                    ":cursor_last_seen_at" : cursor.map(SearchCursorState::last_seen_at),
                    ":cursor_snapshot_id" : cursor.map(SearchCursorState::snapshot_id),
                    ":limit" : fetch_limit,
                },
                map_scored_search_hit_row,
            )
        } else {
            stmt.query_map(
                named_params! {
                    ":query_lower" : analysis.lower.as_str(),
                    ":like" : like,
                    ":prefix_like" : prefix_like.as_str(),
                    ":path_fragment_like" : path_fragment_like.as_deref(),
                    ":exact_phrase_lower" : analysis.exact_phrase.as_deref(),
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
                    ":min_bytes" : min_bytes,
                    ":max_bytes" : max_bytes,
                    ":cursor_score" : cursor.and_then(SearchCursorState::score),
                    ":cursor_last_seen_at" : cursor.map(SearchCursorState::last_seen_at),
                    ":cursor_snapshot_id" : cursor.map(SearchCursorState::snapshot_id),
                    ":limit" : fetch_limit,
                },
                map_scored_search_hit_row,
            )
        }
        .context("execute literal search query")?;

        let native_hits = collect_rows(rows)
            .map(|hits| paginate_rows(hits, limit))
            .map(|page| {
                let has_more = page.has_more();
                SearchResults::new(SearchMode::Literal, page.into_items(), has_more)
            })
            .context("collect literal search rows")?;
        let ocr_hits = self.search_ocr_literal_hits(&analysis, limit, filters, cursor)?;
        Ok(merge_scored_search_results(
            SearchMode::Literal,
            native_hits,
            ocr_hits,
            limit,
            false,
        ))
    }

    fn search_ocr_fts_hits(
        &self,
        query: &str,
        limit: usize,
        filters: &RetrievalFilters,
        cursor: Option<&SearchCursorState>,
    ) -> Result<SearchResults> {
        let fetch_limit = usize_to_i64(limit.saturating_add(1))?;
        let use_snapshot_event_cache = can_use_snapshot_event_cache(filters);
        let has_temporal_event_filters = has_temporal_event_filters(filters);
        let sql = ocr_fts_query(use_snapshot_event_cache, has_temporal_event_filters);
        let app_like = app_like_pattern(filters);
        let bundle_id = filters.bundle_id().map(|value| value.to_ascii_lowercase());
        let kind = filters.kind().map(|value| value.as_str());
        let since = effective_since_param(filters)?;

        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("prepare OCR FTS search query")?;
        let rows = stmt
            .query_map(
                named_params! {
                    ":query" : query,
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
                    ":cursor_score" : cursor.and_then(SearchCursorState::score),
                    ":cursor_last_seen_at" : cursor.map(SearchCursorState::last_seen_at),
                    ":cursor_snapshot_id" : cursor.map(SearchCursorState::snapshot_id),
                    ":limit" : fetch_limit,
                },
                |row| map_search_hit_row(row, true),
            )
            .context("execute OCR FTS search query")?;

        collect_rows(rows)
            .map(|hits| paginate_rows(hits, limit))
            .map(|page| {
                let has_more = page.has_more();
                SearchResults::new(SearchMode::Fts, page.into_items(), has_more)
            })
            .context("collect OCR FTS search rows")
    }

    fn search_ocr_literal_hits(
        &self,
        analysis: &QueryAnalysis,
        limit: usize,
        filters: &RetrievalFilters,
        cursor: Option<&SearchCursorState>,
    ) -> Result<SearchResults> {
        let fetch_limit = usize_to_i64(limit.saturating_add(1))?;
        let like = format!("%{}%", escape_like_pattern(&analysis.trimmed));
        let prefix_like = format!("{}%", escape_like_pattern(&analysis.lower));
        let app_like = app_like_pattern(filters);
        let bundle_id = filters.bundle_id().map(|value| value.to_ascii_lowercase());
        let kind = filters.kind().map(|value| value.as_str());
        let since = effective_since_param(filters)?;
        let include_matching_events = requires_matching_events(filters);
        let use_snapshot_event_cache = can_use_snapshot_event_cache(filters);
        let literal_match = literal_fts_match_query(analysis);
        let sql = ocr_literal_query(
            include_matching_events,
            use_snapshot_event_cache,
            literal_match.is_some(),
        );

        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("prepare OCR literal search query")?;
        let rows = stmt
            .query_map(
                named_params! {
                    ":query_lower" : analysis.lower.as_str(),
                    ":like" : like,
                    ":prefix_like" : prefix_like.as_str(),
                    ":literal_match" : literal_match.as_deref(),
                    ":exact_phrase_lower" : analysis.exact_phrase.as_deref(),
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
                    ":cursor_score" : cursor.and_then(SearchCursorState::score),
                    ":cursor_last_seen_at" : cursor.map(SearchCursorState::last_seen_at),
                    ":cursor_snapshot_id" : cursor.map(SearchCursorState::snapshot_id),
                    ":limit" : fetch_limit,
                },
                map_scored_search_hit_row,
            )
            .context("execute OCR literal search query")?;

        collect_rows(rows)
            .map(|hits| paginate_rows(hits, limit))
            .map(|page| {
                let has_more = page.has_more();
                SearchResults::new(SearchMode::Literal, page.into_items(), has_more)
            })
            .context("collect OCR literal search rows")
    }

    fn search_file_path_literal_page(
        &self,
        path_fragment: &str,
        limit: usize,
        filters: &RetrievalFilters,
        cursor: Option<&SearchCursorState>,
    ) -> Result<SearchResults> {
        let fetch_limit = usize_to_i64(limit.saturating_add(1))?;
        let include_matching_events = requires_matching_events(filters);
        let use_snapshot_event_cache = can_use_snapshot_event_cache(filters);
        let literal_match = if path_fragment.chars().count() >= 3 {
            Some(format!("\"{}\"", path_fragment.replace('"', "\"\"")))
        } else {
            None
        };
        if cursor.is_none() && *filters == RetrievalFilters::default() {
            if let Some(literal_match) = literal_match.as_deref() {
                return self.search_unfiltered_file_path_literal_page(literal_match, limit);
            }
        }
        let sql = file_path_literal_query(
            include_matching_events,
            use_snapshot_event_cache,
            literal_match.is_some(),
        );
        let app_like = app_like_pattern(filters);
        let bundle_id = filters.bundle_id().map(|value| value.to_ascii_lowercase());
        let kind = filters.kind().map(|value| value.as_str());
        let since = effective_since_param(filters)?;
        let min_bytes = filters.min_bytes().map(usize_to_i64).transpose()?;
        let max_bytes = filters.max_bytes().map(usize_to_i64).transpose()?;
        let path_like = format!("%{}%", escape_like_pattern(path_fragment));

        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("prepare file-path literal search query")?;
        let rows = if let Some(literal_match) = literal_match.as_deref() {
            stmt.query_map(
                named_params! {
                    ":literal_match" : literal_match,
                    ":path_fragment" : path_fragment,
                    ":path_like" : path_like.as_str(),
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
                    ":min_bytes" : min_bytes,
                    ":max_bytes" : max_bytes,
                    ":cursor_score" : cursor.and_then(SearchCursorState::score),
                    ":cursor_last_seen_at" : cursor.map(SearchCursorState::last_seen_at),
                    ":cursor_snapshot_id" : cursor.map(SearchCursorState::snapshot_id),
                    ":limit" : fetch_limit,
                },
                map_scored_search_hit_row,
            )
        } else {
            stmt.query_map(
                named_params! {
                    ":path_fragment" : path_fragment,
                    ":path_like" : path_like.as_str(),
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
                    ":min_bytes" : min_bytes,
                    ":max_bytes" : max_bytes,
                    ":cursor_score" : cursor.and_then(SearchCursorState::score),
                    ":cursor_last_seen_at" : cursor.map(SearchCursorState::last_seen_at),
                    ":cursor_snapshot_id" : cursor.map(SearchCursorState::snapshot_id),
                    ":limit" : fetch_limit,
                },
                map_scored_search_hit_row,
            )
        }
        .context("execute file-path literal search query")?;

        collect_rows(rows)
            .map(|hits| paginate_rows(hits, limit))
            .map(|page| {
                let has_more = page.has_more();
                SearchResults::new(SearchMode::Literal, page.into_items(), has_more)
            })
            .context("collect file-path literal search rows")
    }

    fn search_unfiltered_file_path_literal_page(
        &self,
        literal_match: &str,
        limit: usize,
    ) -> Result<SearchResults> {
        let fetch_limit = usize_to_i64(limit.saturating_add(1))?;
        let mut stmt = self
            .conn
            .prepare(FAST_FILE_PATH_LITERAL_QUERY)
            .context("prepare unfiltered file-path literal search query")?;
        let rows = stmt
            .query_map(
                named_params! {
                    ":literal_match" : literal_match,
                    ":limit" : fetch_limit,
                },
                map_scored_search_hit_row,
            )
            .context("execute unfiltered file-path literal search query")?;

        collect_rows(rows)
            .map(|hits| paginate_rows(hits, limit))
            .map(|page| {
                let has_more = page.has_more();
                SearchResults::new(SearchMode::Literal, page.into_items(), has_more)
            })
            .context("collect unfiltered file-path literal search rows")
    }

    fn search_unfiltered_text_literal_page(
        &self,
        analysis: &QueryAnalysis,
        token_match: &str,
        limit: usize,
    ) -> Result<SearchResults> {
        let fetch_limit = usize_to_i64(limit.saturating_add(1))?;
        let like = format!("%{}%", escape_like_pattern(&analysis.trimmed));
        let prefix_like = format!("{}%", escape_like_pattern(&analysis.lower));
        let mut stmt = self
            .conn
            .prepare(FAST_TEXT_LITERAL_QUERY)
            .context("prepare unfiltered text literal search query")?;
        let rows = stmt
            .query_map(
                named_params! {
                    ":token_match" : token_match,
                    ":query_lower" : analysis.lower.as_str(),
                    ":like" : like.as_str(),
                    ":prefix_like" : prefix_like.as_str(),
                    ":exact_phrase_lower" : analysis.exact_phrase.as_deref(),
                    ":limit" : fetch_limit,
                },
                map_scored_search_hit_row,
            )
            .context("execute unfiltered text literal search query")?;

        collect_rows(rows)
            .map(|hits| paginate_rows(hits, limit))
            .map(|page| {
                let has_more = page.has_more();
                SearchResults::new(SearchMode::Literal, page.into_items(), has_more)
            })
            .context("collect unfiltered text literal search rows")
    }

    /// Load one raw representation payload by snapshot id, item index, and UTI.
    ///
    /// # Errors
    ///
    /// Returns an error if the lookup query fails.
    pub fn find_representation_bytes(
        &self,
        snapshot_id: i64,
        item_index: usize,
        uti: &str,
    ) -> Result<Option<ClipboardRepresentation>> {
        let item_index = usize_to_i64(item_index)?;

        rusqlite::OptionalExtension::optional(self.conn.query_row(
            r"
                SELECT uti, kind, raw_sha256, text_value, blob_value
                FROM item_representations
                WHERE snapshot_id = ?1 AND item_index = ?2 AND uti = ?3
            ",
            params![snapshot_id, item_index, uti],
            |row| {
                Ok(ClipboardRepresentation::new(
                    row.get(0)?,
                    row_enum(row, 1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        ))
        .context("load representation bytes")
    }

    fn load_snapshot_summary(&self, snapshot_id: i64) -> Result<Option<SnapshotDetails>> {
        let summary_sql = r"
            SELECT
                s.id,
                s.sha256,
                s.snapshot_kind,
                s.preview_text,
                s.search_text,
                s.item_count,
                s.total_bytes,
                s.created_at,
                CASE
                    WHEN ss.snapshot_id IS NULL THEN (
                        SELECT COUNT(*)
                        FROM capture_events ce
                        WHERE ce.snapshot_id = s.id
                    )
                    ELSE ss.capture_count
                END AS capture_count,
                CASE
                    WHEN ss.snapshot_id IS NULL THEN (
                        SELECT MIN(ce.observed_at)
                        FROM capture_events ce
                        WHERE ce.snapshot_id = s.id
                    )
                    ELSE ss.first_observed_at
                END AS first_observed_at,
                CASE
                    WHEN ss.snapshot_id IS NULL THEN (
                        SELECT MAX(ce.observed_at)
                        FROM capture_events ce
                        WHERE ce.snapshot_id = s.id
                    )
                    ELSE ss.last_observed_at
                END AS last_observed_at,
                CASE
                    WHEN ss.snapshot_id IS NULL THEN (
                        SELECT ce.frontmost_app_name
                        FROM capture_events ce
                        WHERE ce.snapshot_id = s.id
                        ORDER BY ce.observed_at DESC, ce.id DESC
                        LIMIT 1
                    )
                    ELSE ss.last_frontmost_app_name
                END AS last_frontmost_app_name,
                CASE
                    WHEN ss.snapshot_id IS NULL THEN (
                        SELECT ce.frontmost_app_bundle_id
                        FROM capture_events ce
                        WHERE ce.snapshot_id = s.id
                        ORDER BY ce.observed_at DESC, ce.id DESC
                        LIMIT 1
                    )
                    ELSE ss.last_frontmost_app_bundle_id
                END AS last_frontmost_app_bundle_id,
                soc.ocr_text,
                soc.status AS ocr_status
            FROM snapshots s
            LEFT JOIN snapshot_stats ss ON ss.snapshot_id = s.id
            LEFT JOIN snapshot_ocr_cache soc ON soc.snapshot_id = s.id
            WHERE s.id = ?1
        ";

        rusqlite::OptionalExtension::optional(self.conn.query_row(
            summary_sql,
            [snapshot_id],
            |row| {
                Ok(SnapshotDetails::new(
                    row.get(0)?,
                    row.get(1)?,
                    row_enum(row, 2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row_usize(row, 5)?,
                    row_usize(row, 6)?,
                    row.get(7)?,
                    row_usize(row, 8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                    row.get(12)?,
                    row.get(13)?,
                    row.get(14)?,
                    Vec::new(),
                    Vec::new(),
                ))
            },
        ))
        .context("load snapshot summary")
    }

    fn load_recent_events(
        &self,
        snapshot_id: i64,
        event_limit: usize,
    ) -> Result<Vec<CaptureEvent>> {
        let limit = usize_to_i64(event_limit)?;
        let mut event_stmt = self
            .conn
            .prepare(
                r"
                SELECT id, observed_at, change_count, frontmost_app_name, frontmost_app_bundle_id
                FROM capture_events
                WHERE snapshot_id = ?1
                ORDER BY observed_at DESC, id DESC
                LIMIT ?2
            ",
            )
            .context("prepare snapshot events query")?;
        let event_rows = event_stmt
            .query_map(params![snapshot_id, limit], |row| {
                Ok(CaptureEvent::new(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .context("execute snapshot events query")?;

        collect_rows(event_rows).context("collect snapshot events")
    }

    fn load_snapshot_items(&self, snapshot_id: i64) -> Result<Vec<ClipboardItem>> {
        let mut item_stmt = self
            .conn
            .prepare(
                r"
                SELECT item_index, primary_kind, primary_uti, preview_text, search_text, total_bytes
                FROM snapshot_items
                WHERE snapshot_id = ?1
                ORDER BY item_index ASC
            ",
            )
            .context("prepare snapshot items query")?;

        let item_rows = item_stmt
            .query_map([snapshot_id], |row| {
                Ok(ClipboardItem::new(
                    row_usize(row, 0)?,
                    row_enum(row, 1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    Vec::new(),
                ))
            })
            .context("execute snapshot items query")?;
        let mut items = collect_rows(item_rows).context("collect snapshot items")?;

        let mut rep_stmt = self
            .conn
            .prepare(
                r"
                SELECT
                    uti,
                    kind,
                    raw_sha256,
                    text_value,
                    blob_value
                FROM item_representations
                WHERE snapshot_id = ?1 AND item_index = ?2
                ORDER BY uti ASC
            ",
            )
            .context("prepare representation query")?;

        for item in &mut items {
            let rep_rows = rep_stmt
                .query_map(
                    params![snapshot_id, usize_to_i64(item.item_index())?],
                    |row| {
                        Ok(ClipboardRepresentation::new(
                            row.get(0)?,
                            row_enum(row, 1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    },
                )
                .with_context(|| {
                    format!(
                        "execute representation query for item {}",
                        item.item_index()
                    )
                })?;
            item.set_representations(collect_rows(rep_rows).with_context(|| {
                format!("collect representations for item {}", item.item_index())
            })?);
        }

        Ok(items)
    }
}

struct StatsQueryParams {
    since: Option<String>,
    until: Option<String>,
    app_like: Option<String>,
    bundle_id: Option<String>,
    kind: Option<&'static str>,
    has_text: bool,
    has_url: bool,
    has_file_url: bool,
    has_image: bool,
    has_pdf: bool,
    min_bytes: Option<i64>,
    max_bytes: Option<i64>,
}

fn stats_params(filters: &RetrievalFilters) -> Result<StatsQueryParams> {
    Ok(StatsQueryParams {
        since: effective_since_param(filters)?,
        until: filters.until().map(ToOwned::to_owned),
        app_like: app_like_pattern(filters),
        bundle_id: filters.bundle_id().map(|value| value.to_ascii_lowercase()),
        kind: filters.kind().map(RetrievalKindExt::as_static_str),
        has_text: filters.has_text(),
        has_url: filters.has_url(),
        has_file_url: filters.has_file_url(),
        has_image: filters.has_image(),
        has_pdf: filters.has_pdf(),
        min_bytes: filters.min_bytes().map(usize_to_i64).transpose()?,
        max_bytes: filters.max_bytes().map(usize_to_i64).transpose()?,
    })
}

trait RetrievalKindExt {
    fn as_static_str(self) -> &'static str;
}

impl RetrievalKindExt for super::RetrievalKind {
    fn as_static_str(self) -> &'static str {
        self.as_str()
    }
}

#[derive(Debug)]
struct StatsOverview {
    snapshot_count: usize,
    capture_event_count: usize,
    unique_app_count: usize,
    total_bytes: usize,
    first_observed_at: Option<String>,
    last_observed_at: Option<String>,
    archive_span_seconds: Option<i64>,
}

impl StatsOverview {
    fn average_bytes_per_snapshot(&self) -> f64 {
        if self.snapshot_count == 0 {
            0.0
        } else {
            self.total_bytes as f64 / self.snapshot_count as f64
        }
    }

    fn average_captures_per_snapshot(&self) -> f64 {
        if self.snapshot_count == 0 {
            0.0
        } else {
            self.capture_event_count as f64 / self.snapshot_count as f64
        }
    }

    fn dedupe_ratio(&self) -> f64 {
        if self.capture_event_count == 0 {
            0.0
        } else {
            1.0 - (self.snapshot_count as f64 / self.capture_event_count as f64)
        }
    }
}

impl Database {
    fn load_unfiltered_stats_overview(&self) -> Result<StatsOverview> {
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

    fn load_unfiltered_stats_kind_breakdown(&self) -> Result<Vec<StatsKindBreakdownEntry>> {
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

    fn load_unfiltered_stats_top_apps(&self) -> Result<Vec<StatsAppEntry>> {
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

    fn load_unfiltered_stats_hours(&self) -> Result<Vec<StatsTimeBucketEntry>> {
        self.load_unfiltered_stats_time_distribution(
            "00",
            24,
            "strftime('%H', observed_at)",
            |index| format!("{index:02}"),
        )
    }

    fn load_unfiltered_stats_weekdays(&self) -> Result<Vec<StatsTimeBucketEntry>> {
        self.load_unfiltered_stats_time_distribution(
            "0",
            7,
            "strftime('%w', observed_at)",
            |index| weekday_name(index).to_string(),
        )
    }

    fn load_unfiltered_stats_time_distribution(
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

    fn load_unfiltered_stats_snapshot_leaderboard(
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

    fn load_stats_overview(&self) -> Result<StatsOverview> {
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

    fn load_stats_kind_breakdown(&self) -> Result<Vec<StatsKindBreakdownEntry>> {
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

    fn load_stats_top_apps(&self) -> Result<Vec<StatsAppEntry>> {
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

    fn load_stats_hours(&self) -> Result<Vec<StatsTimeBucketEntry>> {
        self.load_stats_time_distribution("00", 24, "strftime('%H', observed_at)", |index| {
            format!("{index:02}")
        })
    }

    fn load_stats_weekdays(&self) -> Result<Vec<StatsTimeBucketEntry>> {
        self.load_stats_time_distribution("0", 7, "strftime('%w', observed_at)", |index| {
            weekday_name(index).to_string()
        })
    }

    fn load_stats_time_distribution(
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

    fn load_stats_snapshot_leaderboard(
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

fn recent_query(
    include_matching_events: bool,
    use_snapshot_event_cache: bool,
    use_snapshot_stats_since_filter: bool,
) -> String {
    format!(
        "{matching_events_cte}
         SELECT
             base.snapshot_id,
             base.event_id,
             base.sha256,
             base.snapshot_kind,
             base.preview_text,
             base.search_text,
             base.why_matched,
             base.matched_fields,
             base.capture_count,
             base.first_observed_at,
             base.last_observed_at,
             base.last_frontmost_app_name,
             base.last_frontmost_app_bundle_id,
             base.urls,
             base.file_urls,
             base.total_bytes,
             base.item_count,
             base.score
         FROM (
             SELECT
                 s.id AS snapshot_id,
                 ss.last_event_id AS event_id,
                 s.sha256 AS sha256,
                 s.snapshot_kind AS snapshot_kind,
                 COALESCE(NULLIF(s.preview_text, ''), soc.ocr_text, '') AS preview_text,
                 s.search_text AS search_text,
                 NULL AS why_matched,
                 '' AS matched_fields,
                 ss.capture_count AS capture_count,
                 ss.first_observed_at AS first_observed_at,
                 ss.last_observed_at AS last_observed_at,
                 ss.last_frontmost_app_name AS last_frontmost_app_name,
                 ss.last_frontmost_app_bundle_id AS last_frontmost_app_bundle_id,
                 COALESCE(sp.urls, '') AS urls,
                 COALESCE(sp.file_urls, '') AS file_urls,
                 s.total_bytes AS total_bytes,
                 s.item_count AS item_count,
                 NULL AS score
             FROM snapshots s
             JOIN snapshot_stats ss ON ss.snapshot_id = s.id
             {matching_events_join}
             LEFT JOIN snapshot_projection_cache sp ON sp.snapshot_id = s.id
             LEFT JOIN snapshot_ocr_cache soc ON soc.snapshot_id = s.id
             LEFT JOIN snapshot_event_filter_cache se ON se.snapshot_id = s.id
             WHERE {snapshot_filter_clause}
               AND {base_event_filter_clause}
               AND {snapshot_stats_since_filter_clause}
         ) base
         WHERE (
             :cursor_last_seen_at IS NULL
             OR base.last_observed_at < :cursor_last_seen_at
             OR (base.last_observed_at = :cursor_last_seen_at AND base.snapshot_id < :cursor_snapshot_id)
         )
         ORDER BY base.last_observed_at DESC, base.snapshot_id DESC
         LIMIT :limit",
        matching_events_cte = matching_events_cte(include_matching_events),
        matching_events_join = matching_events_join(include_matching_events),
        base_event_filter_clause = base_event_filter_clause(
            "se",
            include_matching_events,
            use_snapshot_event_cache,
        ),
        snapshot_stats_since_filter_clause =
            snapshot_stats_since_filter_clause(use_snapshot_stats_since_filter),
        snapshot_filter_clause = snapshot_filter_clause("s", "s.id"),
    )
}

fn timeline_query(sort: TimelineSort) -> String {
    let cursor_predicate = match sort {
        TimelineSort::Desc => {
            r"(
                :cursor_observed_at IS NULL
                OR ce.observed_at < :cursor_observed_at
                OR (ce.observed_at = :cursor_observed_at AND ce.id < :cursor_event_id)
            )"
        }
        TimelineSort::Asc => {
            r"(
                :cursor_observed_at IS NULL
                OR ce.observed_at > :cursor_observed_at
                OR (ce.observed_at = :cursor_observed_at AND ce.id > :cursor_event_id)
            )"
        }
    };
    let ordering = match sort {
        TimelineSort::Desc => "ce.observed_at DESC, ce.id DESC",
        TimelineSort::Asc => "ce.observed_at ASC, ce.id ASC",
    };

    format!(
        r"
         SELECT
             ce.id AS event_id,
             ce.snapshot_id AS snapshot_id,
             ce.observed_at AS observed_at,
             ce.change_count AS change_count,
             s.sha256 AS sha256,
             s.snapshot_kind AS snapshot_kind,
             COALESCE(NULLIF(s.preview_text, ''), NULLIF(soc.ocr_text, ''), s.search_text, '') AS best_text,
             COALESCE(NULLIF(s.preview_text, ''), soc.ocr_text, '') AS preview_text,
             ce.frontmost_app_name AS frontmost_app_name,
             ce.frontmost_app_bundle_id AS frontmost_app_bundle_id,
             COALESCE(sp.urls, '') AS urls,
             COALESCE(sp.file_urls, '') AS file_urls,
             s.total_bytes AS total_bytes,
             s.item_count AS item_count
         FROM capture_events ce
         JOIN snapshots s ON s.id = ce.snapshot_id
         LEFT JOIN snapshot_projection_cache sp ON sp.snapshot_id = ce.snapshot_id
         LEFT JOIN snapshot_ocr_cache soc ON soc.snapshot_id = ce.snapshot_id
         WHERE {event_filter_clause}
           AND {snapshot_filter_clause}
           AND {cursor_predicate}
         ORDER BY {ordering}
         LIMIT :limit",
        event_filter_clause = event_filter_clause("ce"),
        snapshot_filter_clause = snapshot_filter_clause("s", "ce.snapshot_id"),
    )
}

fn literal_query(
    include_matching_events: bool,
    use_snapshot_event_cache: bool,
    use_literal_fts: bool,
) -> String {
    let mut ctes = Vec::new();
    let literal_candidates_join = if use_literal_fts {
        ctes.push(
            "literal_candidates AS (
                 SELECT rowid AS snapshot_id
                 FROM snapshots_literal_fts
                 WHERE snapshots_literal_fts MATCH :literal_match
             )"
            .to_string(),
        );
        "JOIN literal_candidates lc ON lc.snapshot_id = s.id"
    } else {
        ""
    };
    if include_matching_events {
        ctes.push(format!(
            "matching_events AS (
                 SELECT DISTINCT ce.snapshot_id
                 FROM capture_events ce
                 WHERE {}
             )",
            event_filter_clause("ce")
        ));
    }
    let with_clause = if ctes.is_empty() {
        String::new()
    } else {
        format!("WITH {}", ctes.join(", "))
    };
    format!(
        "{with_clause}
         SELECT
             base.snapshot_id,
             base.event_id,
             base.sha256,
             base.snapshot_kind,
             base.preview_text,
             base.search_text,
             base.why_matched,
             base.matched_fields,
             base.capture_count,
             base.first_observed_at,
             base.last_observed_at,
             base.last_frontmost_app_name,
             base.last_frontmost_app_bundle_id,
             base.urls,
             base.file_urls,
             base.total_bytes,
             base.item_count,
             base.score
         FROM (
             SELECT
                 s.id AS snapshot_id,
                 ss.last_event_id AS event_id,
                 s.sha256 AS sha256,
                 s.snapshot_kind AS snapshot_kind,
                 s.preview_text AS preview_text,
                 s.search_text AS search_text,
                 CASE
                     WHEN lower(COALESCE(sp.urls, '')) = :query_lower THEN 'Exact URL match'
                     WHEN :path_fragment_like IS NOT NULL AND lower(COALESCE(sp.file_urls, '')) LIKE :path_fragment_like ESCAPE '\\' THEN 'Path fragment match in file paths'
                     WHEN lower(COALESCE(ss.last_frontmost_app_bundle_id, '')) = :query_lower THEN 'Bundle ID match'
                     WHEN :exact_phrase_lower IS NOT NULL AND lower(COALESCE(NULLIF(s.preview_text, ''), s.search_text, '')) LIKE ('%' || :exact_phrase_lower || '%') ESCAPE '\\' THEN 'Exact phrase match in best text'
                     WHEN lower(COALESCE(NULLIF(s.preview_text, ''), s.search_text, '')) = :query_lower THEN 'Exact text match in best text'
                     WHEN lower(COALESCE(NULLIF(s.preview_text, ''), s.search_text, '')) LIKE :prefix_like ESCAPE '\\' THEN 'Prefix match in best text'
                     WHEN lower(COALESCE(NULLIF(s.preview_text, ''), s.search_text, '')) LIKE :like ESCAPE '\\' THEN 'Literal text match in best text'
                     WHEN lower(COALESCE(s.search_text, '')) LIKE :like ESCAPE '\\' THEN 'Literal search-text match'
                     WHEN lower(COALESCE(ss.last_frontmost_app_name, '')) LIKE :like ESCAPE '\\' THEN 'App name match'
                     ELSE 'Literal field match'
                 END AS why_matched,
                 TRIM(
                     CASE WHEN lower(COALESCE(NULLIF(s.preview_text, ''), '')) LIKE :like ESCAPE '\\' THEN 'best_text' || char(30) ELSE '' END ||
                     CASE WHEN lower(COALESCE(s.preview_text, '')) LIKE :like ESCAPE '\\' THEN 'preview_text' || char(30) ELSE '' END ||
                     CASE WHEN lower(COALESCE(s.search_text, '')) LIKE :like ESCAPE '\\' THEN 'search_text' || char(30) ELSE '' END ||
                     CASE WHEN lower(COALESCE(sp.urls, '')) LIKE :like ESCAPE '\\' OR lower(COALESCE(sp.urls, '')) = :query_lower THEN 'urls' || char(30) ELSE '' END ||
                     CASE WHEN :path_fragment_like IS NOT NULL AND lower(COALESCE(sp.file_urls, '')) LIKE :path_fragment_like ESCAPE '\\' THEN 'file_paths' || char(30) ELSE '' END ||
                     CASE WHEN lower(COALESCE(ss.last_frontmost_app_name, '')) LIKE :like ESCAPE '\\' THEN 'app_name' || char(30) ELSE '' END ||
                     CASE WHEN lower(COALESCE(ss.last_frontmost_app_bundle_id, '')) LIKE :like ESCAPE '\\' OR lower(COALESCE(ss.last_frontmost_app_bundle_id, '')) = :query_lower THEN 'app_bundle_id' || char(30) ELSE '' END,
                     char(30)
                 ) AS matched_fields,
                 ss.capture_count AS capture_count,
                 ss.first_observed_at AS first_observed_at,
                 ss.last_observed_at AS last_observed_at,
                 ss.last_frontmost_app_name AS last_frontmost_app_name,
                 ss.last_frontmost_app_bundle_id AS last_frontmost_app_bundle_id,
                 COALESCE(sp.urls, '') AS urls,
                 COALESCE(sp.file_urls, '') AS file_urls,
                 s.total_bytes AS total_bytes,
                 s.item_count AS item_count,
                 (
                     CASE WHEN lower(COALESCE(sp.urls, '')) = :query_lower THEN 1.16 ELSE 0 END +
                     CASE WHEN :path_fragment_like IS NOT NULL AND lower(COALESCE(sp.file_urls, '')) LIKE :path_fragment_like ESCAPE '\\' THEN 1.12 ELSE 0 END +
                     CASE WHEN lower(COALESCE(ss.last_frontmost_app_bundle_id, '')) = :query_lower THEN 1.08 ELSE 0 END +
                     CASE WHEN :exact_phrase_lower IS NOT NULL AND lower(COALESCE(NULLIF(s.preview_text, ''), s.search_text, '')) LIKE ('%' || :exact_phrase_lower || '%') ESCAPE '\\' THEN 0.98 ELSE 0 END +
                     CASE WHEN lower(COALESCE(NULLIF(s.preview_text, ''), s.search_text, '')) = :query_lower THEN 0.96 ELSE 0 END +
                     CASE WHEN lower(COALESCE(NULLIF(s.preview_text, ''), s.search_text, '')) LIKE :prefix_like ESCAPE '\\' THEN 0.88 ELSE 0 END +
                     CASE WHEN lower(COALESCE(NULLIF(s.preview_text, ''), s.search_text, '')) LIKE :like ESCAPE '\\' THEN 0.78 ELSE 0 END +
                     CASE WHEN lower(COALESCE(s.search_text, '')) LIKE :like ESCAPE '\\' THEN 0.72 ELSE 0 END +
                     CASE WHEN lower(COALESCE(sp.urls, '')) LIKE :like ESCAPE '\\' THEN 0.9 ELSE 0 END +
                     CASE WHEN lower(COALESCE(ss.last_frontmost_app_bundle_id, '')) LIKE :like ESCAPE '\\' THEN 0.84 ELSE 0 END +
                     CASE WHEN lower(COALESCE(ss.last_frontmost_app_name, '')) LIKE :like ESCAPE '\\' THEN 0.7 ELSE 0 END +
                     CASE
                         WHEN datetime(ss.last_observed_at) >= datetime('now', '-24 hours') THEN 0.05
                         WHEN datetime(ss.last_observed_at) >= datetime('now', '-7 days') THEN 0.02
                         ELSE 0
                     END
                 ) AS score
             FROM snapshots s
             {literal_candidates_join}
             JOIN snapshot_stats ss ON ss.snapshot_id = s.id
             {matching_events_join}
             LEFT JOIN snapshot_projection_cache sp ON sp.snapshot_id = s.id
             LEFT JOIN snapshot_event_filter_cache se ON se.snapshot_id = s.id
             WHERE (
                    lower(COALESCE(s.search_text, '')) LIKE :like ESCAPE '\\'
                 OR lower(COALESCE(s.preview_text, '')) LIKE :like ESCAPE '\\'
                 OR lower(COALESCE(sp.urls, '')) LIKE :like ESCAPE '\\'
                 OR lower(COALESCE(ss.last_frontmost_app_name, '')) LIKE :like ESCAPE '\\'
                 OR lower(COALESCE(ss.last_frontmost_app_bundle_id, '')) LIKE :like ESCAPE '\\'
                 OR (:path_fragment_like IS NOT NULL AND lower(COALESCE(sp.file_urls, '')) LIKE :path_fragment_like ESCAPE '\\')
               )
               AND {snapshot_filter_clause}
               AND {base_event_filter_clause}
         ) base
         WHERE (
             :cursor_score IS NULL
             OR base.score < :cursor_score
             OR (
                 base.score = :cursor_score
                 AND (
                     base.last_observed_at < :cursor_last_seen_at
                     OR (base.last_observed_at = :cursor_last_seen_at AND base.snapshot_id < :cursor_snapshot_id)
                 )
             )
         )
         ORDER BY base.score DESC, base.last_observed_at DESC, base.snapshot_id DESC
         LIMIT :limit",
        with_clause = with_clause,
        literal_candidates_join = literal_candidates_join,
        matching_events_join = matching_events_join(include_matching_events),
        base_event_filter_clause = base_event_filter_clause(
            "se",
            include_matching_events,
            use_snapshot_event_cache,
        ),
        snapshot_filter_clause = snapshot_filter_clause("s", "s.id"),
    )
}

fn file_path_literal_query(
    include_matching_events: bool,
    use_snapshot_event_cache: bool,
    use_literal_fts: bool,
) -> String {
    let mut ctes = Vec::new();
    let path_candidates_join = if use_literal_fts {
        ctes.push(
            "path_candidates AS (
                 SELECT rowid AS snapshot_id
                 FROM snapshot_file_url_fts
                 WHERE snapshot_file_url_fts MATCH :literal_match
             )"
            .to_string(),
        );
        "JOIN path_candidates pc ON pc.snapshot_id = s.id"
    } else {
        ""
    };
    if include_matching_events {
        ctes.push(format!(
            "matching_events AS (
                 SELECT DISTINCT ce.snapshot_id
                 FROM capture_events ce
                 WHERE {}
             )",
            event_filter_clause("ce")
        ));
    }
    let with_clause = if ctes.is_empty() {
        String::new()
    } else {
        format!("WITH {}", ctes.join(", "))
    };

    format!(
        "{with_clause}
         SELECT
             base.snapshot_id,
             base.event_id,
             base.sha256,
             base.snapshot_kind,
             base.preview_text,
             base.search_text,
             base.why_matched,
             base.matched_fields,
             base.capture_count,
             base.first_observed_at,
             base.last_observed_at,
             base.last_frontmost_app_name,
             base.last_frontmost_app_bundle_id,
             base.urls,
             base.file_urls,
             base.total_bytes,
             base.item_count,
             base.score
         FROM (
             SELECT
                 s.id AS snapshot_id,
                 ss.last_event_id AS event_id,
                 s.sha256 AS sha256,
                 s.snapshot_kind AS snapshot_kind,
                 s.preview_text AS preview_text,
                 s.search_text AS search_text,
                 'Path fragment match in file paths' AS why_matched,
                 'file_paths' AS matched_fields,
                 ss.capture_count AS capture_count,
                 ss.first_observed_at AS first_observed_at,
                 ss.last_observed_at AS last_observed_at,
                 ss.last_frontmost_app_name AS last_frontmost_app_name,
                 ss.last_frontmost_app_bundle_id AS last_frontmost_app_bundle_id,
                 COALESCE(sp.urls, '') AS urls,
                 COALESCE(sp.file_urls, '') AS file_urls,
                 s.total_bytes AS total_bytes,
                 s.item_count AS item_count,
                 (
                     CASE
                         WHEN instr(lower(COALESCE(sp.file_urls, '')), :path_fragment) > 0
                             THEN 1.12
                         ELSE 0
                     END +
                     CASE
                         WHEN datetime(ss.last_observed_at) >= datetime('now', '-24 hours') THEN 0.05
                         WHEN datetime(ss.last_observed_at) >= datetime('now', '-7 days') THEN 0.02
                         ELSE 0
                     END
                 ) AS score
             FROM snapshots s
             {path_candidates_join}
             JOIN snapshot_stats ss ON ss.snapshot_id = s.id
             {matching_events_join}
             JOIN snapshot_projection_cache sp ON sp.snapshot_id = s.id
             LEFT JOIN snapshot_event_filter_cache se ON se.snapshot_id = s.id
             WHERE lower(COALESCE(sp.file_urls, '')) LIKE :path_like ESCAPE '\\'
               AND {snapshot_filter_clause}
               AND {base_event_filter_clause}
         ) base
         WHERE (
             :cursor_score IS NULL
             OR base.score < :cursor_score
             OR (
                 base.score = :cursor_score
                 AND (
                     base.last_observed_at < :cursor_last_seen_at
                     OR (base.last_observed_at = :cursor_last_seen_at AND base.snapshot_id < :cursor_snapshot_id)
                 )
             )
         )
         ORDER BY base.score DESC, base.last_observed_at DESC, base.snapshot_id DESC
         LIMIT :limit",
        with_clause = with_clause,
        path_candidates_join = path_candidates_join,
        matching_events_join = matching_events_join(include_matching_events),
        snapshot_filter_clause = snapshot_filter_clause("s", "s.id"),
        base_event_filter_clause = base_event_filter_clause(
            "se",
            include_matching_events,
            use_snapshot_event_cache,
        ),
    )
}

fn fts_query(
    use_snapshot_event_cache: bool,
    has_temporal_event_filters: bool,
    use_simple_scoring: bool,
) -> String {
    let why_matched_expression = if use_simple_scoring {
        "NULL"
    } else {
        r"CASE
                     WHEN :exact_phrase_lower IS NOT NULL AND lower(COALESCE(NULLIF(s.preview_text, ''), s.search_text, '')) LIKE ('%' || :exact_phrase_lower || '%') ESCAPE '\'
                         THEN 'Exact phrase match in best text'
                     WHEN lower(COALESCE(NULLIF(s.preview_text, ''), s.search_text, '')) = :query_lower
                         THEN 'Exact text match in best text'
                     WHEN lower(COALESCE(NULLIF(s.preview_text, ''), s.search_text, '')) LIKE ('%' || :query_lower || '%') ESCAPE '\'
                         THEN 'Phrase match in best text'
                     ELSE NULL
                 END"
    };
    let matched_fields_expression = if use_simple_scoring {
        "'search_text'"
    } else {
        r"CASE
                     WHEN :exact_phrase_lower IS NOT NULL AND lower(COALESCE(NULLIF(s.preview_text, ''), s.search_text, '')) LIKE ('%' || :exact_phrase_lower || '%') ESCAPE '\'
                         THEN 'best_text' || char(30) || 'search_text'
                     WHEN lower(COALESCE(NULLIF(s.preview_text, ''), s.search_text, '')) LIKE ('%' || :query_lower || '%') ESCAPE '\'
                         THEN 'best_text' || char(30) || 'search_text'
                     ELSE 'search_text'
                 END"
    };
    let score_expression = if use_simple_scoring {
        "bm25(snapshots_fts)"
    } else {
        r"(
                     bm25(snapshots_fts)
                     - CASE
                         WHEN :exact_phrase_lower IS NOT NULL AND lower(COALESCE(NULLIF(s.preview_text, ''), s.search_text, '')) LIKE ('%' || :exact_phrase_lower || '%') ESCAPE '\' THEN 0.2
                         WHEN lower(COALESCE(NULLIF(s.preview_text, ''), s.search_text, '')) LIKE ('%' || :query_lower || '%') ESCAPE '\' THEN 0.08
                         ELSE 0
                     END
                 )"
    };

    format!(
        "
         WITH search_rows AS (
             SELECT
                 s.id AS snapshot_id,
                 ss.last_event_id AS event_id,
                 s.sha256 AS sha256,
                 s.snapshot_kind AS snapshot_kind,
                 s.preview_text AS preview_text,
                 s.search_text AS search_text,
                 {why_matched_expression} AS why_matched,
                 {matched_fields_expression} AS matched_fields,
                 ss.capture_count AS capture_count,
                 ss.first_observed_at AS first_observed_at,
                 ss.last_observed_at AS last_observed_at,
                 ss.last_frontmost_app_name AS last_frontmost_app_name,
                 ss.last_frontmost_app_bundle_id AS last_frontmost_app_bundle_id,
                 s.total_bytes AS total_bytes,
                 s.item_count AS item_count,
                 {score_expression} AS score
             FROM snapshots_fts
             JOIN snapshots s ON s.id = snapshots_fts.rowid
             JOIN snapshot_stats ss ON ss.snapshot_id = s.id
             LEFT JOIN snapshot_event_filter_cache se ON se.snapshot_id = s.id
             WHERE snapshots_fts MATCH :query
               AND (:query_lower IS NULL OR :query_lower IS NOT NULL)
               AND (:exact_phrase_lower IS NULL OR :exact_phrase_lower IS NOT NULL)
               AND {snapshot_filter_clause}
               AND {event_filter_where_clause}
         )
         , ranked_rows AS (
             SELECT
                 search_rows.snapshot_id,
                 search_rows.event_id,
                 search_rows.sha256,
                 search_rows.snapshot_kind,
                 search_rows.preview_text,
                 search_rows.search_text,
                 search_rows.why_matched,
                 search_rows.matched_fields,
                 search_rows.capture_count,
                 search_rows.first_observed_at,
                 search_rows.last_observed_at,
                 search_rows.last_frontmost_app_name,
                 search_rows.last_frontmost_app_bundle_id,
                 search_rows.total_bytes,
                 search_rows.item_count,
                 search_rows.score
             FROM search_rows
             WHERE (
                 :cursor_score IS NULL
                 OR search_rows.score > :cursor_score
                 OR (
                     search_rows.score = :cursor_score
                     AND (
                         search_rows.last_observed_at < :cursor_last_seen_at
                         OR (
                             search_rows.last_observed_at = :cursor_last_seen_at
                             AND search_rows.snapshot_id < :cursor_snapshot_id
                         )
                     )
                 )
             )
             ORDER BY search_rows.score ASC, search_rows.last_observed_at DESC, search_rows.snapshot_id DESC
             LIMIT :limit
         )
         SELECT
             ranked_rows.snapshot_id,
             ranked_rows.event_id,
             ranked_rows.sha256,
             ranked_rows.snapshot_kind,
             ranked_rows.preview_text,
             ranked_rows.search_text,
             COALESCE(
                 ranked_rows.why_matched,
                 snippet(snapshots_fts, 0, '⟦', '⟧', ' … ', 24),
                 ranked_rows.preview_text
             ) AS why_matched,
             ranked_rows.matched_fields,
             ranked_rows.capture_count,
             ranked_rows.first_observed_at,
             ranked_rows.last_observed_at,
             ranked_rows.last_frontmost_app_name,
             ranked_rows.last_frontmost_app_bundle_id,
             COALESCE(sp.urls, '') AS urls,
             COALESCE(sp.file_urls, '') AS file_urls,
             ranked_rows.total_bytes,
             ranked_rows.item_count,
             ranked_rows.score
         FROM ranked_rows
         JOIN snapshots_fts ON snapshots_fts.rowid = ranked_rows.snapshot_id
         LEFT JOIN snapshot_projection_cache sp ON sp.snapshot_id = ranked_rows.snapshot_id
         ORDER BY ranked_rows.score ASC, ranked_rows.last_observed_at DESC, ranked_rows.snapshot_id DESC",
        event_filter_where_clause = event_filter_where_clause(
            "s.id",
            "se",
            use_snapshot_event_cache,
            has_temporal_event_filters,
        ),
        snapshot_filter_clause = snapshot_filter_clause("s", "s.id"),
        why_matched_expression = why_matched_expression,
        matched_fields_expression = matched_fields_expression,
        score_expression = score_expression,
    )
}

fn ocr_fts_query(use_snapshot_event_cache: bool, has_temporal_event_filters: bool) -> String {
    format!(
        "
         SELECT
             s.id AS snapshot_id,
             ss.last_event_id AS event_id,
             s.sha256 AS sha256,
             s.snapshot_kind AS snapshot_kind,
             COALESCE(NULLIF(s.preview_text, ''), soc.ocr_text, '') AS preview_text,
             s.search_text AS search_text,
             COALESCE(snippet(snapshot_ocr_fts, 0, '⟦', '⟧', ' … ', 24), 'OCR text match') AS why_matched,
             'ocr_text' AS matched_fields,
             ss.capture_count AS capture_count,
             ss.first_observed_at AS first_observed_at,
             ss.last_observed_at AS last_observed_at,
             ss.last_frontmost_app_name AS last_frontmost_app_name,
             ss.last_frontmost_app_bundle_id AS last_frontmost_app_bundle_id,
             COALESCE(sp.urls, '') AS urls,
             COALESCE(sp.file_urls, '') AS file_urls,
             s.total_bytes AS total_bytes,
             s.item_count AS item_count,
             bm25(snapshot_ocr_fts) + 0.1 AS score
         FROM snapshot_ocr_fts
         JOIN snapshot_ocr_cache soc ON soc.snapshot_id = snapshot_ocr_fts.rowid
         JOIN snapshots s ON s.id = snapshot_ocr_fts.rowid
         JOIN snapshot_stats ss ON ss.snapshot_id = s.id
         LEFT JOIN snapshot_projection_cache sp ON sp.snapshot_id = s.id
         LEFT JOIN snapshot_event_filter_cache se ON se.snapshot_id = s.id
         WHERE snapshot_ocr_fts MATCH :query
           AND soc.ocr_text != ''
           AND {snapshot_filter_clause}
           AND {event_filter_where_clause}
           AND (
               :cursor_score IS NULL
               OR bm25(snapshot_ocr_fts) + 0.1 > :cursor_score
               OR (
                   bm25(snapshot_ocr_fts) + 0.1 = :cursor_score
                   AND (
                       ss.last_observed_at < :cursor_last_seen_at
                       OR (ss.last_observed_at = :cursor_last_seen_at AND s.id < :cursor_snapshot_id)
                   )
               )
           )
         ORDER BY score ASC, ss.last_observed_at DESC, s.id DESC
         LIMIT :limit",
        event_filter_where_clause = event_filter_where_clause(
            "s.id",
            "se",
            use_snapshot_event_cache,
            has_temporal_event_filters,
        ),
        snapshot_filter_clause = snapshot_filter_clause("s", "s.id"),
    )
}

fn ocr_literal_query(
    include_matching_events: bool,
    use_snapshot_event_cache: bool,
    use_literal_fts: bool,
) -> String {
    let mut ctes = Vec::new();
    let literal_candidates_join = if use_literal_fts {
        ctes.push(
            "ocr_literal_candidates AS (
                 SELECT rowid AS snapshot_id
                 FROM snapshot_ocr_literal_fts
                 WHERE snapshot_ocr_literal_fts MATCH :literal_match
             )"
            .to_string(),
        );
        "JOIN ocr_literal_candidates olc ON olc.snapshot_id = s.id"
    } else {
        ""
    };
    if include_matching_events {
        ctes.push(format!(
            "matching_events AS (
                 SELECT DISTINCT ce.snapshot_id
                 FROM capture_events ce
                 WHERE {}
             )",
            event_filter_clause("ce")
        ));
    }
    let with_clause = if ctes.is_empty() {
        String::new()
    } else {
        format!("WITH {}", ctes.join(", "))
    };
    format!(
        "{with_clause}
         SELECT
             s.id AS snapshot_id,
             ss.last_event_id AS event_id,
             s.sha256 AS sha256,
             s.snapshot_kind AS snapshot_kind,
             COALESCE(NULLIF(s.preview_text, ''), soc.ocr_text, '') AS preview_text,
             s.search_text AS search_text,
             CASE
                 WHEN :exact_phrase_lower IS NOT NULL AND lower(soc.ocr_text) LIKE ('%' || :exact_phrase_lower || '%') ESCAPE '\\' THEN 'Exact phrase match in OCR text'
                 WHEN lower(soc.ocr_text) = :query_lower THEN 'Exact OCR text match'
                 WHEN lower(soc.ocr_text) LIKE :prefix_like ESCAPE '\\' THEN 'OCR text prefix match'
                 ELSE 'OCR text match'
             END AS why_matched,
             'ocr_text' AS matched_fields,
             ss.capture_count AS capture_count,
             ss.first_observed_at AS first_observed_at,
             ss.last_observed_at AS last_observed_at,
             ss.last_frontmost_app_name AS last_frontmost_app_name,
             ss.last_frontmost_app_bundle_id AS last_frontmost_app_bundle_id,
             COALESCE(sp.urls, '') AS urls,
             COALESCE(sp.file_urls, '') AS file_urls,
             s.total_bytes AS total_bytes,
             s.item_count AS item_count,
             (
                 CASE WHEN :exact_phrase_lower IS NOT NULL AND lower(soc.ocr_text) LIKE ('%' || :exact_phrase_lower || '%') ESCAPE '\\' THEN 0.82 ELSE 0 END +
                 CASE WHEN lower(soc.ocr_text) = :query_lower THEN 0.8 ELSE 0 END +
                 CASE WHEN lower(soc.ocr_text) LIKE :prefix_like ESCAPE '\\' THEN 0.74 ELSE 0 END +
                 CASE WHEN lower(soc.ocr_text) LIKE :like ESCAPE '\\' THEN 0.68 ELSE 0 END +
                 CASE
                     WHEN datetime(ss.last_observed_at) >= datetime('now', '-24 hours') THEN 0.05
                     WHEN datetime(ss.last_observed_at) >= datetime('now', '-7 days') THEN 0.02
                     ELSE 0
                 END
             ) AS score
         FROM snapshot_ocr_cache soc
         JOIN snapshots s ON s.id = soc.snapshot_id
         {literal_candidates_join}
         JOIN snapshot_stats ss ON ss.snapshot_id = s.id
         {matching_events_join}
         LEFT JOIN snapshot_projection_cache sp ON sp.snapshot_id = s.id
         LEFT JOIN snapshot_event_filter_cache se ON se.snapshot_id = s.id
         WHERE soc.ocr_text != ''
           AND lower(soc.ocr_text) LIKE :like ESCAPE '\\'
           AND (:literal_match IS NULL OR :literal_match IS NOT NULL)
           AND {snapshot_filter_clause}
           AND {base_event_filter_clause}
           AND (
               :cursor_score IS NULL
               OR score < :cursor_score
               OR (
                   score = :cursor_score
                   AND (
                       ss.last_observed_at < :cursor_last_seen_at
                       OR (ss.last_observed_at = :cursor_last_seen_at AND s.id < :cursor_snapshot_id)
                   )
               )
           )
         ORDER BY score DESC, ss.last_observed_at DESC, s.id DESC
         LIMIT :limit",
        with_clause = with_clause,
        literal_candidates_join = literal_candidates_join,
        matching_events_join = matching_events_join(include_matching_events),
        base_event_filter_clause = base_event_filter_clause(
            "se",
            include_matching_events,
            use_snapshot_event_cache,
        ),
        snapshot_filter_clause = snapshot_filter_clause("s", "s.id"),
    )
}

fn matching_events_cte(include_matching_events: bool) -> String {
    if include_matching_events {
        format!(
            "WITH matching_events AS (
                 SELECT DISTINCT ce.snapshot_id
                 FROM capture_events ce
                 WHERE {}
             )",
            event_filter_clause("ce")
        )
    } else {
        String::new()
    }
}

fn matching_events_join(include_matching_events: bool) -> &'static str {
    if include_matching_events {
        "JOIN matching_events me ON me.snapshot_id = s.id"
    } else {
        ""
    }
}

fn has_temporal_event_filters(filters: &RetrievalFilters) -> bool {
    filters.since().is_some() || filters.until().is_some() || filters.hours().is_some()
}

fn requires_matching_events(filters: &RetrievalFilters) -> bool {
    has_temporal_event_filters(filters)
}

fn can_use_snapshot_stats_since_filter(filters: &RetrievalFilters) -> bool {
    (filters.since().is_some() || filters.hours().is_some())
        && filters.until().is_none()
        && filters.app().is_none()
        && filters.bundle_id().is_none()
}

fn can_use_snapshot_stats_for_stats(filters: &RetrievalFilters) -> bool {
    !has_temporal_event_filters(filters) && filters.app().is_none() && filters.bundle_id().is_none()
}

fn can_use_snapshot_event_cache(filters: &RetrievalFilters) -> bool {
    !has_temporal_event_filters(filters)
        && (filters.app().is_some() || filters.bundle_id().is_some())
}

fn event_filter_clause(alias: &str) -> String {
    format!(
        "(:since IS NULL OR datetime({alias}.observed_at) >= datetime(:since))
         AND (:until IS NULL OR datetime({alias}.observed_at) <= datetime(:until))
         AND (:app_like IS NULL OR ({alias}.frontmost_app_name IS NOT NULL AND lower({alias}.frontmost_app_name) LIKE :app_like ESCAPE '\\'))
         AND (:bundle_id IS NULL OR ({alias}.frontmost_app_bundle_id IS NOT NULL AND lower({alias}.frontmost_app_bundle_id) = :bundle_id))"
    )
}

fn event_filter_bind_clause() -> &'static str {
    "(:since IS NULL OR 1)
     AND (:until IS NULL OR 1)
     AND (:app_like IS NULL OR 1)
     AND (:bundle_id IS NULL OR 1)"
}

fn snapshot_event_filter_clause(cache_alias: &str) -> String {
    format!(
        "(:since IS NULL OR 1)
         AND (:until IS NULL OR 1)
         AND (:app_like IS NULL OR ({cache_alias}.app_names_lower != '' AND {cache_alias}.app_names_lower LIKE :app_like ESCAPE '\\'))
         AND (:bundle_id IS NULL OR instr(char(31) || {cache_alias}.bundle_ids_lower || char(31), char(31) || :bundle_id || char(31)) > 0)"
    )
}

fn base_event_filter_clause(
    cache_alias: &str,
    include_matching_events: bool,
    use_snapshot_event_cache: bool,
) -> String {
    if include_matching_events {
        event_filter_bind_clause().to_string()
    } else if use_snapshot_event_cache {
        snapshot_event_filter_clause(cache_alias)
    } else {
        event_filter_bind_clause().to_string()
    }
}

fn snapshot_stats_since_filter_clause(use_snapshot_stats_since_filter: bool) -> &'static str {
    if use_snapshot_stats_since_filter {
        "ss.last_observed_at >= datetime(:since)
         AND datetime(ss.last_observed_at) >= datetime(:since)"
    } else {
        "(:since IS NULL OR 1)"
    }
}

fn event_filter_where_clause(
    snapshot_id_expr: &str,
    cache_alias: &str,
    use_snapshot_event_cache: bool,
    has_temporal_event_filters: bool,
) -> String {
    if use_snapshot_event_cache {
        snapshot_event_filter_clause(cache_alias)
    } else if has_temporal_event_filters {
        format!(
            "EXISTS (
                 SELECT 1
                 FROM capture_events ce
                 WHERE ce.snapshot_id = {snapshot_id_expr}
                   AND {}
             )",
            event_filter_clause("ce")
        )
    } else {
        event_filter_bind_clause().to_string()
    }
}

fn snapshot_filter_clause(snapshot_alias: &str, snapshot_id_expr: &str) -> String {
    format!(
        "(:min_bytes IS NULL OR {snapshot_alias}.total_bytes >= :min_bytes)
         AND (:max_bytes IS NULL OR {snapshot_alias}.total_bytes <= :max_bytes)
         AND (
             :kind IS NULL
             OR (:kind = 'text' AND EXISTS (
                 SELECT 1 FROM item_representations ir
                 WHERE ir.snapshot_id = {snapshot_id_expr}
                   AND ir.kind IN ('plain_text', 'html', 'json', 'xml', 'rtf')
             ))
             OR (:kind = 'html' AND EXISTS (
                 SELECT 1 FROM item_representations ir
                 WHERE ir.snapshot_id = {snapshot_id_expr}
                   AND ir.kind = 'html'
             ))
             OR (:kind = 'rtf' AND EXISTS (
                 SELECT 1 FROM item_representations ir
                 WHERE ir.snapshot_id = {snapshot_id_expr}
                   AND ir.kind = 'rtf'
             ))
             OR (:kind = 'url' AND EXISTS (
                 SELECT 1 FROM item_representations ir
                 WHERE ir.snapshot_id = {snapshot_id_expr}
                   AND ir.kind = 'url'
             ))
             OR (:kind = 'file' AND EXISTS (
                 SELECT 1 FROM item_representations ir
                 WHERE ir.snapshot_id = {snapshot_id_expr}
                   AND ir.kind = 'file_url'
             ))
             OR (:kind = 'image' AND EXISTS (
                 SELECT 1 FROM item_representations ir
                 WHERE ir.snapshot_id = {snapshot_id_expr}
                   AND ir.kind = 'image'
             ))
             OR (:kind = 'pdf' AND EXISTS (
                 SELECT 1 FROM item_representations ir
                 WHERE ir.snapshot_id = {snapshot_id_expr}
                   AND ir.kind = 'pdf'
             ))
             OR (:kind = 'binary' AND EXISTS (
                 SELECT 1 FROM item_representations ir
                 WHERE ir.snapshot_id = {snapshot_id_expr}
                   AND ir.kind = 'binary'
             ))
             OR (:kind = 'other' AND {snapshot_alias}.snapshot_kind IN ('mixed', 'empty'))
         )
         AND (:has_text = 0 OR (
             ({snapshot_alias}.preview_text IS NOT NULL AND {snapshot_alias}.preview_text != '')
             OR EXISTS (
                 SELECT 1 FROM item_representations ir
                 WHERE ir.snapshot_id = {snapshot_id_expr}
                   AND ir.kind IN ('plain_text', 'url', 'file_url', 'html', 'json', 'xml', 'rtf')
                   AND ir.text_value IS NOT NULL AND ir.text_value != ''
             )
             OR EXISTS (
                 SELECT 1 FROM snapshot_ocr_cache soc
                 WHERE soc.snapshot_id = {snapshot_id_expr}
                   AND soc.ocr_text != ''
             )
         ))
         AND (:has_url = 0 OR EXISTS (
             SELECT 1 FROM item_representations ir
             WHERE ir.snapshot_id = {snapshot_id_expr}
               AND ir.kind = 'url'
         ))
         AND (:has_file_url = 0 OR EXISTS (
             SELECT 1 FROM item_representations ir
             WHERE ir.snapshot_id = {snapshot_id_expr}
               AND ir.kind = 'file_url'
         ))
         AND (:has_image = 0 OR EXISTS (
             SELECT 1 FROM item_representations ir
             WHERE ir.snapshot_id = {snapshot_id_expr}
               AND ir.kind = 'image'
         ))
         AND (:has_pdf = 0 OR EXISTS (
             SELECT 1 FROM item_representations ir
             WHERE ir.snapshot_id = {snapshot_id_expr}
               AND ir.kind = 'pdf'
         ))"
    )
}

fn app_like_pattern(filters: &RetrievalFilters) -> Option<String> {
    filters
        .app()
        .map(|value| format!("%{}%", escape_like_pattern(&value.to_ascii_lowercase())))
}

fn is_simple_fts_query(analysis: &QueryAnalysis) -> bool {
    analysis.exact_phrase.is_none()
        && !analysis.literal_preferred
        && !analysis.trimmed.is_empty()
        && analysis
            .trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric())
}

fn effective_since_param(filters: &RetrievalFilters) -> Result<Option<String>> {
    if let Some(since) = filters.since() {
        return Ok(Some(since.to_string()));
    }

    let Some(hours) = filters.hours() else {
        return Ok(None);
    };
    let since = (time::OffsetDateTime::now_utc() - time::Duration::hours(i64::from(hours)))
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|error| anyhow::anyhow!("format filter time: {error}"))?;
    Ok(Some(since))
}

fn map_search_hit_row(row: &Row<'_>, has_score: bool) -> rusqlite::Result<SearchHit> {
    let urls = split_aggregated_values(&row.get::<_, String>(13)?);
    let file_paths = split_aggregated_values(&row.get::<_, String>(14)?)
        .into_iter()
        .map(|value| normalise_file_path(&value))
        .collect::<Vec<_>>();
    let matched_fields = split_match_fields(&row.get::<_, String>(7)?);

    Ok(SearchHit::new(
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row_enum(row, 3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        matched_fields,
        row_usize(row, 8)?,
        row.get(9)?,
        row.get(10)?,
        row.get(11)?,
        row.get(12)?,
        urls,
        file_paths,
        row_usize(row, 15)?,
        row_usize(row, 16)?,
        if has_score { row.get(17)? } else { None },
    ))
}

fn map_scored_search_hit_row(row: &Row<'_>) -> rusqlite::Result<SearchHit> {
    map_search_hit_row(row, true)
}

fn map_timeline_event_row(row: &Row<'_>) -> rusqlite::Result<TimelineEvent> {
    let urls = split_aggregated_values(&row.get::<_, String>(10)?);
    let file_paths = split_aggregated_values(&row.get::<_, String>(11)?)
        .into_iter()
        .map(|value| normalise_file_path(&value))
        .collect::<Vec<_>>();

    Ok(TimelineEvent::new(
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row_enum(row, 5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        urls,
        file_paths,
        row_usize(row, 12)?,
        row_usize(row, 13)?,
    ))
}

fn paginate_rows(mut rows: Vec<SearchHit>, limit: usize) -> Page<SearchHit> {
    let has_more = rows.len() > limit;
    if has_more {
        rows.truncate(limit);
    }

    Page::new(rows, has_more)
}

fn merge_scored_search_results(
    mode: SearchMode,
    native: SearchResults,
    ocr: SearchResults,
    limit: usize,
    lower_score_is_better: bool,
) -> SearchResults {
    let mut rows = native.hits().to_vec();
    let mut seen = rows
        .iter()
        .map(SearchHit::snapshot_id)
        .collect::<std::collections::HashSet<_>>();
    for hit in ocr.hits() {
        if seen.insert(hit.snapshot_id()) {
            rows.push(hit.clone());
        }
    }

    rows.sort_by(|left, right| {
        let left_score = left.score().unwrap_or(if lower_score_is_better {
            f64::INFINITY
        } else {
            f64::NEG_INFINITY
        });
        let right_score = right.score().unwrap_or(if lower_score_is_better {
            f64::INFINITY
        } else {
            f64::NEG_INFINITY
        });
        let score_order = if lower_score_is_better {
            left_score
                .partial_cmp(&right_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        } else {
            right_score
                .partial_cmp(&left_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        };
        score_order
            .then_with(|| right.last_observed_at().cmp(left.last_observed_at()))
            .then_with(|| right.snapshot_id().cmp(&left.snapshot_id()))
    });

    let has_more = native.has_more() || ocr.has_more() || rows.len() > limit;
    if rows.len() > limit {
        rows.truncate(limit);
    }
    SearchResults::new(mode, rows, has_more)
}

fn paginate_timeline_rows(mut rows: Vec<TimelineEvent>, limit: usize) -> Page<TimelineEvent> {
    let has_more = rows.len() > limit;
    if has_more {
        rows.truncate(limit);
    }

    Page::new(rows, has_more)
}

fn split_aggregated_values(value: &str) -> Vec<String> {
    if value.is_empty() {
        Vec::new()
    } else {
        value
            .split(LIST_VALUE_SEPARATOR)
            .filter(|entry| !entry.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }
}

fn split_match_fields(value: &str) -> Vec<String> {
    if value.is_empty() {
        Vec::new()
    } else {
        value
            .split(MATCHED_FIELDS_SEPARATOR)
            .filter(|entry| !entry.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }
}

fn normalise_file_path(value: &str) -> String {
    decode_file_url_path(value).unwrap_or_else(|| value.to_string())
}

fn decode_file_url_path(value: &str) -> Option<String> {
    let rest = value.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let decoded = percent_decode(rest);

    if decoded.is_empty() {
        None
    } else {
        Some(decoded)
    }
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(first), Some(second)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                out.push((first << 4) | second);
                index += 3;
                continue;
            }
        }

        out.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn analyze_query(query: &str) -> QueryAnalysis {
    let trimmed = query.trim().to_string();
    let lower = trimmed.to_ascii_lowercase();
    let exact_phrase = trimmed
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let is_url_like = lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("file://");
    let is_path_like = trimmed.starts_with("~/")
        || trimmed.starts_with('/')
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
        || trimmed.contains('\\');
    let is_bundle_id_like = lower.contains('.')
        && !lower.contains(' ')
        && !is_url_like
        && !lower.contains('/')
        && !lower.contains('\\')
        && !lower.starts_with("~/");
    let punctuation_heavy =
        trimmed.contains(':') || trimmed.contains('/') || trimmed.contains('\\');
    let shell_fragment = trimmed.contains(" -")
        || trimmed.contains(" --")
        || trimmed.contains(" | ")
        || trimmed.contains(" && ")
        || trimmed.contains('=')
        || trimmed.contains('$');
    let literal_preferred = trimmed.contains('%')
        || trimmed.contains('_')
        || trimmed.contains('\\')
        || is_url_like
        || is_path_like
        || is_bundle_id_like
        || punctuation_heavy
        || shell_fragment;
    let path_fragment = if let Some(value) = trimmed.strip_prefix("~/") {
        Some(value.trim_start_matches('/').to_ascii_lowercase())
    } else if lower.starts_with("file://") {
        Some(normalise_file_path(&trimmed).to_ascii_lowercase())
    } else if trimmed.starts_with('/') || trimmed.starts_with("./") || trimmed.starts_with("../") {
        Some(lower.clone())
    } else if trimmed.contains('\\')
        && (trimmed.starts_with(".\\")
            || trimmed.starts_with("..\\")
            || trimmed.starts_with('\\')
            || trimmed.contains(":\\"))
    {
        Some(lower.clone())
    } else {
        None
    }
    .filter(|value| !value.is_empty());

    QueryAnalysis {
        trimmed,
        lower,
        exact_phrase,
        literal_preferred,
        path_fragment,
    }
}

fn is_invalid_fts_query(error: &SqlError) -> bool {
    let sqlite_unknown_error = matches!(error.sqlite_error_code(), Some(ErrorCode::Unknown));

    match error {
        SqlError::SqlInputError { msg, .. } => invalid_fts_message(msg),
        SqlError::SqliteFailure(_, Some(msg)) if sqlite_unknown_error => invalid_fts_message(msg),
        _ => false,
    }
}

fn invalid_fts_message(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("fts5: syntax error")
        || message.contains("malformed match expression")
        || message.contains("unterminated string")
        || message.contains("no such column:")
}

fn escape_like_pattern(query: &str) -> String {
    query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn literal_fts_match_query(analysis: &QueryAnalysis) -> Option<String> {
    let candidate = analysis.path_fragment.as_deref().unwrap_or(&analysis.lower);

    if candidate.chars().count() < 3 {
        return None;
    }

    if analysis
        .trimmed
        .chars()
        .any(|ch| matches!(ch, '%' | '_' | '\\'))
    {
        return None;
    }

    Some(format!("\"{}\"", candidate.replace('"', "\"\"")))
}

fn literal_token_match_query(analysis: &QueryAnalysis) -> Option<String> {
    let is_url_like =
        analysis.lower.starts_with("http://") || analysis.lower.starts_with("https://");
    let is_bundle_id_like = analysis.lower.contains('.')
        && !analysis.lower.contains(' ')
        && !is_url_like
        && !analysis.lower.contains('/')
        && !analysis.lower.contains('\\')
        && !analysis.lower.starts_with("~/");
    if is_url_like || is_bundle_id_like {
        return None;
    }

    if analysis.trimmed.contains('%')
        || analysis.trimmed.contains('_')
        || analysis.trimmed.contains('\\')
    {
        return None;
    }

    let tokens = analysis
        .lower
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| {
            !token.is_empty() && (token.len() >= 2 || token.chars().all(|ch| ch.is_ascii_digit()))
        })
        .map(|token| format!("\"{token}\""))
        .collect::<Vec<_>>();

    if tokens.len() < 2 {
        None
    } else {
        Some(tokens.join(" AND "))
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::db::{Database, RetrievalFilters, SearchMode, SearchResults};
    use crate::model::{build_item, build_representation, build_snapshot, CaptureContext};

    use super::{
        analyze_query, fts_query, invalid_fts_message, is_simple_fts_query,
        literal_fts_match_query, literal_token_match_query, normalise_file_path,
    };

    fn fake_snapshot(change_count: i64, text: &str) -> crate::model::ClipboardSnapshot {
        build_snapshot(
            CaptureContext::new(change_count)
                .with_frontmost_app_name("Terminal")
                .with_frontmost_app_bundle_id("com.apple.Terminal"),
            vec![build_item(
                0,
                vec![build_representation(
                    "public.utf8-plain-text".to_string(),
                    None,
                    text.as_bytes().to_vec(),
                )],
            )],
        )
    }

    fn unfiltered() -> RetrievalFilters {
        RetrievalFilters::default()
    }

    #[test]
    fn like_special_characters_skip_fts_mode() {
        assert!(analyze_query("50%").literal_preferred);
        assert!(analyze_query("config_test").literal_preferred);
        assert!(analyze_query(r"logs\2024").literal_preferred);
        assert!(analyze_query("https://example.com").literal_preferred);
        assert!(analyze_query("com.apple.Terminal").literal_preferred);
        assert!(analyze_query("~/path/with/slashes").literal_preferred);
        assert!(analyze_query("foo:bar").literal_preferred);
        assert!(analyze_query("git commit -m").literal_preferred);
        assert!(!analyze_query("git clone").literal_preferred);
    }

    #[test]
    fn query_analysis_preserves_exact_phrase_without_forcing_literal() {
        let analysis = analyze_query("\"launchctl bootstrap\"");
        assert_eq!(
            analysis.exact_phrase.as_deref(),
            Some("launchctl bootstrap")
        );
        assert!(!analysis.literal_preferred);
    }

    #[test]
    fn simple_fts_query_skips_per_row_phrase_scoring() {
        assert!(is_simple_fts_query(&analyze_query("git")));
        assert!(!is_simple_fts_query(&analyze_query("\"git clone\"")));
        assert!(!is_simple_fts_query(&analyze_query("git clone")));
        assert!(!is_simple_fts_query(&analyze_query("com.apple.Terminal")));

        let simple_sql = fts_query(false, false, true);
        let phrase_sql = fts_query(false, false, false);

        assert!(!simple_sql.contains("Phrase match in best text"));
        assert!(!simple_sql.contains("THEN 0.08"));
        assert!(phrase_sql.contains("Phrase match in best text"));
        assert!(phrase_sql.contains("THEN 0.08"));
    }

    #[test]
    fn invalid_fts_messages_are_narrowly_classified() {
        assert!(invalid_fts_message("fts5: syntax error near \"%\""));
        assert!(invalid_fts_message("malformed MATCH expression"));
        assert!(invalid_fts_message("no such column: https"));
        assert!(!invalid_fts_message("database disk image is malformed"));
    }

    #[test]
    fn literal_fts_match_query_skips_like_escaped_inputs() {
        assert!(literal_fts_match_query(&analyze_query("50%")).is_none());
        assert!(literal_fts_match_query(&analyze_query("config_test")).is_none());
        assert!(literal_fts_match_query(&analyze_query(r"logs\2024")).is_none());
        assert_eq!(
            literal_fts_match_query(&analyze_query("/tmp/repo/42/Cargo.toml")).as_deref(),
            Some("\"/tmp/repo/42/cargo.toml\"")
        );
        assert_eq!(
            analyze_query("~/path/with/slashes")
                .path_fragment
                .as_deref(),
            Some("path/with/slashes")
        );
        assert_eq!(
            analyze_query("file:///tmp/repo/42/Cargo.toml")
                .path_fragment
                .as_deref(),
            Some("/tmp/repo/42/cargo.toml")
        );
        assert_eq!(
            literal_token_match_query(&analyze_query("/tmp/repo/42/Cargo.toml")).as_deref(),
            Some("\"tmp\" AND \"repo\" AND \"42\" AND \"cargo\" AND \"toml\"")
        );
    }

    #[test]
    fn file_urls_decode_to_paths() {
        assert_eq!(
            normalise_file_path("file:///Users/test/Documents/hello%20world.txt"),
            "/Users/test/Documents/hello world.txt"
        );
        assert_eq!(
            normalise_file_path("/Users/test/Documents/hello world.txt"),
            "/Users/test/Documents/hello world.txt"
        );
    }

    #[test]
    fn search_auto_uses_literal_matching_for_wildcard_queries() -> Result<()> {
        let mut db = Database::open_in_memory()?;

        db.store_capture(&fake_snapshot(1, "Discount: 50 percent off"))?;
        db.store_capture(&fake_snapshot(2, "Discount: 50%"))?;

        let results = db.search_auto("50%", 10, &unfiltered())?;
        assert_eq!(results.mode_used(), SearchMode::Literal);
        assert_eq!(results.hits().len(), 1);
        assert_eq!(results.hits()[0].preview_text(), "Discount: 50%");
        Ok(())
    }

    #[test]
    fn find_snapshot_hydrates_nested_items_and_recent_events() -> Result<()> {
        let mut db = Database::open_in_memory()?;
        let snapshot = build_snapshot(
            CaptureContext::new(1)
                .with_frontmost_app_name("Terminal")
                .with_frontmost_app_bundle_id("com.apple.Terminal"),
            vec![build_item(
                0,
                vec![build_representation(
                    "public.utf8-plain-text".to_string(),
                    None,
                    b"git status".to_vec(),
                )],
            )],
        );
        let stored = db.store_capture(&snapshot)?;
        db.store_capture(&snapshot)?;

        let details = db
            .find_snapshot(stored.snapshot_id(), 10)?
            .expect("snapshot exists");
        assert_eq!(details.items().len(), 1);
        assert_eq!(details.recent_events().len(), 2);
        Ok(())
    }

    #[test]
    fn empty_fts_results_fall_back_to_literal_matches() -> Result<()> {
        let mut db = Database::open_in_memory()?;
        db.store_capture(&fake_snapshot(1, "https://example.com/repo"))?;

        let results: SearchResults =
            db.search_auto("https://example.com/repo", 10, &unfiltered())?;
        assert_eq!(results.mode_used(), SearchMode::Literal);
        assert_eq!(results.hits().len(), 1);
        Ok(())
    }
}
