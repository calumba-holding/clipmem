pub(crate) mod app;
pub(crate) mod cli;
pub(crate) mod clipboard;
pub mod db;
pub mod model;
pub(crate) mod paths;
pub(crate) mod platform;
pub(crate) mod render;

use anyhow::Result;

/// Run the CLI entrypoint.
///
/// # Errors
///
/// Returns an error if command execution fails.
pub fn run() -> Result<()> {
    cli::run()
}
