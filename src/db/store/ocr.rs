use anyhow::{Context, Result};

use crate::db::sqlite_helpers::collect_rows;

pub(in crate::db) fn enqueue_ocr_candidates_tx(
    tx: &rusqlite::Transaction<'_>,
    snapshot_id: Option<i64>,
) -> Result<usize> {
    tx.execute(
        r"
            INSERT INTO ocr_results (raw_sha256, status)
            SELECT DISTINCT ir.raw_sha256, 'pending'
            FROM item_representations ir
            WHERE ir.kind = 'image'
              AND length(ir.blob_value) > 0
              AND (?1 IS NULL OR ir.snapshot_id = ?1)
            ON CONFLICT(raw_sha256) DO NOTHING
        ",
        [snapshot_id],
    )
    .context("enqueue ocr candidates")
}

pub(in crate::db) fn enqueue_ocr_for_snapshot_tx(
    tx: &rusqlite::Transaction<'_>,
    snapshot_id: i64,
) -> Result<usize> {
    enqueue_ocr_candidates_tx(tx, Some(snapshot_id))
}

pub(in crate::db) fn rebuild_snapshot_ocr_cache_for_hash(
    tx: &rusqlite::Transaction<'_>,
    raw_sha256: &str,
) -> Result<()> {
    let mut stmt = tx
        .prepare(
            r"
                SELECT DISTINCT snapshot_id
                FROM item_representations
                WHERE raw_sha256 = ?1
            ",
        )
        .context("prepare affected ocr snapshot query")?;
    let rows = stmt
        .query_map([raw_sha256], |row| row.get::<_, i64>(0))
        .context("execute affected ocr snapshot query")?;
    let snapshot_ids = collect_rows(rows).context("collect affected ocr snapshots")?;
    drop(stmt);
    for snapshot_id in snapshot_ids {
        rebuild_snapshot_ocr_cache(tx, snapshot_id)?;
    }
    Ok(())
}

pub(in crate::db) fn rebuild_snapshot_ocr_cache(
    tx: &rusqlite::Transaction<'_>,
    snapshot_id: i64,
) -> Result<()> {
    tx.execute(
        r"
            INSERT INTO snapshot_ocr_cache (snapshot_id, ocr_text, status, updated_at)
            SELECT
                s.id,
                COALESCE((
                        SELECT GROUP_CONCAT(text_value, char(10) || char(10))
                    FROM (
                        SELECT DISTINCT o.text_value AS text_value
                        FROM item_representations ir
                        JOIN ocr_results o ON o.raw_sha256 = ir.raw_sha256
                        WHERE ir.snapshot_id = s.id
                          AND o.status = 'ready'
                          AND o.text_value IS NOT NULL
                          AND o.text_value != ''
                        ORDER BY o.text_value
                    )
                ), '') AS ocr_text,
                CASE
                    WHEN EXISTS (
                        SELECT 1
                        FROM item_representations ir
                        JOIN ocr_results o ON o.raw_sha256 = ir.raw_sha256
                        WHERE ir.snapshot_id = s.id
                          AND o.status = 'ready'
                          AND o.text_value IS NOT NULL
                          AND o.text_value != ''
                    ) THEN 'ready'
                    WHEN EXISTS (
                        SELECT 1
                        FROM item_representations ir
                        JOIN ocr_results o ON o.raw_sha256 = ir.raw_sha256
                        WHERE ir.snapshot_id = s.id
                          AND o.status = 'pending'
                    ) THEN 'pending'
                    WHEN EXISTS (
                        SELECT 1
                        FROM item_representations ir
                        JOIN ocr_results o ON o.raw_sha256 = ir.raw_sha256
                        WHERE ir.snapshot_id = s.id
                          AND o.status = 'failed'
                    ) THEN 'failed'
                    ELSE 'skipped'
                END AS status,
                CURRENT_TIMESTAMP
            FROM snapshots s
            WHERE s.id = ?1
            ON CONFLICT(snapshot_id) DO UPDATE SET
                ocr_text = excluded.ocr_text,
                status = excluded.status,
                updated_at = excluded.updated_at
        ",
        [snapshot_id],
    )
    .context("rebuild snapshot ocr cache")?;
    Ok(())
}
