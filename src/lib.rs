pub(crate) mod app;
pub(crate) mod cli;
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

#[cfg(test)]
mod tests {
    use anyhow::{anyhow, Result};

    fn invoke(run_cli: impl FnOnce() -> Result<()>) -> Result<()> {
        run_cli()
    }

    #[test]
    fn run_test_helper_returns_success_from_cli_runner() -> Result<()> {
        invoke(|| Ok(()))
    }

    #[test]
    fn run_test_helper_propagates_cli_errors() {
        let error = invoke(|| Err(anyhow!("boom"))).expect_err("expected runner failure");

        assert!(format!("{error:#}").contains("boom"));
    }
}
