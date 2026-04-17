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
    #[test]
    fn capture_snapshot_entrypoint_is_exposed() {
        let capture: fn() -> anyhow::Result<crate::model::ClipboardSnapshot> =
            super::capture_snapshot;

        let _ = capture;
    }
}
