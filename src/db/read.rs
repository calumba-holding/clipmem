use anyhow::{Context, Result};
use rusqlite::{params, Error as SqlError, ErrorCode, Row};

use crate::model::{
    CaptureEvent, ClipboardItem, ClipboardRepresentation, DoctorReport, SearchHit, SnapshotDetails,
};

use super::{
    collect_rows, row_enum, row_usize, sanitise_limit, usize_to_i64, Database, SearchMode,
    SearchResults,
};

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
    pub fn search_auto(&self, query: &str, limit: usize) -> Result<SearchResults> {
        let limit = sanitise_limit(limit);

        if requires_literal_like_search(query) {
            return self.search_literal(query, limit);
        }

        let results = match self.search_fts(query, limit) {
            Ok(results) => results,
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

        if results.hits().is_empty() {
            self.search_literal(query, limit)
        } else {
            Ok(results)
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

        let mut stmt = self.conn.prepare(&sql).context("prepare recent query")?;
        let rows = stmt
            .query_map(params![modifier.as_deref(), limit], |row| {
                map_search_hit_row(row, false)
            })
            .context("execute recent query")?;

        collect_rows(rows).context("collect recent query rows")
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
    pub fn search_fts(&self, query: &str, limit: usize) -> Result<SearchResults> {
        let limit = usize_to_i64(sanitise_limit(limit))?;
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

        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("prepare FTS search query")?;
        let rows = stmt
            .query_map(params![query, limit], |row| map_search_hit_row(row, true))
            .context("execute FTS search query")?;

        collect_rows(rows)
            .map(|hits| SearchResults::new(SearchMode::Fts, hits))
            .context("collect FTS search rows")
    }

    /// Search stored snapshots with literal `LIKE` matching semantics.
    ///
    /// # Errors
    ///
    /// Returns an error if the literal query cannot be prepared or executed.
    pub fn search_literal(&self, query: &str, limit: usize) -> Result<SearchResults> {
        let limit = usize_to_i64(sanitise_limit(limit))?;
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

        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("prepare literal search query")?;
        let rows = stmt
            .query_map(params![like, limit], |row| map_search_hit_row(row, false))
            .context("execute literal search query")?;

        collect_rows(rows)
            .map(|hits| SearchResults::new(SearchMode::Literal, hits))
            .context("collect literal search rows")
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

fn search_hit_query(snippet_expr: &str, score_expr: Option<&str>, body: &str) -> String {
    let score_select = score_expr
        .map(|expr| format!(",\n        {expr} AS score"))
        .unwrap_or_default();

    format!(
        "{SEARCH_HIT_SELECT_PREFIX}        {snippet_expr} AS snippet,\n{SEARCH_HIT_SELECT_SUFFIX}{score_select}\n{body}"
    )
}

fn map_search_hit_row(row: &Row<'_>, has_score: bool) -> rusqlite::Result<SearchHit> {
    Ok(SearchHit::new(
        row.get(0)?,
        row.get(1)?,
        row_enum(row, 2)?,
        row.get(3)?,
        row.get(4)?,
        row_usize(row, 5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row_usize(row, 9)?,
        row_usize(row, 10)?,
        if has_score { row.get(11)? } else { None },
    ))
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
    use anyhow::Result;

    use crate::db::{Database, SearchMode};
    use crate::model::{build_item, build_representation, build_snapshot, CaptureContext};

    use super::{invalid_fts_message, requires_literal_like_search};

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

    #[test]
    fn search_auto_uses_literal_matching_for_wildcard_queries() -> Result<()> {
        let mut db = Database::open_in_memory()?;

        db.store_capture(&fake_snapshot(1, "Discount: 50 percent off"))?;
        db.store_capture(&fake_snapshot(2, "Discount: 50%"))?;

        let results = db.search_auto("50%", 10)?;
        let hits = results.hits();

        assert_eq!(results.mode_used(), SearchMode::Literal);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].preview_text(), "Discount: 50%");
        Ok(())
    }

    #[test]
    fn find_snapshot_hydrates_nested_items_and_recent_events() -> Result<()> {
        let mut db = Database::open_in_memory()?;
        let first = fake_snapshot(1, "git clone");
        let second = fake_snapshot(2, "git clone");

        let store = db.store_capture(&first)?;
        db.store_capture(&second)?;

        let details = db
            .find_snapshot(store.snapshot_id(), 10)?
            .expect("expected stored snapshot details");

        assert_eq!(details.capture_count(), 2);
        assert_eq!(details.items().len(), 1);
        assert_eq!(details.items()[0].representations().len(), 1);
        assert_eq!(
            details.items()[0].representations()[0].text_value(),
            Some("git clone")
        );
        assert_eq!(details.recent_events().len(), 2);
        Ok(())
    }
}
