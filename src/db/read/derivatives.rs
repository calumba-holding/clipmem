use anyhow::{Context, Result};
use rusqlite::{params, OptionalExtension};

use crate::db::sqlite_helpers::{row_usize, usize_to_i64};
use crate::db::types::{Database, RepresentationDerivativePayload};

impl Database {
    /// Read exactly one ready preview derivative for the requested logical source.
    pub fn read_representation_preview(
        &self,
        snapshot_id: i64,
        item_index: usize,
        uti: &str,
    ) -> Result<Option<RepresentationDerivativePayload>> {
        let item_index = usize_to_i64(item_index)?;
        self.conn
            .query_row(
                "SELECT rd.source_raw_sha256, rd.output_uti, rd.encoder_version, rd.encoder_options_hash, rd.width, rd.height, rd.blob_value FROM item_representations ir JOIN representation_derivatives rd ON rd.source_raw_sha256 = ir.raw_sha256 WHERE ir.snapshot_id = ?1 AND ir.item_index = ?2 AND ir.uti = ?3 AND rd.derivative_kind = 'preview' AND rd.status = 'ready' ORDER BY rd.encoder_version DESC LIMIT 1",
                params![snapshot_id, item_index, uti],
                |row| {
                    Ok(RepresentationDerivativePayload {
                        source_raw_sha256: row.get(0)?,
                        output_uti: row.get(1)?,
                        encoder_version: row.get(2)?,
                        encoder_options_hash: row.get(3)?,
                        width: row_usize(row, 4)?,
                        height: row_usize(row, 5)?,
                        bytes: row.get(6)?,
                    })
                },
            )
            .optional()
            .context("load targeted representation preview")
    }
}
