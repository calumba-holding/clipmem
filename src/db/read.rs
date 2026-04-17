use anyhow::{Context, Result};
use rusqlite::{named_params, params, Error as SqlError, ErrorCode, Row};

use crate::model::{
    CaptureEvent, ClipboardItem, ClipboardRepresentation, DoctorReport, FlattenedTextProjection,
    SearchHit, SnapshotDetails, TimelineEvent,
};

use super::{
    collect_rows, row_enum, row_usize, sanitise_limit, usize_to_i64, Database, Page,
    RecentCursorState, RetrievalFilters, SearchCursorState, SearchMode, SearchResults,
    TimelineCursorState, TimelineSort,
};

const LIST_VALUE_SEPARATOR: char = '\u{1f}';
const LIST_QUERY_CTES: &str = r"
    WITH event_summary AS (
        SELECT
            snapshot_id,
            COUNT(*) AS capture_count,
            MIN(observed_at) AS first_observed_at,
            MAX(observed_at) AS last_observed_at
        FROM capture_events
        GROUP BY snapshot_id
    ),
    latest_event AS (
        SELECT
            snapshot_id,
            id AS event_id,
            observed_at,
            frontmost_app_name,
            frontmost_app_bundle_id
        FROM (
            SELECT
                ce.*,
                ROW_NUMBER() OVER (
                    PARTITION BY ce.snapshot_id
                    ORDER BY ce.observed_at DESC, ce.id DESC
                ) AS row_number
            FROM capture_events ce
        )
        WHERE row_number = 1
    ),
    url_values AS (
        SELECT
            snapshot_id,
            GROUP_CONCAT(text_value, char(31)) AS urls
        FROM (
            SELECT DISTINCT snapshot_id, text_value
            FROM item_representations
            WHERE kind = 'url' AND text_value IS NOT NULL AND text_value != ''
        )
        GROUP BY snapshot_id
    ),
    file_url_values AS (
        SELECT
            snapshot_id,
            GROUP_CONCAT(text_value, char(31)) AS file_urls
        FROM (
            SELECT DISTINCT snapshot_id, text_value
            FROM item_representations
            WHERE kind = 'file_url' AND text_value IS NOT NULL AND text_value != ''
        )
        GROUP BY snapshot_id
    )
";

impl Database {
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

        if requires_literal_like_search(query) {
            return self.search_literal_page(query, limit, filters, cursor);
        }

