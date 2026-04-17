use anyhow::{bail, Result};

use crate::model::ClipboardSnapshot;

pub fn current_change_count() -> Result<i64> {
    bail!("clipboard capture is only supported on macOS")
}

pub fn capture_snapshot() -> Result<ClipboardSnapshot> {
    bail!("clipboard capture is only supported on macOS")
}
