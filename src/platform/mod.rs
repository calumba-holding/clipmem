#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod unsupported;

#[cfg(target_os = "macos")]
pub use self::macos::capture_snapshot;
#[cfg(not(target_os = "macos"))]
pub use self::unsupported::capture_snapshot;

#[cfg(test)]
mod tests {
    use anyhow::{anyhow, Result};

    use crate::model::{build_snapshot, CaptureContext, ClipboardSnapshot};

    fn invoke_capture(
        capture: impl FnOnce() -> Result<ClipboardSnapshot>,
    ) -> Result<ClipboardSnapshot> {
        capture()
    }

    #[test]
    fn capture_test_helper_returns_snapshot_from_platform_impl() -> Result<()> {
        let snapshot = invoke_capture(|| Ok(build_snapshot(CaptureContext::new(3), Vec::new())))?;

        assert_eq!(snapshot.change_count(), 3);
        Ok(())
    }

    #[test]
    fn capture_test_helper_propagates_platform_errors() {
        let error = invoke_capture(|| Err(anyhow!("platform unavailable")))
            .expect_err("expected platform capture failure");

        assert!(format!("{error:#}").contains("platform unavailable"));
    }
}
