use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::model::{
    CaptureEvent, CaptureStoreResult, ClipboardItem, ClipboardRepresentation, ClipboardSnapshot,
    DoctorReport, SearchHit, SnapshotDetails,
};

const SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS snapshots (
    id            INTEGER PRIMARY KEY,
    sha256        TEXT NOT NULL UNIQUE,
    snapshot_kind TEXT NOT NULL,
    preview_text  TEXT NOT NULL,
    search_text   TEXT NOT NULL,
    item_count    INTEGER NOT NULL,
    total_bytes   INTEGER NOT NULL,
    created_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS snapshot_items (
    snapshot_id   INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    item_index    INTEGER NOT NULL,
    primary_kind  TEXT NOT NULL,
    primary_uti   TEXT,
    preview_text  TEXT NOT NULL,
    search_text   TEXT NOT NULL,
    total_bytes   INTEGER NOT NULL,
    PRIMARY KEY (snapshot_id, item_index)
);

CREATE TABLE IF NOT EXISTS item_representations (
    snapshot_id    INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    item_index     INTEGER NOT NULL,
    uti            TEXT NOT NULL,
    classification TEXT NOT NULL,
    is_text        INTEGER NOT NULL CHECK (is_text IN (0, 1)),
    byte_len       INTEGER NOT NULL,
    raw_sha256     TEXT NOT NULL,
    text_value     TEXT,
    blob_value     BLOB NOT NULL,
    PRIMARY KEY (snapshot_id, item_index, uti)
);

CREATE TABLE IF NOT EXISTS capture_events (
    id                     INTEGER PRIMARY KEY,
    snapshot_id            INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    observed_at            TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    change_count           INTEGER NOT NULL,
    frontmost_app_bundle_id TEXT,
    frontmost_app_name     TEXT
);

CREATE INDEX IF NOT EXISTS idx_capture_events_snapshot_id
    ON capture_events(snapshot_id);

CREATE INDEX IF NOT EXISTS idx_capture_events_observed_at
    ON capture_events(observed_at DESC);

CREATE INDEX IF NOT EXISTS idx_snapshot_items_snapshot_id
    ON snapshot_items(snapshot_id, item_index);

CREATE VIRTUAL TABLE IF NOT EXISTS snapshots_fts USING fts5(
    search_text,
    preview_text,
    content='snapshots',
    content_rowid='id',
    tokenize='unicode61'
);

CREATE TRIGGER IF NOT EXISTS snapshots_ai AFTER INSERT ON snapshots BEGIN
    INSERT INTO snapshots_fts(rowid, search_text, preview_text)
    VALUES (new.id, new.search_text, new.preview_text);
END;

CREATE TRIGGER IF NOT EXISTS snapshots_ad AFTER DELETE ON snapshots BEGIN
    INSERT INTO snapshots_fts(snapshots_fts, rowid, search_text, preview_text)
    VALUES ('delete', old.id, old.search_text, old.preview_text);
END;

CREATE TRIGGER IF NOT EXISTS snapshots_au AFTER UPDATE ON snapshots BEGIN
    INSERT INTO snapshots_fts(snapshots_fts, rowid, search_text, preview_text)
    VALUES ('delete', old.id, old.search_text, old.preview_text);
    INSERT INTO snapshots_fts(rowid, search_text, preview_text)
    VALUES (new.id, new.search_text, new.preview_text);
END;
"#;

pub struct Database {
    conn: Connection,
    path: PathBuf,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("failed to open {}", path.display()))?;
        configure_connection(&conn)?;
        conn.execute_batch(SCHEMA)?;

        Ok(Self {
            conn,
            path: path.to_path_buf(),
        })
    }

    #[cfg(test)]
    fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        configure_connection(&conn)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn,
            path: PathBuf::from(":memory:"),
        })
    }

    pub fn store_capture(&mut self, snapshot: &ClipboardSnapshot) -> Result<CaptureStoreResult> {
        let tx = self.conn.transaction()?;

        let existing_id: Option<i64> = tx
            .query_row(
                "SELECT id FROM snapshots WHERE sha256 = ?1",
                [snapshot.fingerprint.as_str()],
                |row| row.get(0),
            )
            .optional()?;

        let (snapshot_id, inserted_new_snapshot) = match existing_id {
            Some(id) => (id, false),
            None => {
                tx.execute(
                    "INSERT INTO snapshots (
                        sha256,
                        snapshot_kind,
                        preview_text,
                        search_text,
                        item_count,
                        total_bytes
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        snapshot.fingerprint,
                        snapshot.snapshot_kind,
                        snapshot.preview_text,
                        snapshot.search_text,
                        snapshot.item_count as i64,
                        snapshot.total_bytes as i64,
                    ],
                )?;
                let snapshot_id = tx.last_insert_rowid();

                for item in &snapshot.items {
                    insert_item(&tx, snapshot_id, item)?;
                }

                (snapshot_id, true)
            }
        };

        tx.execute(
            "INSERT INTO capture_events (
                snapshot_id,
                change_count,
                frontmost_app_bundle_id,
                frontmost_app_name
            ) VALUES (?1, ?2, ?3, ?4)",
            params![
                snapshot_id,
                snapshot.change_count,
                snapshot.frontmost_app_bundle_id.as_deref(),
                snapshot.frontmost_app_name.as_deref(),
            ],
        )?;
        let event_id = tx.last_insert_rowid();

        tx.commit()?;

        Ok(CaptureStoreResult {
            snapshot_id,
            event_id,
            inserted_new_snapshot,
        })
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let limit = sanitise_limit(limit);

        // Invalid FTS syntax falls back to literal substring matching.
        let hits = self
            .search_fts(query, limit)
            .or_else(|_| self.search_like(query, limit))?;
        if hits.is_empty() {
            self.search_like(query, limit)
        } else {
            Ok(hits)
        }
    }

    pub fn recent(&self, limit: usize, hours: Option<u32>) -> Result<Vec<SearchHit>> {
        let limit = sanitise_limit(limit);

        let modifier = hours.map(|hours| format!("-{} hours", hours));
        let sql = r#"
            SELECT
                s.id,
                s.sha256,
                s.snapshot_kind,
                s.preview_text,
                s.preview_text AS snippet,
                COUNT(e.id) AS capture_count,
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
                ) AS last_frontmost_app_bundle_id,
                s.total_bytes,
                s.item_count
            FROM snapshots s
            JOIN capture_events e ON e.snapshot_id = s.id
            WHERE (?1 IS NULL OR e.observed_at >= datetime('now', ?1))
            GROUP BY s.id
            ORDER BY last_observed_at DESC, s.id DESC
            LIMIT ?2
        "#;

        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params![modifier.as_deref(), limit as i64], |row| {
            Ok(SearchHit {
                snapshot_id: row.get(0)?,
                sha256: row.get(1)?,
                snapshot_kind: row.get(2)?,
                preview_text: row.get(3)?,
                snippet: row.get(4)?,
                capture_count: row.get(5)?,
                last_observed_at: row.get(6)?,
                last_frontmost_app_name: row.get(7)?,
                last_frontmost_app_bundle_id: row.get(8)?,
                total_bytes: row.get(9)?,
                item_count: row.get(10)?,
                score: None,
            })
        })?;

        collect_rows(rows)
    }

    pub fn get_snapshot(
        &self,
        snapshot_id: i64,
        event_limit: usize,
    ) -> Result<Option<SnapshotDetails>> {
        let summary_sql = r#"
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
        "#;

        let mut details: SnapshotDetails = match self
            .conn
            .query_row(summary_sql, [snapshot_id], |row| {
                Ok(SnapshotDetails {
                    snapshot_id: row.get(0)?,
                    sha256: row.get(1)?,
                    snapshot_kind: row.get(2)?,
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
            .optional()?
        {
            Some(details) => details,
            None => return Ok(None),
        };

        let event_limit = sanitise_limit(event_limit);
        let mut event_stmt = self.conn.prepare(
            r#"
                SELECT id, observed_at, change_count, frontmost_app_name, frontmost_app_bundle_id
                FROM capture_events
                WHERE snapshot_id = ?1
                ORDER BY observed_at DESC, id DESC
                LIMIT ?2
            "#,
        )?;
        let event_rows = event_stmt.query_map(params![snapshot_id, event_limit as i64], |row| {
            Ok(CaptureEvent {
                event_id: row.get(0)?,
                observed_at: row.get(1)?,
                change_count: row.get(2)?,
                frontmost_app_name: row.get(3)?,
                frontmost_app_bundle_id: row.get(4)?,
            })
        })?;
        details.recent_events = collect_rows(event_rows)?;

        let mut item_stmt = self.conn.prepare(
            r#"
                SELECT item_index, primary_kind, primary_uti, preview_text, search_text, total_bytes
                FROM snapshot_items
                WHERE snapshot_id = ?1
                ORDER BY item_index ASC
            "#,
        )?;

        let item_rows = item_stmt.query_map([snapshot_id], |row| {
            Ok(ClipboardItem {
                item_index: row.get::<_, i64>(0)? as usize,
                primary_kind: row.get(1)?,
                primary_uti: row.get(2)?,
                preview_text: row.get(3)?,
                search_text: row.get(4)?,
                total_bytes: row.get::<_, i64>(5)? as usize,
                representations: Vec::new(),
            })
        })?;
        let mut items = collect_rows(item_rows)?;

        let mut rep_stmt = self.conn.prepare(
            r#"
                SELECT
                    uti,
                    classification,
                    is_text,
                    byte_len,
                    raw_sha256,
                    text_value,
                    blob_value
                FROM item_representations
                WHERE snapshot_id = ?1 AND item_index = ?2
                ORDER BY uti ASC
            "#,
        )?;

        for item in &mut items {
            let rep_rows = rep_stmt.query_map(params![snapshot_id, item.item_index as i64], |row| {
                Ok(ClipboardRepresentation {
                    uti: row.get(0)?,
                    classification: row.get(1)?,
                    is_text: row.get::<_, i64>(2)? != 0,
                    byte_len: row.get::<_, i64>(3)? as usize,
                    raw_sha256: row.get(4)?,
                    text_value: row.get(5)?,
                    raw_bytes: row.get(6)?,
                })
            })?;
            item.representations = collect_rows(rep_rows)?;
        }

        details.items = items;
        Ok(Some(details))
    }

    pub fn doctor(&self) -> Result<DoctorReport> {
        let sqlite_version: String =
            self.conn
                .query_row("SELECT sqlite_version()", [], |row| row.get(0))?;

        let journal_mode: String =
            self.conn
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

    fn search_fts(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let sql = r#"
            SELECT
                s.id,
                s.sha256,
                s.snapshot_kind,
                s.preview_text,
                COALESCE(snippet(snapshots_fts, 0, '⟦', '⟧', ' … ', 24), s.preview_text) AS snippet,
                COUNT(e.id) AS capture_count,
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
                ) AS last_frontmost_app_bundle_id,
                s.total_bytes,
                s.item_count,
                bm25(snapshots_fts) AS score
            FROM snapshots_fts
            JOIN snapshots s ON s.id = snapshots_fts.rowid
            LEFT JOIN capture_events e ON e.snapshot_id = s.id
            WHERE snapshots_fts MATCH ?1
            GROUP BY s.id
            ORDER BY score ASC, last_observed_at DESC
            LIMIT ?2
        "#;

        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params![query, limit as i64], |row| {
            Ok(SearchHit {
                snapshot_id: row.get(0)?,
                sha256: row.get(1)?,
                snapshot_kind: row.get(2)?,
                preview_text: row.get(3)?,
                snippet: row.get(4)?,
                capture_count: row.get(5)?,
                last_observed_at: row.get(6)?,
                last_frontmost_app_name: row.get(7)?,
                last_frontmost_app_bundle_id: row.get(8)?,
                total_bytes: row.get(9)?,
                item_count: row.get(10)?,
                score: row.get(11)?,
            })
        })?;

        collect_rows(rows)
    }

    fn search_like(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let like = format!("%{}%", escape_like_pattern(query));
        let sql = r#"
            SELECT
                s.id,
                s.sha256,
                s.snapshot_kind,
                s.preview_text,
                s.preview_text AS snippet,
                COUNT(e.id) AS capture_count,
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
                ) AS last_frontmost_app_bundle_id,
                s.total_bytes,
                s.item_count
            FROM snapshots s
            LEFT JOIN capture_events e ON e.snapshot_id = s.id
            WHERE s.search_text LIKE ?1 ESCAPE '\' OR s.preview_text LIKE ?1 ESCAPE '\'
            GROUP BY s.id
            ORDER BY last_observed_at DESC, s.id DESC
            LIMIT ?2
        "#;

        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params![like, limit as i64], |row| {
            Ok(SearchHit {
                snapshot_id: row.get(0)?,
                sha256: row.get(1)?,
                snapshot_kind: row.get(2)?,
                preview_text: row.get(3)?,
                snippet: row.get(4)?,
                capture_count: row.get(5)?,
                last_observed_at: row.get(6)?,
                last_frontmost_app_name: row.get(7)?,
                last_frontmost_app_bundle_id: row.get(8)?,
                total_bytes: row.get(9)?,
                item_count: row.get(10)?,
                score: None,
            })
        })?;

        collect_rows(rows)
    }
}