        if let Some(cursor) = cursor {
            return match cursor.mode_used() {
                SearchMode::Fts => self.search_fts_page(query, limit, filters, Some(cursor)),
                SearchMode::Literal => self.search_literal_page(query, limit, filters, Some(cursor)),
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
        let sql = recent_query();
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
            "{LIST_QUERY_CTES}
             SELECT EXISTS(
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
        if self.load_snapshot_summary(snapshot_id)?.is_none() {
            return Ok(None);
        }

        let items = self.load_snapshot_items(snapshot_id)?;
        Ok(Some(FlattenedTextProjection::from_items(&items)))
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
        let sql = fts_query();
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

        collect_rows(rows)
            .map(|hits| paginate_rows(hits, limit))
            .map(|page| {
                let has_more = page.has_more();
                SearchResults::new(SearchMode::Fts, page.into_items(), has_more)
            })
            .context("collect FTS search rows")
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
        let like = format!("%{}%", escape_like_pattern(query));
        let sql = literal_query();
        let app_like = app_like_pattern(filters);
        let bundle_id = filters.bundle_id().map(|value| value.to_ascii_lowercase());
        let kind = filters.kind().map(|value| value.as_str());
        let since = effective_since_param(filters)?;

        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("prepare literal search query")?;
        let rows = stmt
            .query_map(
                named_params! {
                    ":like" : like,
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
                    ":cursor_last_seen_at" : cursor.map(SearchCursorState::last_seen_at),
                    ":cursor_snapshot_id" : cursor.map(SearchCursorState::snapshot_id),
                    ":limit" : fetch_limit,
                },
                |row| map_search_hit_row(row, false),
            )
            .context("execute literal search query")?;

        collect_rows(rows)
            .map(|hits| paginate_rows(hits, limit))
            .map(|page| {
                let has_more = page.has_more();
                SearchResults::new(SearchMode::Literal, page.into_items(), has_more)
            })
            .context("collect literal search rows")
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
                COUNT(e.id) AS capture_count,
                MIN(e.observed_at) AS first_observed_at,
                MAX(e.observed_at) AS last_observed_at,
                (
                    SELECT ce.frontmost_app_name
                    FROM capture_events ce
                    WHERE ce.snapshot_id = s.id
                    ORDER BY ce.observed_at DESC, ce.id DESC
                    LIMIT 1
                ) AS last_frontmost_app_name,
                (
                    SELECT ce.frontmost_app_bundle_id
                    FROM capture_events ce
                    WHERE ce.snapshot_id = s.id
                    ORDER BY ce.observed_at DESC, ce.id DESC
                    LIMIT 1
                ) AS last_frontmost_app_bundle_id
            FROM snapshots s
            LEFT JOIN capture_events e ON e.snapshot_id = s.id
            WHERE s.id = ?1
            GROUP BY s.id
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

fn recent_query() -> String {
    format!(
        "{LIST_QUERY_CTES}
         , matching_events AS (
             SELECT DISTINCT ce.snapshot_id
             FROM capture_events ce
             WHERE {event_filter_clause}
         )
         SELECT
             base.snapshot_id,
             base.event_id,
             base.sha256,
             base.snapshot_kind,
             base.preview_text,
             base.why_matched,
             base.capture_count,
             base.first_observed_at,
             base.last_observed_at,
             base.last_frontmost_app_name,
             base.last_frontmost_app_bundle_id,
             base.urls,
             base.file_urls,
             base.total_bytes,
             base.item_count
         FROM (
             SELECT
                 s.id AS snapshot_id,
                 le.event_id AS event_id,
                 s.sha256 AS sha256,
                 s.snapshot_kind AS snapshot_kind,
                 s.preview_text AS preview_text,
                 NULL AS why_matched,
                 es.capture_count AS capture_count,
                 es.first_observed_at AS first_observed_at,
                 es.last_observed_at AS last_observed_at,
                 le.frontmost_app_name AS last_frontmost_app_name,
                 le.frontmost_app_bundle_id AS last_frontmost_app_bundle_id,
                 COALESCE(uv.urls, '') AS urls,
                 COALESCE(fv.file_urls, '') AS file_urls,
                 s.total_bytes AS total_bytes,
                 s.item_count AS item_count
             FROM snapshots s
             JOIN event_summary es ON es.snapshot_id = s.id
             JOIN latest_event le ON le.snapshot_id = s.id
             JOIN matching_events me ON me.snapshot_id = s.id
             LEFT JOIN url_values uv ON uv.snapshot_id = s.id
             LEFT JOIN file_url_values fv ON fv.snapshot_id = s.id
             WHERE {snapshot_filter_clause}
         ) base
         WHERE (
             :cursor_last_seen_at IS NULL
             OR base.last_observed_at < :cursor_last_seen_at
             OR (base.last_observed_at = :cursor_last_seen_at AND base.snapshot_id < :cursor_snapshot_id)
         )
         ORDER BY base.last_observed_at DESC, base.snapshot_id DESC
         LIMIT :limit",
        event_filter_clause = event_filter_clause("ce"),
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
         WITH url_values AS (
             SELECT
                 snapshot_id,
                 GROUP_CONCAT(text_value, char(31)) AS urls
             FROM (
                 SELECT DISTINCT snapshot_id, text_value
                 FROM item_representations
                 WHERE kind = 'url' AND text_value IS NOT NULL AND text_value != ''
             )
             GROUP BY snapshot_id
         ),
         file_url_values AS (
             SELECT
                 snapshot_id,
                 GROUP_CONCAT(text_value, char(31)) AS file_urls
             FROM (
                 SELECT DISTINCT snapshot_id, text_value
                 FROM item_representations
                 WHERE kind = 'file_url' AND text_value IS NOT NULL AND text_value != ''
             )
             GROUP BY snapshot_id
         )
         SELECT
             ce.id AS event_id,
             ce.snapshot_id AS snapshot_id,
             ce.observed_at AS observed_at,
             ce.change_count AS change_count,
             s.sha256 AS sha256,
             s.snapshot_kind AS snapshot_kind,
             COALESCE(NULLIF(s.preview_text, ''), s.search_text, '') AS best_text,
             s.preview_text AS preview_text,
             ce.frontmost_app_name AS frontmost_app_name,
             ce.frontmost_app_bundle_id AS frontmost_app_bundle_id,
             COALESCE(uv.urls, '') AS urls,
             COALESCE(fv.file_urls, '') AS file_urls,
             s.total_bytes AS total_bytes,
             s.item_count AS item_count
         FROM capture_events ce
         JOIN snapshots s ON s.id = ce.snapshot_id
         LEFT JOIN url_values uv ON uv.snapshot_id = ce.snapshot_id
         LEFT JOIN file_url_values fv ON fv.snapshot_id = ce.snapshot_id
         WHERE {event_filter_clause}
           AND {snapshot_filter_clause}
           AND {cursor_predicate}
         ORDER BY {ordering}
         LIMIT :limit",
        event_filter_clause = event_filter_clause("ce"),
        snapshot_filter_clause = snapshot_filter_clause("s", "ce.snapshot_id"),
    )
}

fn literal_query() -> String {
    format!(
        "{LIST_QUERY_CTES}
         , matching_events AS (
             SELECT DISTINCT ce.snapshot_id
             FROM capture_events ce
             WHERE {event_filter_clause}
         )
         SELECT
             base.snapshot_id,
             base.event_id,
             base.sha256,
             base.snapshot_kind,
             base.preview_text,
             base.why_matched,
             base.capture_count,
             base.first_observed_at,
             base.last_observed_at,
             base.last_frontmost_app_name,
             base.last_frontmost_app_bundle_id,
             base.urls,
             base.file_urls,
             base.total_bytes,
             base.item_count
         FROM (
             SELECT
                 s.id AS snapshot_id,
                 le.event_id AS event_id,
                 s.sha256 AS sha256,
                 s.snapshot_kind AS snapshot_kind,
                 s.preview_text AS preview_text,
                 s.preview_text AS why_matched,
                 es.capture_count AS capture_count,
                 es.first_observed_at AS first_observed_at,
                 es.last_observed_at AS last_observed_at,
                 le.frontmost_app_name AS last_frontmost_app_name,
                 le.frontmost_app_bundle_id AS last_frontmost_app_bundle_id,
                 COALESCE(uv.urls, '') AS urls,
                 COALESCE(fv.file_urls, '') AS file_urls,
                 s.total_bytes AS total_bytes,
                 s.item_count AS item_count
             FROM snapshots s
             JOIN event_summary es ON es.snapshot_id = s.id
             JOIN latest_event le ON le.snapshot_id = s.id
             JOIN matching_events me ON me.snapshot_id = s.id
             LEFT JOIN url_values uv ON uv.snapshot_id = s.id
             LEFT JOIN file_url_values fv ON fv.snapshot_id = s.id
             WHERE (s.search_text LIKE :like ESCAPE '\\'
                OR s.preview_text LIKE :like ESCAPE '\\')
               AND {snapshot_filter_clause}
         ) base
         WHERE (
             :cursor_last_seen_at IS NULL
             OR base.last_observed_at < :cursor_last_seen_at
             OR (base.last_observed_at = :cursor_last_seen_at AND base.snapshot_id < :cursor_snapshot_id)
         )
         ORDER BY base.last_observed_at DESC, base.snapshot_id DESC
         LIMIT :limit",
        event_filter_clause = event_filter_clause("ce"),
        snapshot_filter_clause = snapshot_filter_clause("s", "s.id"),
    )
}

fn fts_query() -> String {
    format!(
        "{LIST_QUERY_CTES}
         , matching_events AS (
             SELECT DISTINCT ce.snapshot_id
             FROM capture_events ce
             WHERE {event_filter_clause}
         )
         , search_rows AS (
             SELECT
                 s.id AS snapshot_id,
                 le.event_id AS event_id,
                 s.sha256 AS sha256,
                 s.snapshot_kind AS snapshot_kind,
                 s.preview_text AS preview_text,
                 COALESCE(snippet(snapshots_fts, 0, '⟦', '⟧', ' … ', 24), s.preview_text) AS why_matched,
                 es.capture_count AS capture_count,
                 es.first_observed_at AS first_observed_at,
                 es.last_observed_at AS last_observed_at,
                 le.frontmost_app_name AS last_frontmost_app_name,
                 le.frontmost_app_bundle_id AS last_frontmost_app_bundle_id,
                 COALESCE(uv.urls, '') AS urls,
                 COALESCE(fv.file_urls, '') AS file_urls,
                 s.total_bytes AS total_bytes,
                 s.item_count AS item_count,
                 bm25(snapshots_fts) AS score
             FROM snapshots_fts
             JOIN snapshots s ON s.id = snapshots_fts.rowid
             JOIN event_summary es ON es.snapshot_id = s.id
             JOIN latest_event le ON le.snapshot_id = s.id
             JOIN matching_events me ON me.snapshot_id = s.id
             LEFT JOIN url_values uv ON uv.snapshot_id = s.id
             LEFT JOIN file_url_values fv ON fv.snapshot_id = s.id
             WHERE snapshots_fts MATCH :query
               AND {snapshot_filter_clause}
         )
         SELECT
             search_rows.snapshot_id,
             search_rows.event_id,
             search_rows.sha256,
             search_rows.snapshot_kind,
             search_rows.preview_text,
             search_rows.why_matched,
             search_rows.capture_count,
             search_rows.first_observed_at,
             search_rows.last_observed_at,
             search_rows.last_frontmost_app_name,
             search_rows.last_frontmost_app_bundle_id,
             search_rows.urls,
             search_rows.file_urls,
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
         LIMIT :limit",
        event_filter_clause = event_filter_clause("ce"),
        snapshot_filter_clause = snapshot_filter_clause("s", "s.id"),
    )
}

fn event_filter_clause(alias: &str) -> String {
    format!(
        "(:since IS NULL OR datetime({alias}.observed_at) >= datetime(:since))
         AND (:until IS NULL OR datetime({alias}.observed_at) <= datetime(:until))
         AND (:app_like IS NULL OR ({alias}.frontmost_app_name IS NOT NULL AND lower({alias}.frontmost_app_name) LIKE :app_like ESCAPE '\\'))
         AND (:bundle_id IS NULL OR ({alias}.frontmost_app_bundle_id IS NOT NULL AND lower({alias}.frontmost_app_bundle_id) = :bundle_id))"
    )
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
    let urls = split_aggregated_values(&row.get::<_, String>(11)?);
    let file_paths = split_aggregated_values(&row.get::<_, String>(12)?)
        .into_iter()
        .map(|value| normalise_file_path(&value))
        .collect::<Vec<_>>();

    Ok(SearchHit::new(
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row_enum(row, 3)?,
        row.get(4)?,
        row.get(5)?,
        row_usize(row, 6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
        urls,
        file_paths,
        row_usize(row, 13)?,
        row_usize(row, 14)?,
        if has_score { row.get(15)? } else { None },
    ))
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

fn requires_literal_like_search(query: &str) -> bool {
    query.contains('%') || query.contains('_') || query.contains('\\')
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::db::{Database, RetrievalFilters, SearchMode, SearchResults};
    use crate::model::{build_item, build_representation, build_snapshot, CaptureContext};

    use super::{invalid_fts_message, normalise_file_path, requires_literal_like_search};

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
        assert!(requires_literal_like_search("50%"));
        assert!(requires_literal_like_search("config_test"));
        assert!(requires_literal_like_search(r"logs\2024"));
        assert!(!requires_literal_like_search("git clone"));
    }

    #[test]
    fn invalid_fts_messages_are_narrowly_classified() {
        assert!(invalid_fts_message("fts5: syntax error near \"%\""));
        assert!(invalid_fts_message("malformed MATCH expression"));
        assert!(invalid_fts_message("no such column: https"));
        assert!(!invalid_fts_message("database disk image is malformed"));
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

        let details = db.find_snapshot(stored.snapshot_id(), 10)?.expect("snapshot exists");
        assert_eq!(details.items().len(), 1);
        assert_eq!(details.recent_events().len(), 2);
        Ok(())
    }

    #[test]
    fn empty_fts_results_fall_back_to_literal_matches() -> Result<()> {
        let mut db = Database::open_in_memory()?;
        db.store_capture(&fake_snapshot(1, "https://example.com/repo"))?;

        let results: SearchResults = db.search_auto("https://example.com/repo", 10, &unfiltered())?;
        assert_eq!(results.mode_used(), SearchMode::Literal);
        assert_eq!(results.hits().len(), 1);
        Ok(())
    }
}
