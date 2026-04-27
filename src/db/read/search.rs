use anyhow::{Context, Result};
use rusqlite::{named_params, params, Error as SqlError, Result as SqlResult};

use crate::db::core::sanitise_limit;
use crate::db::read::filter_sql::{
    app_like_pattern, can_use_snapshot_event_cache, effective_since_param, escape_like_pattern,
    has_temporal_event_filters, requires_matching_events,
};
use crate::db::read::queries::{
    file_path_literal_query, fts_query, literal_query, ocr_fts_query, ocr_literal_query,
};
use crate::db::read::query_analysis::{
    analyze_query, is_invalid_fts_query, is_simple_fts_query, literal_fts_match_query,
    QueryAnalysis,
};
use crate::db::read::row_mapping::{
    map_scored_search_hit_row, map_search_hit_row, merge_scored_search_results, paginate_rows,
};
use crate::db::sqlite_helpers::{collect_rows, usize_to_i64};
use crate::db::types::{Database, RetrievalFilters, SearchCursorState, SearchMode, SearchResults};
use crate::model::SearchHit;

pub(in crate::db) const FAST_FILE_PATH_LITERAL_QUERY: &str = r"
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

struct SearchExecutionParams<'a> {
    fetch_limit: i64,
    app_like: Option<String>,
    bundle_id: Option<String>,
    kind: Option<&'a str>,
    since: Option<String>,
    until: Option<&'a str>,
    has_text: bool,
    has_url: bool,
    has_file_url: bool,
    has_image: bool,
    has_pdf: bool,
    min_bytes: Option<i64>,
    max_bytes: Option<i64>,
    cursor_score: Option<f64>,
    cursor_last_seen_at: Option<&'a str>,
    cursor_snapshot_id: Option<i64>,
}

impl<'a> SearchExecutionParams<'a> {
    fn new(
        limit: usize,
        filters: &'a RetrievalFilters,
        cursor: Option<&'a SearchCursorState>,
    ) -> Result<Self> {
        Ok(Self {
            fetch_limit: usize_to_i64(limit.saturating_add(1))?,
            app_like: app_like_pattern(filters),
            bundle_id: filters.bundle_id().map(|value| value.to_ascii_lowercase()),
            kind: filters.kind().map(|value| value.as_str()),
            since: effective_since_param(filters)?,
            until: filters.until(),
            has_text: filters.has_text(),
            has_url: filters.has_url(),
            has_file_url: filters.has_file_url(),
            has_image: filters.has_image(),
            has_pdf: filters.has_pdf(),
            min_bytes: filters.min_bytes().map(usize_to_i64).transpose()?,
            max_bytes: filters.max_bytes().map(usize_to_i64).transpose()?,
            cursor_score: cursor.and_then(SearchCursorState::score),
            cursor_last_seen_at: cursor.map(SearchCursorState::last_seen_at),
            cursor_snapshot_id: cursor.map(SearchCursorState::snapshot_id),
        })
    }
}

