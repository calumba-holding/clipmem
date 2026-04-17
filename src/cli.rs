use anyhow::Result;
use clap::{error::ErrorKind, Args, CommandFactory, Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::PathBuf;

use crate::db::SearchMode;

mod commands;
mod db_path;
mod output;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(super) enum OutputFormat {
    Text,
    Json,
    Jsonl,
    Md,
    Toon,
}

impl OutputFormat {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
            Self::Jsonl => "jsonl",
            Self::Md => "md",
            Self::Toon => "toon",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(super) enum RecallOutputFormat {
    Md,
    Json,
    Toon,
}

#[derive(Debug, Clone, Args)]
pub(super) struct OutputArgs {
    /// Output format: `text` for terminal use, `json` for stable parsing, `jsonl` for pipelines, `md` for compact review, and `toon` for flat list output only (default: text).
    #[arg(long, value_enum)]
    format: Option<OutputFormat>,

    /// Compatibility alias for `--format json`.
    #[arg(long, default_value_t = false)]
    json: bool,
}

impl OutputArgs {
    pub(super) fn resolved(&self) -> std::result::Result<OutputFormat, clap::Error> {
        match (self.json, self.format) {
            (false, Some(format)) => Ok(format),
            (false, None) => Ok(OutputFormat::Text),
            (true, None) | (true, Some(OutputFormat::Json)) => Ok(OutputFormat::Json),
            (true, Some(format)) => Err(Cli::command().error(
                ErrorKind::ArgumentConflict,
                format!(
                    "`--json` is only compatible with `--format json`, got `--format {}`",
                    format.as_str()
                ),
            )),
        }
    }
}

#[derive(Debug, Clone, Args)]
pub(super) struct RecallOutputArgs {
    /// Output format: `md` for direct agent use, `json` for structured parsing, or `toon` for flattened tabular recall output (default: md).
    #[arg(long, value_enum)]
    format: Option<RecallOutputFormat>,
}

impl RecallOutputArgs {
    #[must_use]
    pub(super) fn resolved(&self) -> RecallOutputFormat {
        self.format.unwrap_or(RecallOutputFormat::Md)
    }
}

#[derive(Debug, Parser)]
#[command(name = "clipmem")]
#[command(version)]
#[command(about = "macOS clipboard memory backed by SQLite")]
struct Cli {
    /// Path to the `SQLite` database.
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Continuously poll the clipboard and archive observed changes.
    Watch(WatchArgs),
    /// Capture the current clipboard state once.
    CaptureOnce(CaptureOnceArgs),
    /// Search the clipboard archive.
    Search(SearchArgs),
    /// Show recently captured clipboard states.
    Recent(RecentArgs),
    /// Recall the most likely clipboard item for an agent query.
    Recall(RecallArgs),
    /// Show a stored snapshot in detail.
    Get(GetArgs),
    /// Export one stored representation as raw bytes.
    Export(ExportArgs),
    /// Print `SQLite` and FTS5 diagnostics.
    Doctor(DoctorArgs),
}

#[derive(Debug, Args)]
struct WatchArgs {
    /// Poll interval in milliseconds.
    #[arg(long, default_value_t = 400)]
    interval_ms: u64,

    /// Do not print one-line status messages for each capture.
    #[arg(long, default_value_t = false)]
    quiet: bool,

    /// Skip capturing the clipboard state that already exists when the watcher starts.
    #[arg(long, default_value_t = false)]
    skip_initial: bool,
}

#[derive(Debug, Args)]
struct CaptureOnceArgs {
    /// Emit the captured snapshot as JSON.
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Debug, Args)]
struct SearchArgs {
    /// Query string for the selected search mode.
    query: String,

    /// Search mode to execute.
    #[arg(long, value_enum, default_value_t = SearchMode::Auto)]
    mode: SearchMode,

    /// Maximum number of results.
    #[arg(long, default_value_t = 10, value_parser = parse_bounded_limit)]
    limit: usize,

    /// Resume a paginated result set using the opaque `next_cursor` from a prior response.
    #[arg(long)]
    cursor: Option<String>,

    #[command(flatten)]
    output: OutputArgs,
}

fn parse_normalized_score(value: &str) -> Result<f64, LimitParseError> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| LimitParseError(format!("invalid floating-point value '{value}'")))?;

    if (0.0..=1.0).contains(&parsed) {
        Ok(parsed)
    } else {
        Err(LimitParseError(format!(
            "value must be between 0.0 and 1.0, got {parsed}"
        )))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LimitParseError(String);

impl std::fmt::Display for LimitParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for LimitParseError {}

fn parse_bounded_limit(value: &str) -> Result<usize, LimitParseError> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| LimitParseError(format!("invalid integer value '{value}'")))?;

    if (1..=250).contains(&parsed) {
        Ok(parsed)
    } else {
        Err(LimitParseError(format!(
            "value must be between 1 and 250, got {parsed}"
        )))
    }
}

