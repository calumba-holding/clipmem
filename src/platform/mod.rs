#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod unsupported;

#[cfg(target_os = "macos")]
pub use self::macos::capture_snapshot;
#[cfg(not(target_os = "macos"))]
pub use self::unsupported::capture_snapshot;
