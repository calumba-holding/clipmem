#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod unsupported;

/// Capture the current clipboard into the internal snapshot model.
///
/// # Errors
///
/// Returns an error when clipboard capture is unsupported or the platform API fails.
#[cfg(target_os = "macos")]
pub use self::macos::capture_snapshot;
#[cfg(not(target_os = "macos"))]
pub use self::unsupported::capture_snapshot;

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