fn collect_search_results<I>(
    rows: I,
    limit: usize,
    mode: SearchMode,
    context: &'static str,
) -> Result<SearchResults>
where
    I: Iterator<Item = SqlResult<SearchHit>>,
{
    collect_rows(rows)
        .map(|hits| paginate_rows(hits, limit))
        .map(|page| {
            let has_more = page.has_more();
            SearchResults::new(mode, page.into_items(), has_more)
        })
        .with_context(|| format!("collect {context} search rows"))
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
        let params = SearchExecutionParams::new(limit, filters, cursor)?;
        let analysis = analyze_query(query);
        let use_snapshot_event_cache = can_use_snapshot_event_cache(filters);
        let has_temporal_event_filters = has_temporal_event_filters(filters);
        let sql = fts_query(
            use_snapshot_event_cache,
            has_temporal_event_filters,
            is_simple_fts_query(&analysis),
        );

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
                    ":since" : params.since.as_deref(),
                    ":until" : params.until,
                    ":app_like" : params.app_like.as_deref(),
                    ":bundle_id" : params.bundle_id.as_deref(),
                    ":kind" : params.kind,
                    ":has_text" : params.has_text,
                    ":has_url" : params.has_url,
                    ":has_file_url" : params.has_file_url,
                    ":has_image" : params.has_image,
                    ":has_pdf" : params.has_pdf,
                    ":min_bytes" : params.min_bytes,
                    ":max_bytes" : params.max_bytes,
                    ":cursor_score" : params.cursor_score,
                    ":cursor_last_seen_at" : params.cursor_last_seen_at,
                    ":cursor_snapshot_id" : params.cursor_snapshot_id,
                    ":limit" : params.fetch_limit,
                },
                |row| map_search_hit_row(row, true),
            )
            .context("execute FTS search query")?;

        let native_hits = collect_search_results(rows, limit, SearchMode::Fts, "FTS")?;
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
        let params = SearchExecutionParams::new(limit, filters, cursor)?;
        let analysis = analyze_query(query);
        if let Some(path_fragment) = analysis.path_fragment.as_deref() {
            let results =
                self.search_file_path_literal_page(path_fragment, limit, filters, cursor)?;
            if cursor.is_some() || !results.hits().is_empty() {
                return Ok(results);
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

        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("prepare literal search query")?;
        let prefix_like = format!("{}%", escape_like_pattern(&analysis.lower));
        let path_fragment_like = analysis
            .path_fragment
            .as_ref()
            .map(|value| format!("%{}%", escape_like_pattern(value)));
        let rows = if let Some(literal_match) = literal_match.as_deref() {
            stmt.query_map(
                named_params! {
                    ":query_lower" : analysis.lower.as_str(),
                    ":like" : like,
                    ":prefix_like" : prefix_like.as_str(),
                    ":literal_match" : literal_match,
                    ":path_fragment_like" : path_fragment_like.as_deref(),
                    ":exact_phrase_lower" : analysis.exact_phrase.as_deref(),
                    ":since" : params.since.as_deref(),
                    ":until" : params.until,
                    ":app_like" : params.app_like.as_deref(),
                    ":bundle_id" : params.bundle_id.as_deref(),
                    ":kind" : params.kind,
                    ":has_text" : params.has_text,
                    ":has_url" : params.has_url,
                    ":has_file_url" : params.has_file_url,
                    ":has_image" : params.has_image,
                    ":has_pdf" : params.has_pdf,
                    ":min_bytes" : params.min_bytes,
                    ":max_bytes" : params.max_bytes,
                    ":cursor_score" : params.cursor_score,
                    ":cursor_last_seen_at" : params.cursor_last_seen_at,
                    ":cursor_snapshot_id" : params.cursor_snapshot_id,
                    ":limit" : params.fetch_limit,
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
                    ":since" : params.since.as_deref(),
                    ":until" : params.until,
                    ":app_like" : params.app_like.as_deref(),
                    ":bundle_id" : params.bundle_id.as_deref(),
                    ":kind" : params.kind,
                    ":has_text" : params.has_text,
                    ":has_url" : params.has_url,
                    ":has_file_url" : params.has_file_url,
                    ":has_image" : params.has_image,
                    ":has_pdf" : params.has_pdf,
                    ":min_bytes" : params.min_bytes,
                    ":max_bytes" : params.max_bytes,
                    ":cursor_score" : params.cursor_score,
                    ":cursor_last_seen_at" : params.cursor_last_seen_at,
                    ":cursor_snapshot_id" : params.cursor_snapshot_id,
                    ":limit" : params.fetch_limit,
                },
                map_scored_search_hit_row,
            )
        }
        .context("execute literal search query")?;

        let native_hits = collect_search_results(rows, limit, SearchMode::Literal, "literal")?;
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
        let params = SearchExecutionParams::new(limit, filters, cursor)?;
        let use_snapshot_event_cache = can_use_snapshot_event_cache(filters);
        let has_temporal_event_filters = has_temporal_event_filters(filters);
        let sql = ocr_fts_query(use_snapshot_event_cache, has_temporal_event_filters);

        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("prepare OCR FTS search query")?;
        let rows = stmt
            .query_map(
                named_params! {
                    ":query" : query,
                    ":since" : params.since.as_deref(),
                    ":until" : params.until,
                    ":app_like" : params.app_like.as_deref(),
                    ":bundle_id" : params.bundle_id.as_deref(),
                    ":kind" : params.kind,
                    ":has_text" : params.has_text,
                    ":has_url" : params.has_url,
                    ":has_file_url" : params.has_file_url,
                    ":has_image" : params.has_image,
                    ":has_pdf" : params.has_pdf,
                    ":min_bytes" : params.min_bytes,
                    ":max_bytes" : params.max_bytes,
                    ":cursor_score" : params.cursor_score,
                    ":cursor_last_seen_at" : params.cursor_last_seen_at,
                    ":cursor_snapshot_id" : params.cursor_snapshot_id,
                    ":limit" : params.fetch_limit,
                },
                |row| map_search_hit_row(row, true),
            )
            .context("execute OCR FTS search query")?;

        collect_search_results(rows, limit, SearchMode::Fts, "OCR FTS")
    }

    fn search_ocr_literal_hits(
        &self,
        analysis: &QueryAnalysis,
        limit: usize,
        filters: &RetrievalFilters,
        cursor: Option<&SearchCursorState>,
    ) -> Result<SearchResults> {
        let params = SearchExecutionParams::new(limit, filters, cursor)?;
        let like = format!("%{}%", escape_like_pattern(&analysis.trimmed));
        let prefix_like = format!("{}%", escape_like_pattern(&analysis.lower));
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
                    ":since" : params.since.as_deref(),
                    ":until" : params.until,
                    ":app_like" : params.app_like.as_deref(),
                    ":bundle_id" : params.bundle_id.as_deref(),
                    ":kind" : params.kind,
                    ":has_text" : params.has_text,
                    ":has_url" : params.has_url,
                    ":has_file_url" : params.has_file_url,
                    ":has_image" : params.has_image,
                    ":has_pdf" : params.has_pdf,
                    ":min_bytes" : params.min_bytes,
                    ":max_bytes" : params.max_bytes,
                    ":cursor_score" : params.cursor_score,
                    ":cursor_last_seen_at" : params.cursor_last_seen_at,
                    ":cursor_snapshot_id" : params.cursor_snapshot_id,
                    ":limit" : params.fetch_limit,
                },
                map_scored_search_hit_row,
            )
            .context("execute OCR literal search query")?;

        collect_search_results(rows, limit, SearchMode::Literal, "OCR literal")
    }

    fn search_file_path_literal_page(
        &self,
        path_fragment: &str,
        limit: usize,
        filters: &RetrievalFilters,
        cursor: Option<&SearchCursorState>,
    ) -> Result<SearchResults> {
        let params = SearchExecutionParams::new(limit, filters, cursor)?;
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
                    ":since" : params.since.as_deref(),
                    ":until" : params.until,
                    ":app_like" : params.app_like.as_deref(),
                    ":bundle_id" : params.bundle_id.as_deref(),
                    ":kind" : params.kind,
                    ":has_text" : params.has_text,
                    ":has_url" : params.has_url,
                    ":has_file_url" : params.has_file_url,
                    ":has_image" : params.has_image,
                    ":has_pdf" : params.has_pdf,
                    ":min_bytes" : params.min_bytes,
                    ":max_bytes" : params.max_bytes,
                    ":cursor_score" : params.cursor_score,
                    ":cursor_last_seen_at" : params.cursor_last_seen_at,
                    ":cursor_snapshot_id" : params.cursor_snapshot_id,
                    ":limit" : params.fetch_limit,
                },
                map_scored_search_hit_row,
            )
        } else {
            stmt.query_map(
                named_params! {
                    ":path_fragment" : path_fragment,
                    ":path_like" : path_like.as_str(),
                    ":since" : params.since.as_deref(),
                    ":until" : params.until,
                    ":app_like" : params.app_like.as_deref(),
                    ":bundle_id" : params.bundle_id.as_deref(),
                    ":kind" : params.kind,
                    ":has_text" : params.has_text,
                    ":has_url" : params.has_url,
                    ":has_file_url" : params.has_file_url,
                    ":has_image" : params.has_image,
                    ":has_pdf" : params.has_pdf,
                    ":min_bytes" : params.min_bytes,
                    ":max_bytes" : params.max_bytes,
                    ":cursor_score" : params.cursor_score,
                    ":cursor_last_seen_at" : params.cursor_last_seen_at,
                    ":cursor_snapshot_id" : params.cursor_snapshot_id,
                    ":limit" : params.fetch_limit,
                },
                map_scored_search_hit_row,
            )
        }
        .context("execute file-path literal search query")?;

        collect_search_results(rows, limit, SearchMode::Literal, "file-path literal")
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

        collect_search_results(
            rows,
            limit,
            SearchMode::Literal,
            "unfiltered file-path literal",
        )
    }
}
