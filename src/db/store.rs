use anyhow::Result;
use rusqlite::params;

use crate::model::{CaptureStoreResult, ClipboardItem, ClipboardSnapshot};

use super::{usize_to_i64, Database};

impl Database {
    /// Store a captured clipboard snapshot and append a capture event for the observation.
    ///
    /// # Errors
    ///
    /// Returns an error if any transaction step fails, including snapshot, item,
    /// representation, or capture-event inserts, or the final commit.
    pub fn store_capture(&mut self, snapshot: &ClipboardSnapshot) -> Result<CaptureStoreResult> {
        let tx = self.conn.transaction()?;

        let existing_id: Option<i64> = rusqlite::OptionalExtension::optional(tx.query_row(
            "SELECT id FROM snapshots WHERE sha256 = ?1",
            [snapshot.fingerprint.as_str()],
            |row| row.get(0),
        ))?;

        let snapshot_id = if let Some(id) = existing_id {
            id
        } else {
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
                    snapshot.snapshot_kind.as_str(),
                    snapshot.preview_text,
                    snapshot.search_text,
                    usize_to_i64(snapshot.item_count)?,
                    usize_to_i64(snapshot.total_bytes)?,
                ],
            )?;
            let inserted_snapshot_id = tx.last_insert_rowid();

            for item in &snapshot.items {
                insert_item(&tx, inserted_snapshot_id, item)?;
            }

            inserted_snapshot_id
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
            inserted_new_snapshot: existing_id.is_none(),
        })
    }
}

fn insert_item(
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
            usize_to_i64(item.item_index)?,
            item.primary_kind.as_str(),
            item.primary_uti.as_deref(),
            item.preview_text,
            item.search_text,
            usize_to_i64(item.total_bytes)?,
        ],
    )?;

    for rep in &item.representations {
        tx.execute(
            "INSERT INTO item_representations (
                snapshot_id,
                item_index,
                uti,
                kind,
                byte_len,
                raw_sha256,
                text_value,
                blob_value
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                snapshot_id,
                usize_to_i64(item.item_index)?,
                rep.uti,
                rep.kind.as_str(),
                usize_to_i64(rep.byte_len)?,
                rep.raw_sha256,
                rep.text_value.as_deref(),
                rep.raw_bytes,
            ],
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::Database;
    use crate::clipboard::build_snapshot;
    use crate::model::CaptureContext;

    #[test]
    fn empty_snapshots_store_without_placeholder_rows() -> Result<()> {
        let mut db = Database::open_in_memory()?;
        let snapshot = build_snapshot(CaptureContext::new(9), Vec::new());

        let store = db.store_capture(&snapshot)?;
        let stored_item_count: i64 = db.conn.query_row(
            "SELECT item_count FROM snapshots WHERE id = ?1",
            [store.snapshot_id()],
            |row| row.get(0),
        )?;
        let stored_row_count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM snapshot_items WHERE snapshot_id = ?1",
            [store.snapshot_id()],
            |row| row.get(0),
        )?;

        assert_eq!(stored_item_count, 0);
        assert_eq!(stored_row_count, 0);
        Ok(())
    }
}