fn configure_connection(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    Ok(())
}

fn escape_like_pattern(query: &str) -> String {
    query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn insert_item(tx: &rusqlite::Transaction<'_>, snapshot_id: i64, item: &ClipboardItem) -> Result<()> {
    tx.execute(
        "INSERT INTO snapshot_items (
            snapshot_id,
            item_index,
            primary_kind,
            primary_uti,
            preview_text,
            search_text,
            total_bytes
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            snapshot_id,
            item.item_index as i64,
            item.primary_kind,
            item.primary_uti.as_deref(),
            item.preview_text,
            item.search_text,
            item.total_bytes as i64,
        ],
    )?;

    for rep in &item.representations {
        tx.execute(
            "INSERT INTO item_representations (
                snapshot_id,
                item_index,
                uti,
                classification,
                is_text,
                byte_len,
                raw_sha256,
                text_value,
                blob_value
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                snapshot_id,
                item.item_index as i64,
                rep.uti,
                rep.classification,
                if rep.is_text { 1i64 } else { 0i64 },
                rep.byte_len as i64,
                rep.raw_sha256,
                rep.text_value.as_deref(),
                rep.raw_bytes,
            ],
        )?;
    }

    Ok(())
}

fn sanitise_limit(limit: usize) -> usize {
    limit.clamp(1, 250)
}