#[derive(Debug, Args)]
struct RecentArgs {
    /// Maximum number of results.
    #[arg(long, default_value_t = 10, value_parser = parse_bounded_limit)]
    limit: usize,

    /// Restrict results to the most recent N hours.
    #[arg(long)]
    hours: Option<u32>,

    /// Resume a paginated result set using the opaque `next_cursor` from a prior response.
    #[arg(long)]
    cursor: Option<String>,

    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Args)]
struct RecallArgs {
    /// Optional query describing the clipboard item to recall.
    query: Option<String>,

    /// Search mode to use when a query is present.
    #[arg(long, value_enum, default_value_t = SearchMode::Auto)]
    mode: SearchMode,

    /// Maximum number of ranked candidates to consider.
    #[arg(long, default_value_t = 5, value_parser = parse_bounded_limit)]
    limit: usize,

    /// Restrict recent fallback candidates to the most recent N hours.
    #[arg(long)]
    hours: Option<u32>,

    /// Expand the best candidate text instead of returning the compact form.
    #[arg(long, default_value_t = false)]
    full: bool,

    /// Force quoted best-text output when text is available.
    #[arg(long, default_value_t = false)]
    quote: bool,

    /// Minimum normalized match score before search is treated as strong enough on its own.
    #[arg(long, value_parser = parse_normalized_score)]
    min_score: Option<f64>,

    /// Bias ranking toward recency.
    #[arg(long, default_value_t = false)]
    prefer_recent: bool,

    /// Bias ranking toward clipboard events from the matching app or bundle id.
    #[arg(long)]
    prefer_app: Option<String>,

    #[command(flatten)]
    output: RecallOutputArgs,
}

#[derive(Debug, Args)]
struct GetArgs {
    /// Snapshot identifier.
    snapshot_id: i64,

    /// Number of recent events to include.
    #[arg(long, default_value_t = 10, value_parser = parse_bounded_limit)]
    events: usize,

    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Args)]
struct ExportArgs {
    /// Snapshot identifier.
    snapshot_id: i64,

    /// Item index inside the stored snapshot.
    #[arg(long)]
    item: usize,

    /// Representation UTI to export.
    #[arg(long)]
    uti: String,

    /// Destination path for the raw bytes.
    #[arg(long)]
    out: PathBuf,
}

#[derive(Debug, Args)]
struct DoctorArgs {
    /// Emit diagnostics as JSON.
    #[arg(long, default_value_t = false)]
    json: bool,
}

/// Parse CLI arguments and execute the requested command.
///
/// # Errors
///
/// Returns an error if command execution fails.
pub fn run() -> Result<()> {
    run_from(std::env::args_os())
}

/// Run the CLI entrypoint with an explicit argument vector.
///
/// # Errors
///
/// Returns an error if argument parsing or command execution fails.
pub fn run_from<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = Cli::try_parse_from(args)?;
    validate_cli(&cli)?;
    let db_path = cli.db.unwrap_or_else(db_path::default_db_path);
    commands::run_command(cli.command, &db_path)
}

