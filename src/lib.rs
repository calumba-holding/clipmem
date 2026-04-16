pub(crate) mod app;
pub(crate) mod cli;
pub(crate) mod clipboard;
pub mod db;
pub mod model;
pub(crate) mod paths;
pub(crate) mod platform;
pub(crate) mod render;
pub mod testing;

use anyhow::Result;

/// Run the `clipmem` command-line interface.
///
/// # Errors
///
/// Returns an error if command execution fails.
pub fn run() -> Result<()> {
    cli::run()
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    #[test]
    fn run_entrypoint_is_exposed() {
        let function: fn() -> Result<()> = super::run;

        let _ = function;
    }
}