fn collect_rows<T, I>(rows: I) -> Result<Vec<T>>
where
    I: Iterator<Item = rusqlite::Result<T>>,
{
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ClipboardItem, ClipboardRepresentation};
    use crate::util::{fingerprint_snapshot, join_item_previews, joined_search_text, snapshot_kind};

    fn fake_snapshot(change_count: i64, text: &str) -> ClipboardSnapshot {
        let rep = ClipboardRepresentation {
            uti: "public.utf8-plain-text".to_string(),
            classification: "plain_text".to_string(),
            is_text: true,
            byte_len: text.len(),
            raw_sha256: crate::util::hash_bytes(text.as_bytes()),
            text_value: Some(text.to_string()),
            raw_bytes: text.as_bytes().to_vec(),
        };

        let item = ClipboardItem {
            item_index: 0,
            primary_kind: "plain_text".to_string(),
            primary_uti: Some("public.utf8-plain-text".to_string()),
            preview_text: text.to_string(),
            search_text: text.to_string(),
            total_bytes: text.len(),
            representations: vec![rep],
        };

        let items = vec![item];
        ClipboardSnapshot {
            change_count,
            frontmost_app_bundle_id: Some("com.example.test".to_string()),
            frontmost_app_name: Some("Test App".to_string()),
            fingerprint: fingerprint_snapshot(&items),
            snapshot_kind: snapshot_kind(&items),
            preview_text: join_item_previews(&items),
            search_text: joined_search_text(&items),
            item_count: items.len(),
            total_bytes: text.len(),
            items,
        }
    }

    #[test]
    fn duplicate_snapshots_share_content_row() -> Result<()> {
        let mut db = Database::open_in_memory()?;

        let first = fake_snapshot(1, "git clone https://example.com/repo");
        let second = fake_snapshot(2, "git clone https://example.com/repo");

        let first_store = db.store_capture(&first)?;
        let second_store = db.store_capture(&second)?;

        assert!(first_store.inserted_new_snapshot);
        assert!(!second_store.inserted_new_snapshot);
        assert_eq!(first_store.snapshot_id, second_store.snapshot_id);

        let hits = db.search("git", 10)?;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].capture_count, 2);

        let details = db.get_snapshot(first_store.snapshot_id, 10)?.unwrap();
        assert_eq!(details.capture_count, 2);
        assert_eq!(details.items.len(), 1);

        Ok(())
    }

    #[test]
    fn search_like_treats_percent_as_literal() -> Result<()> {
        let mut db = Database::open_in_memory()?;

        db.store_capture(&fake_snapshot(1, "Discount: 50 percent off"))?;
        db.store_capture(&fake_snapshot(2, "Discount: 50%"))?;

        let hits = db.search_like("50%", 10)?;
        let previews: Vec<_> = hits.iter().map(|hit| hit.preview_text.as_str()).collect();

        assert_eq!(previews, vec!["Discount: 50%"]);
        Ok(())
    }

    #[test]
    fn search_like_treats_underscore_as_literal() -> Result<()> {
        let mut db = Database::open_in_memory()?;

        db.store_capture(&fake_snapshot(1, "configXtest"))?;
        db.store_capture(&fake_snapshot(2, "config test"))?;
        db.store_capture(&fake_snapshot(3, "config_test"))?;

        let hits = db.search_like("config_test", 10)?;
        let previews: Vec<_> = hits.iter().map(|hit| hit.preview_text.as_str()).collect();

        assert_eq!(previews, vec!["config_test"]);
        Ok(())
    }

    #[test]
    fn search_like_treats_escape_character_as_literal() -> Result<()> {
        let mut db = Database::open_in_memory()?;

        db.store_capture(&fake_snapshot(1, r"logs\2024\archive"))?;
        db.store_capture(&fake_snapshot(2, "logs/2024/archive"))?;

        let hits = db.search_like(r"logs\2024", 10)?;
        let previews: Vec<_> = hits.iter().map(|hit| hit.preview_text.as_str()).collect();

        assert_eq!(previews, vec![r"logs\2024\archive"]);
        Ok(())
    }

    #[test]
    fn search_percent_literal_returns_only_exact_matches() -> Result<()> {
        let mut db = Database::open_in_memory()?;

        db.store_capture(&fake_snapshot(1, "Discount: 50 percent off"))?;
        db.store_capture(&fake_snapshot(2, "Discount: 50%"))?;

        let hits = db.search("50%", 10)?;
        let previews: Vec<_> = hits.iter().map(|hit| hit.preview_text.as_str()).collect();

        assert_eq!(previews, vec!["Discount: 50%"]);
        Ok(())
    }

    #[test]
    fn search_underscore_literal_returns_only_exact_matches() -> Result<()> {
        let mut db = Database::open_in_memory()?;

        db.store_capture(&fake_snapshot(1, "configXtest"))?;
        db.store_capture(&fake_snapshot(2, "config test"))?;
        db.store_capture(&fake_snapshot(3, "config_test"))?;

        let hits = db.search("config_test", 10)?;
        let previews: Vec<_> = hits.iter().map(|hit| hit.preview_text.as_str()).collect();

        assert_eq!(previews, vec!["config_test"]);
        Ok(())
    }
}
