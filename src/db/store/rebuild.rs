use anyhow::{Context, Result};
use rusqlite::params;

use crate::db::sqlite_helpers::{collect_rows, row_usize, usize_to_i64};
use crate::db::store::config::ImageOptimizationCandidate;
use crate::model::{truncate_chars, ClipboardItem, ClipboardKind, SnapshotKind};

pub(in crate::db) fn rebuild_snapshot_summary(
    tx: &rusqlite::Transaction<'_>,
    snapshot_id: i64,
) -> Result<()> {
    let mut stmt = tx
        .prepare(
            r"
                SELECT primary_kind, preview_text, search_text, total_bytes
                FROM snapshot_items
                WHERE snapshot_id = ?1
                ORDER BY item_index ASC
            ",
        )
        .context("prepare optimized snapshot summary query")?;
    let rows = stmt
        .query_map([snapshot_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row_usize(row, 3)?,
            ))
        })
        .context("execute optimized snapshot summary query")?;
    let items = collect_rows(rows).context("collect optimized snapshot summary rows")?;
    drop(stmt);

    let item_count = items.len();
    let total_bytes = items.iter().map(|(_, _, _, bytes)| *bytes).sum::<usize>();
    let preview_parts = items
        .iter()
        .map(|(_, preview, _, _)| preview.trim())
        .filter(|preview| !preview.is_empty())
        .collect::<Vec<_>>();
    let preview_text = if preview_parts.is_empty() {
        "[empty clipboard]".to_string()
    } else {
        truncate_chars(&preview_parts.join(" | "), 280)
    };
    let search_text = items
        .iter()
        .map(|(_, _, search, _)| search.trim())
        .filter(|search| !search.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    let snapshot_kind = snapshot_kind_from_item_rows(&items)?;

    tx.execute(
        r"
            UPDATE snapshots
            SET snapshot_kind = ?2,
                preview_text = ?3,
                search_text = ?4,
                item_count = ?5,
                total_bytes = ?6
            WHERE id = ?1
        ",
        params![
            snapshot_id,
            snapshot_kind.as_str(),
            preview_text,
            search_text,
            usize_to_i64(item_count)?,
            usize_to_i64(total_bytes)?
        ],
    )
    .context("update optimized snapshot summary")?;
    Ok(())
}

pub(in crate::db) fn snapshot_kind_from_item_rows(
    items: &[(String, String, String, usize)],
) -> Result<SnapshotKind> {
    if items.is_empty() {
        return Ok(SnapshotKind::Empty);
    }

    let first = parse_item_kind(&items[0].0)?;
    for (kind, _, _, _) in items.iter().skip(1) {
        if parse_item_kind(kind)? != first {
            return Ok(SnapshotKind::Mixed);
        }
    }
    Ok(SnapshotKind::from(first))
}

fn parse_item_kind(kind: &str) -> Result<ClipboardKind> {
    kind.parse::<ClipboardKind>()
        .with_context(|| format!("parse snapshot item kind {kind:?}"))
}

pub(in crate::db) fn snapshot_fingerprint_with_replacement(
    conn: &rusqlite::Connection,
    candidate: &ImageOptimizationCandidate,
    replacement_uti: &str,
    replacement_bytes: &[u8],
) -> Result<String> {
    recompute_snapshot_fingerprint_with(conn, candidate.snapshot_id, |item_index, uti, bytes| {
        if item_index == candidate.item_index && uti == candidate.uti {
            Some((replacement_uti.to_string(), replacement_bytes.to_vec()))
        } else {
            Some((uti.to_string(), bytes.to_vec()))
        }
    })
}

pub(in crate::db) fn recompute_snapshot_fingerprint(
    conn: &rusqlite::Connection,
    snapshot_id: i64,
) -> Result<String> {
    recompute_snapshot_fingerprint_with(conn, snapshot_id, |_, uti, bytes| {
        Some((uti.to_string(), bytes.to_vec()))
    })
}

