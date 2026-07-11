use anyhow::{Context, Result};
use rusqlite::Connection;

use crate::db::sqlite_helpers::collect_rows;

pub(super) fn ensure_source_provenance_column(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(item_representations)")?;
    let columns = collect_rows(stmt.query_map([], |row| row.get::<_, String>(1))?)?;
    if !columns.iter().any(|column| column == "source_provenance") {
        conn.execute(
            "ALTER TABLE item_representations ADD COLUMN source_provenance TEXT NOT NULL DEFAULT 'captured_original' CHECK (source_provenance IN ('captured_original', 'legacy_transcoded_by_clipmem'))",
            [],
        )
        .context("add source provenance column")?;
    }
    conn.execute(
        "UPDATE item_representations SET source_provenance = 'legacy_transcoded_by_clipmem' WHERE image_compression_status = 'compressed'",
        [],
    )
    .context("classify legacy transcoded image sources")?;
    Ok(())
}
