use anyhow::{bail, Result};

use crate::model::{ClipboardItem, ClipboardSnapshot};
use crate::platform::{RestorePlanItem, RestorePlanRepresentation, RestoreReport};

pub fn current_change_count() -> Result<i64> {
    bail!("clipboard capture is only supported on macOS")
}

pub fn capture_snapshot() -> Result<ClipboardSnapshot> {
    bail!("clipboard capture is only supported on macOS")
}

pub(crate) fn build_restore_plan(items: &[ClipboardItem]) -> Vec<RestorePlanItem> {
    items
        .iter()
        .map(|item| {
            RestorePlanItem::new(
                item.item_index(),
                item.representations()
                    .iter()
                    .map(|representation| {
                        RestorePlanRepresentation::new(
                            representation.uti().to_string(),
                            representation.raw_bytes().to_vec(),
                        )
                    })
                    .collect(),
            )
        })
        .collect()
}

pub(crate) fn restore_items(_items: &[ClipboardItem]) -> Result<RestoreReport> {
    bail!("clipboard restore is only supported on macOS")
}