pub(in crate::db) fn recompute_snapshot_fingerprint_with<F>(
    conn: &rusqlite::Connection,
    snapshot_id: i64,
    mut representation: F,
) -> Result<String>
where
    F: FnMut(i64, &str, &[u8]) -> Option<(String, Vec<u8>)>,
{
    use sha2::{Digest, Sha256};

    let mut item_stmt = conn
        .prepare(
            "SELECT item_index FROM snapshot_items WHERE snapshot_id = ?1 ORDER BY item_index ASC",
        )
        .context("prepare snapshot fingerprint item query")?;
    let item_rows = item_stmt
        .query_map([snapshot_id], |row| row.get::<_, i64>(0))
        .context("execute snapshot fingerprint item query")?;
    let item_indices = collect_rows(item_rows).context("collect snapshot fingerprint items")?;
    drop(item_stmt);

    let mut hasher = Sha256::new();
    hasher.update(b"clipmem/v1");
    for item_index in item_indices {
        hasher.update((item_index as u64).to_be_bytes());
        let mut rep_stmt = conn
            .prepare(
                r"
                    SELECT uti, blob_value
                    FROM item_representations
                    WHERE snapshot_id = ?1 AND item_index = ?2
                    ORDER BY uti ASC
                ",
            )
            .context("prepare snapshot fingerprint representation query")?;
        let rep_rows = rep_stmt
            .query_map(params![snapshot_id, item_index], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .context("execute snapshot fingerprint representation query")?;
        let mut reps =
            collect_rows(rep_rows).context("collect snapshot fingerprint representations")?;
        drop(rep_stmt);
        reps = reps
            .into_iter()
            .filter_map(|(uti, bytes)| representation(item_index, &uti, &bytes))
            .collect();
        reps.sort_by(|(left_uti, _), (right_uti, _)| left_uti.cmp(right_uti));

        for (uti, bytes) in reps {
            hasher.update((uti.len() as u64).to_be_bytes());
            hasher.update(uti.as_bytes());
            hasher.update((bytes.len() as u64).to_be_bytes());
            hasher.update(bytes);
        }
    }

    Ok(hex::encode(hasher.finalize()))
}

pub(in crate::db) fn insert_item(
    tx: &rusqlite::Transaction<'_>,
    snapshot_id: i64,
    item: &ClipboardItem,
) -> Result<()> {
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
            usize_to_i64(item.item_index())?,
            item.primary_kind().as_str(),
            item.primary_uti(),
            item.preview_text(),
            item.search_text(),
            usize_to_i64(item.total_bytes())?,
        ],
    )
    .with_context(|| format!("insert snapshot_items row for item {}", item.item_index()))?;

    for rep in item.representations() {
        tx.execute(
            "INSERT INTO item_representations (
                snapshot_id,
                item_index,
                uti,
                kind,
                byte_len,
                raw_sha256,
                text_value,
                blob_value,
                image_compression_status
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'uncompressed')",
            params![
                snapshot_id,
                usize_to_i64(item.item_index())?,
                rep.uti(),
                rep.kind().as_str(),
                usize_to_i64(rep.byte_len())?,
                rep.raw_sha256(),
                rep.text_value(),
                rep.raw_bytes(),
            ],
        )
        .with_context(|| {
            format!(
                "insert item_representations row for item {} and uti {}",
                item.item_index(),
                rep.uti()
            )
        })?;
    }

    Ok(())
}

pub(in crate::db) fn set_representation_cache_deferred(
    tx: &rusqlite::Transaction<'_>,
    deferred: bool,
) -> Result<()> {
    tx.execute(
        "UPDATE clipmem_settings SET representation_cache_deferred = ?1 WHERE id = 1",
        [if deferred { 1_i64 } else { 0_i64 }],
    )
    .context("update representation cache deferral flag")?;
    Ok(())
}

pub(in crate::db) fn rebuild_snapshot_projection_cache_for_snapshot(
    tx: &rusqlite::Transaction<'_>,
    snapshot_id: i64,
) -> Result<()> {
    tx.execute(
        r"
        INSERT INTO snapshot_projection_cache (snapshot_id, urls, file_urls)
        SELECT
            s.id,
            COALESCE(uv.urls, ''),
            COALESCE(fv.file_urls, '')
        FROM snapshots s
        LEFT JOIN (
            SELECT
                snapshot_id,
                GROUP_CONCAT(text_value, char(31)) AS urls
            FROM (
                SELECT DISTINCT snapshot_id, text_value
                FROM item_representations
                WHERE snapshot_id = ?1
                  AND kind = 'url'
                  AND text_value IS NOT NULL
                  AND text_value != ''
                ORDER BY text_value
            )
            GROUP BY snapshot_id
        ) uv ON uv.snapshot_id = s.id
        LEFT JOIN (
            SELECT
                snapshot_id,
                GROUP_CONCAT(text_value, char(31)) AS file_urls
            FROM (
                SELECT DISTINCT snapshot_id, text_value
                FROM item_representations
                WHERE snapshot_id = ?1
                  AND kind = 'file_url'
                  AND text_value IS NOT NULL
                  AND text_value != ''
                ORDER BY text_value
            )
            GROUP BY snapshot_id
        ) fv ON fv.snapshot_id = s.id
        WHERE s.id = ?1
        ON CONFLICT(snapshot_id) DO UPDATE SET
            urls = excluded.urls,
            file_urls = excluded.file_urls
        ",
        [snapshot_id],
    )
    .context("rebuild snapshot projection cache")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::snapshot_kind_from_item_rows;
    use crate::model::SnapshotKind;

    fn item_row(kind: &str) -> (String, String, String, usize) {
        (kind.to_string(), String::new(), String::new(), 0)
    }

    #[test]
    fn snapshot_kind_from_item_rows_returns_typed_kinds() {
        assert_eq!(
            snapshot_kind_from_item_rows(&[]).expect("empty kind should derive"),
            SnapshotKind::Empty
        );
        assert_eq!(
            snapshot_kind_from_item_rows(&[item_row("plain_text")])
                .expect("single item kind should derive"),
            SnapshotKind::PlainText
        );
        assert_eq!(
            snapshot_kind_from_item_rows(&[item_row("plain_text"), item_row("image")])
                .expect("mixed item kind should derive"),
            SnapshotKind::Mixed
        );
    }

    #[test]
    fn snapshot_kind_from_item_rows_rejects_invalid_item_kind() {
        let error = snapshot_kind_from_item_rows(&[item_row("mixed")])
            .expect_err("snapshot-only kind should not parse as an item kind");

        assert!(
            error
                .to_string()
                .contains("parse snapshot item kind \"mixed\""),
            "unexpected error: {error:#}"
        );
    }
}
