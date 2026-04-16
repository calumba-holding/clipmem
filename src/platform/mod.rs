use anyhow::Result;

use crate::model::ClipboardSnapshot;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod unsupported;

#[cfg(target_os = "macos")]
/// Capture the current clipboard into the internal snapshot model.
///
/// # Errors
///
/// Returns an error if platform clipboard access fails.
pub fn capture_snapshot() -> Result<ClipboardSnapshot> {
    macos::capture_snapshot()
}

#[cfg(not(target_os = "macos"))]
/// Capture the current clipboard into the internal snapshot model.
///
/// # Errors
///
/// Always returns an error on unsupported platforms.
pub fn capture_snapshot() -> Result<ClipboardSnapshot> {
    unsupported::capture_snapshot()
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::capture_snapshot;
    use crate::model::ClipboardSnapshot;

    #[test]
    fn capture_snapshot_entrypoint_is_exposed() {
        let function: fn() -> Result<ClipboardSnapshot> = capture_snapshot;

        let _ = function;
    }
}
