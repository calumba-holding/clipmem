use anyhow::Result;

use crate::model::ClipboardSnapshot;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod unsupported;

#[cfg(target_os = "macos")]
pub fn capture_snapshot() -> Result<ClipboardSnapshot> {
    macos::capture_snapshot()
}

#[cfg(not(target_os = "macos"))]
pub fn capture_snapshot() -> Result<ClipboardSnapshot> {
    unsupported::capture_snapshot()
}
