pub(crate) mod app;
pub mod archive;
pub mod capture;
pub(crate) mod cli;
pub(crate) mod db;
pub(crate) mod model;
pub(crate) mod platform;

pub use crate::cli::{run, run_cli, run_from, CliError, CliExitCode};

#[cfg(test)]
mod tests {
    use super::{archive, capture, run_from};

    #[test]
    fn run_from_returns_parse_errors_without_exiting_process() {
        let error = run_from(["clipmem", "recent", "--limit", "0"])
            .expect_err("invalid arguments should return an error");

        assert!(error.to_string().contains("between 1 and 250"));
    }

    #[test]
    fn public_facades_expose_archive_and_capture_boundaries() {
        let snapshot = capture::build_snapshot(
            capture::CaptureContext::new(1).with_frontmost_app_name("Terminal"),
            vec![capture::build_item(
                0,
                vec![capture::build_representation(
                    "public.utf8-plain-text".to_string(),
                    None,
                    b"git status".to_vec(),
                )],
            )],
        );

        assert_eq!(archive::SearchMode::Literal.as_str(), "literal");
        assert_eq!(snapshot.preview_text(), "git status");
    }
}
