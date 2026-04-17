#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod unsupported;

#[cfg(target_os = "macos")]
pub use self::macos::capture_snapshot;
#[cfg(target_os = "macos")]
pub use self::macos::current_change_count;
#[cfg(not(target_os = "macos"))]
pub use self::unsupported::capture_snapshot;
#[cfg(not(target_os = "macos"))]
pub use self::unsupported::current_change_count;

#[cfg(test)]
mod tests {
    #[test]
    fn capture_snapshot_entrypoint_is_exposed() {
        let capture: fn() -> anyhow::Result<crate::model::ClipboardSnapshot> =
            super::capture_snapshot;

        let _ = capture;
    }

    #[test]
    fn current_change_count_entrypoint_is_exposed() {
        let current_change_count: fn() -> anyhow::Result<i64> = super::current_change_count;

        let _ = current_change_count;
    }
}