fn validate_cli(cli: &Cli) -> std::result::Result<(), clap::Error> {
    match &cli.command {
        Command::Search(args) => {
            args.output.resolved()?;
        }
        Command::Recent(args) => {
            args.output.resolved()?;
        }
        Command::Recall(_args) => {}
        Command::Get(args) => {
            args.output.resolved()?;
        }
        Command::Watch(_)
        | Command::CaptureOnce(_)
        | Command::Export(_)
        | Command::Doctor(_) => {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use clap::Parser;

    use crate::db::SearchMode;

    use super::{Cli, Command, OutputFormat, RecallOutputFormat};

    #[test]
    fn watch_command_parses_global_db_and_runtime_flags() {
        let cli = Cli::parse_from([
            "clipmem",
            "--db",
            "/tmp/clipmem.sqlite3",
            "watch",
            "--interval-ms",
            "250",
            "--quiet",
            "--skip-initial",
        ]);

        assert_eq!(cli.db, Some(PathBuf::from("/tmp/clipmem.sqlite3")));
        match cli.command {
            Command::Watch(args) => {
                assert_eq!(args.interval_ms, 250);
                assert!(args.quiet);
                assert!(args.skip_initial);
            }
            other => panic!("expected watch command, got {other:?}"),
        }
    }

    #[test]
    fn search_command_parses_explicit_mode() {
        let cli = Cli::parse_from(["clipmem", "search", "--mode", "literal", "50%"]);

        match cli.command {
            Command::Search(args) => {
                assert!(matches!(args.mode, SearchMode::Literal));
                assert_eq!(args.query, "50%");
                assert_eq!(args.output.resolved().unwrap(), OutputFormat::Text);
            }
            other => panic!("expected search command, got {other:?}"),
        }
    }

    #[test]
    fn list_commands_parse_output_format_and_cursor() {
        let cli = Cli::parse_from([
            "clipmem",
            "recent",
            "--limit",
            "5",
            "--cursor",
            "abcd",
            "--format",
            "jsonl",
        ]);

        match cli.command {
            Command::Recent(args) => {
                assert_eq!(args.cursor.as_deref(), Some("abcd"));
                assert_eq!(args.output.resolved().unwrap(), OutputFormat::Jsonl);
            }
            other => panic!("expected recent command, got {other:?}"),
        }
    }

    #[test]
    fn recall_command_parses_optional_query_and_flags() {
        let cli = Cli::parse_from([
            "clipmem",
            "recall",
            "git status",
            "--format",
            "json",
            "--limit",
            "4",
            "--hours",
            "24",
            "--full",
            "--quote",
            "--min-score",
            "0.7",
            "--prefer-recent",
            "--prefer-app",
            "terminal",
        ]);

        match cli.command {
            Command::Recall(args) => {
                assert_eq!(args.query.as_deref(), Some("git status"));
                assert_eq!(args.output.resolved(), RecallOutputFormat::Json);
                assert_eq!(args.limit, 4);
                assert_eq!(args.hours, Some(24));
                assert!(args.full);
                assert!(args.quote);
                assert_eq!(args.min_score, Some(0.7));
                assert!(args.prefer_recent);
                assert_eq!(args.prefer_app.as_deref(), Some("terminal"));
            }
            other => panic!("expected recall command, got {other:?}"),
        }
    }

    #[test]
    fn recall_command_defaults_to_markdown_output() {
        let cli = Cli::parse_from(["clipmem", "recall"]);

        match cli.command {
            Command::Recall(args) => {
                assert_eq!(args.output.resolved(), RecallOutputFormat::Md);
            }
            other => panic!("expected recall command, got {other:?}"),
        }
    }

    #[test]
    fn json_alias_resolves_to_json_output() {
        let cli = Cli::parse_from(["clipmem", "get", "42", "--json"]);

        match cli.command {
            Command::Get(args) => {
                assert_eq!(args.output.resolved().unwrap(), OutputFormat::Json);
            }
            other => panic!("expected get command, got {other:?}"),
        }
    }

    #[test]
    fn json_alias_rejects_non_json_format() {
        let error = super::run_from(["clipmem", "search", "git", "--json", "--format", "md"])
            .expect_err("invalid output alias combination should fail");

        assert!(error
            .to_string()
            .contains("`--json` is only compatible with `--format json`"));
    }

    #[test]
    fn search_command_rejects_zero_limit() {
        let result = Cli::try_parse_from(["clipmem", "search", "--limit", "0", "git"]);

        assert!(result.is_err());
    }

    #[test]
    fn export_command_parses_required_arguments() {
        let cli = Cli::parse_from([
            "clipmem",
            "export",
            "42",
            "--item",
            "1",
            "--uti",
            "public.png",
            "--out",
            "/tmp/clipmem.bin",
        ]);

        match cli.command {
            Command::Export(args) => {
                assert_eq!(args.snapshot_id, 42);
                assert_eq!(args.item, 1);
                assert_eq!(args.uti, "public.png");
                assert_eq!(args.out, PathBuf::from("/tmp/clipmem.bin"));
            }
            other => panic!("expected export command, got {other:?}"),
        }
    }

    #[test]
    fn get_command_rejects_zero_event_limit() {
        let result = Cli::try_parse_from(["clipmem", "get", "42", "--events", "0"]);

        assert!(result.is_err());
    }
}
