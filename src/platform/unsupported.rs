use anyhow::{bail, Result};

use crate::model::{ClipboardItem, ClipboardSnapshot};
use crate::platform::RestoreReport;

pub fn current_change_count() -> Result<i64> {
    bail!("clipboard capture is only supported on macOS")
}

pub fn capture_snapshot() -> Result<ClipboardSnapshot> {
    bail!("clipboard capture is only supported on macOS")
}

pub(crate) fn restore_items_registered<F>(
    _items: &[ClipboardItem],
    _register: F,
) -> Result<RestoreReport>
where
    F: FnOnce(i64) -> Result<()>,
{
    anyhow::bail!("clipboard restore is only supported on macOS")
}
