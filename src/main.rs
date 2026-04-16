use anyhow::Result;

fn main() -> Result<()> {
    clipmem::run()
}

#[cfg(test)]
mod tests {
    use anyhow::{anyhow, Result};

    fn invoke(run_app: impl FnOnce() -> Result<()>) -> Result<()> {
        run_app()
    }

    #[test]
    fn main_test_helper_returns_success_from_library_entrypoint() -> Result<()> {
        invoke(|| Ok(()))
    }

    #[test]
    fn main_test_helper_propagates_library_errors() {
        let error = invoke(|| Err(anyhow!("library failed")))
            .expect_err("expected library entrypoint failure");

        assert!(format!("{error:#}").contains("library failed"));
    }
}
