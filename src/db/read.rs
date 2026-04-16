use anyhow::Result;
use rusqlite::{params, Error as SqlError, ErrorCode, OptionalExtension, Row};

use crate::model::{
    CaptureEvent, ClipboardItem, ClipboardRepresentation, DoctorReport, SearchHit, SnapshotDetails,
};

use super::{collect_rows, row_enum, row_usize, sanitise_limit, usize_to_i64, Database};

const SEARCH_HIT_SELECT_PREFIX: &str = r"
    SELECT
        s.id,
        s.sha256,
        s.snapshot_kind,
        s.preview_text,
";

const SEARCH_HIT_SELECT_SUFFIX: &str = r"
        (
            SELECT COUNT(*)
            FROM capture_events ce
            WHERE ce.snapshot_id = s.id
        ) AS capture_count,
        (
            SELECT MAX(ce.observed_at)
            FROM capture_events ce
            WHERE ce.snapshot_id = s.id
        ) AS last_observed_at,
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
        ) AS last_frontmost_app_bundle_id,
        s.total_bytes,
        s.item_count
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
    pub fn search_auto(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let limit = sanitise_limit(limit);

        if requires_literal_like_search(query) {
            return self.search_literal(query, limit);
        }

        let hits = match self.search_fts(query, limit) {
            Ok(hits) => hits,
            Err(error) => {
                if let Some(sqlite_error) = error.downcast_ref::<SqlError>() {
                    if is_invalid_fts_query(sqlite_error) {
                        self.search_literal(query, limit)?
                    } else {
                        return Err(error);
                    }
                } else {
                    return Err(error);
                }
            }
        };

        if hits.is_empty() {
            self.search_literal(query, limit)
        } else {
            Ok(hits)
        }
    }

    /// Return recently observed snapshots, optionally restricted to the last `hours`.
    ///
    /// # Errors
    ///
    /// Returns an error if the query cannot be prepared or executed.
    pub fn recent(&self, limit: usize, hours: Option<u32>) -> Result<Vec<SearchHit>> {
        let limit = usize_to_i64(sanitise_limit(limit))?;
        let modifier = hours.map(|hours| format!("-{hours} hours"));
        let sql = search_hit_query(
            "s.preview_text",
            None,
            r"
            FROM snapshots s
            WHERE (
                ?1 IS NULL
                OR EXISTS (
                    SELECT 1
                    FROM capture_events ce
                    WHERE ce.snapshot_id = s.id
                      AND ce.observed_at >= datetime('now', ?1)
                )
            )
            ORDER BY last_observed_at DESC, s.id DESC
            LIMIT ?2
        ",
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![modifier.as_deref(), limit], |row| {
            map_search_hit_row(row, false)
        })?;

        collect_rows(rows)
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

        details.recent_events =
            self.load_recent_events(snapshot_id, sanitise_limit(event_limit))?;
        details.items = self.load_snapshot_items(snapshot_id)?;

        Ok(Some(details))
    }

    /// Collect `SQLite` and FTS5 diagnostics for the current archive database.
    ///
    /// # Errors
    ///
    /// Returns an error if any diagnostic query fails.
    pub fn doctor(&self) -> Result<DoctorReport> {
        let sqlite_version: String = self
            .conn
            .query_row("SELECT sqlite_version()", [], |row| row.get(0))?;

        let journal_mode: String = self
            .conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))?;

        let mut compile_stmt = self.conn.prepare("PRAGMA compile_options")?;
        let compile_rows = compile_stmt.query_map([], |row| row.get::<_, String>(0))?;
        let compile_options = collect_rows(compile_rows)?;

        let fts5_compile_option_present = compile_options
            .iter()
            .any(|opt| opt == "ENABLE_FTS5" || opt == "SQLITE_ENABLE_FTS5");

        let fts5_create_virtual_table_ok = self
            .conn
            .execute_batch(
                "CREATE VIRTUAL TABLE temp.__clipmem_fts_check USING fts5(x);
                 DROP TABLE temp.__clipmem_fts_check;",
            )
            .is_ok();

        Ok(DoctorReport {
            db_path: self.path.display().to_string(),
            sqlite_version,
            journal_mode,
            fts5_compile_option_present,
            fts5_create_virtual_table_ok,
            compile_options,
        })
    }

    /// Search stored snapshots with explicit FTS5 semantics.
    ///
    /// # Errors
    ///
    /// Returns an error if the FTS query cannot be prepared or executed.
    pub fn search_fts(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let limit = usize_to_i64(limit)?;
        let sql = search_hit_query(
            "COALESCE(snippet(snapshots_fts, 0, '⟦', '⟧', ' … ', 24), s.preview_text)",
            Some("bm25(snapshots_fts)"),
            r"
            FROM snapshots_fts
            JOIN snapshots s ON s.id = snapshots_fts.rowid
            WHERE snapshots_fts MATCH ?1
            ORDER BY score ASC, last_observed_at DESC
            LIMIT ?2
        ",
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![query, limit], |row| map_search_hit_row(row, true))?;

        collect_rows(rows)
    }

    /// Search stored snapshots with literal `LIKE` matching semantics.
    ///
    /// # Errors
    ///
    /// Returns an error if the literal query cannot be prepared or executed.
    pub fn search_literal(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let limit = usize_to_i64(limit)?;
        let like = format!("%{}%", escape_like_pattern(query));
        let sql = search_hit_query(
            "s.preview_text",
            None,
            r"
            FROM snapshots s
            WHERE s.search_text LIKE ?1 ESCAPE '\' OR s.preview_text LIKE ?1 ESCAPE '\'
            ORDER BY last_observed_at DESC, s.id DESC
            LIMIT ?2
        ",
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![like, limit], |row| map_search_hit_row(row, false))?;

        collect_rows(rows)
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

        self.conn
            .query_row(summary_sql, [snapshot_id], |row| {
                Ok(SnapshotDetails {
                    snapshot_id: row.get(0)?,
                    sha256: row.get(1)?,
                    snapshot_kind: row_enum(row, 2)?,
                    preview_text: row.get(3)?,
                    search_text: row.get(4)?,
                    item_count: row.get(5)?,
                    total_bytes: row.get(6)?,
                    created_at: row.get(7)?,
                    capture_count: row.get(8)?,
                    first_observed_at: row.get(9)?,
                    last_observed_at: row.get(10)?,
                    last_frontmost_app_name: row.get(11)?,
                    last_frontmost_app_bundle_id: row.get(12)?,
                    recent_events: Vec::new(),
                    items: Vec::new(),
                })
            })
            .optional()
            .map_err(Into::into)
    }

    fn load_recent_events(
        &self,
        snapshot_id: i64,
        event_limit: usize,
    ) -> Result<Vec<CaptureEvent>> {
        let limit = usize_to_i64(event_limit)?;
        let mut event_stmt = self.conn.prepare(
            r"
                SELECT id, observed_at, change_count, frontmost_app_name, frontmost_app_bundle_id
                FROM capture_events
                WHERE snapshot_id = ?1
                ORDER BY observed_at DESC, id DESC
                LIMIT ?2
            ",
        )?;
        let event_rows = event_stmt.query_map(params![snapshot_id, limit], |row| {
            Ok(CaptureEvent {
                event_id: row.get(0)?,
                observed_at: row.get(1)?,
                change_count: row.get(2)?,
                frontmost_app_name: row.get(3)?,
                frontmost_app_bundle_id: row.get(4)?,
            })
        })?;

        collect_rows(event_rows)
    }

    fn load_snapshot_items(&self, snapshot_id: i64) -> Result<Vec<ClipboardItem>> {
        let mut item_stmt = self.conn.prepare(
            r"
                SELECT item_index, primary_kind, primary_uti, preview_text, search_text, total_bytes
                FROM snapshot_items
                WHERE snapshot_id = ?1
                ORDER BY item_index ASC
            ",
        )?;

        let item_rows = item_stmt.query_map([snapshot_id], |row| {
            Ok(ClipboardItem {
                item_index: row_usize(row, 0)?,
                primary_kind: row_enum(row, 1)?,
                primary_uti: row.get(2)?,
                preview_text: row.get(3)?,
                search_text: row.get(4)?,
                total_bytes: row_usize(row, 5)?,
                representations: Vec::new(),
            })
        })?;
        let mut items = collect_rows(item_rows)?;

        let mut rep_stmt = self.conn.prepare(
            r"
                SELECT
                    uti,
                    classification,
                    raw_sha256,
                    text_value,
                    blob_value
                FROM item_representations
                WHERE snapshot_id = ?1 AND item_index = ?2
                ORDER BY uti ASC
            ",
        )?;

        for item in &mut items {
            let rep_rows = rep_stmt.query_map(
                params![snapshot_id, usize_to_i64(item.item_index)?],
                |row| {
                    Ok(ClipboardRepresentation::new(
                        row.get(0)?,
                        row_enum(row, 1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )?;
            item.representations = collect_rows(rep_rows)?;
        }

        Ok(items)
    }
}

fn search_hit_query(snippet_expr: &str, score_expr: Option<&str>, body: &str) -> String {
    let score_select = score_expr
        .map(|expr| format!(",\n        {expr} AS score"))
        .unwrap_or_default();

    format!(
        "{SEARCH_HIT_SELECT_PREFIX}        {snippet_expr} AS snippet,\n{SEARCH_HIT_SELECT_SUFFIX}{score_select}\n{body}"
    )
}

fn map_search_hit_row(row: &Row<'_>, has_score: bool) -> rusqlite::Result<SearchHit> {
    Ok(SearchHit {
        snapshot_id: row.get(0)?,
        sha256: row.get(1)?,
        snapshot_kind: row_enum(row, 2)?,
        preview_text: row.get(3)?,
        snippet: row.get(4)?,
        capture_count: row.get(5)?,
        last_observed_at: row.get(6)?,
        last_frontmost_app_name: row.get(7)?,
        last_frontmost_app_bundle_id: row.get(8)?,
        total_bytes: row.get(9)?,
        item_count: row.get(10)?,
        score: if has_score { row.get(11)? } else { None },
    })
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
    use super::{invalid_fts_message, requires_literal_like_search};

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
        assert!(!invalid_fts_message("database disk image is malformed"));
    }
}
