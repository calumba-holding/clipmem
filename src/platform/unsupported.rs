use anyhow::{bail, Result};

use crate::model::ClipboardSnapshot;

pub fn capture_snapshot() -> Result<ClipboardSnapshot> {
    bail!("clipboard capture is only supported on macOS")
}
