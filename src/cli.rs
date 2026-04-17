use anyhow::Result;
use clap::{error::ErrorKind, Args, CommandFactory, Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::db::{RetrievalFilters, RetrievalKind, SearchMode, TimelineSort};

mod commands;
mod db_path;
mod output;
mod service;

const ROOT_AFTER_HELP: &str = "\
Examples:
  clipmem setup
  clipmem service status
  clipmem recall \"what was that shell command?\"
  clipmem recent --hours 24
  clipmem timeline --hours 24 --format json
  clipmem get 42 --format json

Agent-first flow:
  1. Use `clipmem recall` for the best answer plus alternatives.
  2. Use `clipmem timeline` for chronological event history.
  3. Use `clipmem search` for direct lexical matching.
  4. Use `clipmem get` for nested snapshot detail.";

const WATCH_AFTER_HELP: &str = "\
Examples:
  clipmem watch
  clipmem watch --interval-ms 350
  clipmem watch --skip-initial

Notes:
  - Default interval is 400 ms.
  - Intervals below 50 ms are clamped to 50 ms at runtime.
  - Status lines stream on stdout; runtime diagnostics go to stderr.";

const CAPTURE_ONCE_AFTER_HELP: &str = "\
Examples:
  clipmem capture-once
  clipmem capture-once --json";

const SEARCH_AFTER_HELP: &str = "\
Examples:
  clipmem search \"git commit -m\"
  clipmem search \"https://example.com/repo\" --format json
  clipmem search \"launchctl bootstrap\" --limit 25 --cursor \"<next_cursor>\" --format json
  clipmem search --mode literal \"foo:bar\"

Notes:
  - `--limit` is the page size. Defaults to 10 and is bounded 1-250.
  - `--cursor` resumes from a prior `next_cursor`. Cursors are opaque and tied to the active query, mode, and filters.
  - Auto mode prefers literal matching for URLs, paths, bundle ids, dotted identifiers, and shell-like fragments when that is more reliable.";

const RECENT_AFTER_HELP: &str = "\
Examples:
  clipmem recent
  clipmem recent --hours 24 --app safari
  clipmem recent --format json --limit 25 --cursor \"<next_cursor>\"

Notes:
  - `recent` is snapshot-centric and deduplicated by snapshot id.
  - `--limit` is the page size. Defaults to 10 and is bounded 1-250.
  - `--cursor` resumes from a prior `next_cursor`. Cursors are opaque and tied to the active filters.";

const TIMELINE_AFTER_HELP: &str = "\
Examples:
  clipmem timeline --hours 24
  clipmem timeline --since 2026-04-16T09:00:00Z --until 2026-04-16T18:00:00Z --sort asc --format json
  clipmem timeline --app safari --has-url --limit 25 --cursor \"<next_cursor>\" --format json

Notes:
  - `timeline` is event-centric and returns one row per real capture event.
  - `--limit` is the page size. Defaults to 10 and is bounded 1-250.
  - `--cursor` resumes from a prior `next_cursor`. Cursors are opaque and tied to the active filters and sort order.";

const RECALL_AFTER_HELP: &str = "\
Examples:
  clipmem recall \"what was that command I copied?\"
  clipmem recall \"find the URL I copied yesterday\" --format json
  clipmem recall --prefer-recent --hours 24
  clipmem recall \"give me the exact text\" --quote --full

Notes:
  - `recall` is the primary best-first retrieval command for agents.
  - `--limit` controls ranked candidates considered. Defaults to 5 and is bounded 1-250.
  - Use `--quote` or `--full` when the exact surfaced text matters more than the compact answer.";

const GET_AFTER_HELP: &str = "\
Examples:
  clipmem get 42
  clipmem get 42 --format json
  clipmem get 42 --events 25 --format md

Notes:
  - `get` is for detailed nested snapshot inspection after you already know the snapshot id.
  - `--events` defaults to 10 and is bounded 1-250.
  - `--format toon` is not supported because `get` returns nested snapshot detail.";

const EXPORT_AFTER_HELP: &str = "\
Examples:
  clipmem export 42 --item 0 --uti public.png --out ./clipboard.png
  clipmem export 42 --item 0 --uti public.utf8-plain-text --out ./clipboard.txt --app terminal
  clipmem export 42 --item 0 --uti public.png --out ./clipboard.png --force

Notes:
  - `export` creates a new file at `--out` by default and refuses to replace an existing path.
  - Pass `--force` to replace an existing regular file.
  - Symlink destinations are never allowed.
  - Success output is written to stdout; failures are written to stderr only.";

const RESTORE_AFTER_HELP: &str = "\
Examples:
  clipmem restore 42

Notes:
  - Restores every stored item and representation for the snapshot back onto the macOS clipboard.
  - The active watcher may record the restored clipboard state as a fresh capture event.";

const FORGET_AFTER_HELP: &str = "\
Examples:
  clipmem forget 42

Notes:
  - Forget irreversibly deletes the snapshot content row, all child representations, and all capture events for that snapshot id.
  - This is a hard delete; there is no recycle bin.";

const PURGE_AFTER_HELP: &str = "\
Examples:
  clipmem purge --older-than 30d
  clipmem purge --older-than 12h --dry-run

Notes:
  - Purge ages snapshots by `last_observed_at`, not the original snapshot creation time.
  - Duration grammar is a single integer plus one unit: `Nd`, `Nh`, or `Nm`.";

const SETTINGS_AFTER_HELP: &str = "\
Examples:
  clipmem settings show
  clipmem settings pause on
  clipmem settings retention 30d
  clipmem settings retention forever
  clipmem settings ignore add com.apple.Passwords
  clipmem settings ignore list --format json";

const DOCTOR_AFTER_HELP: &str = "\
Examples:
  clipmem doctor
  clipmem doctor --json

Notes:
  - Doctor output is requested human or structured output and always goes to stdout.
  - A nonzero exit code means required checks or diagnostics failed.";

const SETUP_AFTER_HELP: &str = "\
Examples:
  clipmem setup
  clipmem setup --db /tmp/clipmem.sqlite3

Notes:
  - `setup` performs one foreground capture, then starts background capture using Homebrew services or a direct LaunchAgent.
  - Re-running `setup` is safe and updates the managed service definition as needed.";

const SERVICE_AFTER_HELP: &str = "\
Examples:
  clipmem service start
  clipmem service status
  clipmem service status --json
  clipmem service stop
  clipmem service uninstall

Notes:
  - Homebrew installs prefer `brew services`; Cargo and manual installs use a direct LaunchAgent.
  - `status` is informational and reports freshness, provider state, and any setup conflicts.";

const SERVICE_STATUS_AFTER_HELP: &str = "\
Examples:
  clipmem service status
  clipmem service status --json

Notes:
  - Text output is intended for humans.
  - `--json` is the stable machine-readable form used by packaged skill health checks.";

const OPENCLAW_INSTALL_AFTER_HELP: &str = "\
Examples:
  clipmem agents openclaw install-skill
  clipmem agents openclaw install-skill --shared
  clipmem agents openclaw install-skill --dest /tmp/clipboard-memory --force

Notes:
  - Default install target is the current OpenClaw workspace skills directory.
  - `--shared` installs into ~/.openclaw/skills/clipboard-memory instead.";

const OPENCLAW_UNINSTALL_AFTER_HELP: &str = "\
Examples:
  clipmem agents openclaw uninstall-skill
  clipmem agents openclaw uninstall-skill --shared
  clipmem agents openclaw uninstall-skill --dest /tmp/clipboard-memory";

const OPENCLAW_PRINT_AFTER_HELP: &str = "\
Examples:
  clipmem agents openclaw print-skill

Notes:
  - Prints the packaged OpenClaw SKILL.md to stdout for inspection or templating.";

const OPENCLAW_DOCTOR_AFTER_HELP: &str = "\
Examples:
  clipmem agents openclaw doctor
  clipmem agents openclaw doctor --shared
  clipmem agents openclaw doctor --dest /tmp/clipboard-memory

Notes:
  - Reports are written to stdout.
  - A nonzero exit code means required OpenClaw integration checks failed.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliExitCode {
    Ok = 0,
    Internal = 1,
    InvalidArgs = 2,
    NotFound = 3,
    UnsupportedFormat = 4,
    DbError = 5,
    PlatformError = 6,
}

impl CliExitCode {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub fn as_exit_code(self) -> ExitCode {
        ExitCode::from(self.as_u8())
    }
}

#[derive(Debug)]
pub struct CliError {
    exit_code: CliExitCode,
    message: String,
}

impl CliError {
    #[must_use]
    pub fn exit_code(&self) -> CliExitCode {
        self.exit_code
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    fn new(exit_code: CliExitCode, message: impl Into<String>) -> Self {
        Self {
            exit_code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DurationValue {
    raw: String,
    seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RetentionValue {
    Forever,
    Duration(DurationValue),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(super) enum PauseState {
    On,
    Off,
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

impl DurationValue {
    #[must_use]
    pub(super) fn new(raw: String, seconds: u64) -> Self {
        Self { raw, seconds }
    }

    #[must_use]
    pub(super) fn raw(&self) -> &str {
        &self.raw
    }

    #[must_use]
    pub(super) fn seconds(&self) -> u64 {
        self.seconds
    }
}

impl RetentionValue {
    #[must_use]
    pub(super) fn retention_seconds(&self) -> Option<u64> {
        match self {
            Self::Forever => None,
            Self::Duration(duration) => Some(duration.seconds()),
        }
    }
}

impl PauseState {
    #[must_use]
    pub(super) fn is_paused(self) -> bool {
        matches!(self, Self::On)
    }
}

#[derive(Debug, Parser)]
#[command(name = "clipmem")]
#[command(version)]
#[command(about = "macOS clipboard memory backed by SQLite")]
#[command(after_help = ROOT_AFTER_HELP)]
#[command(next_line_help = true)]
struct Cli {
    /// Path to the `SQLite` database.
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Manage agent-harness integrations.
    Agents(AgentsArgs),
    /// Initialize the database, seed one capture, and start background capture.
    Setup(SetupArgs),
    /// Manage the background clipmem watcher service.
    Service(ServiceArgs),
    /// Continuously poll the clipboard and archive observed changes.
    Watch(WatchArgs),
    /// Capture the current clipboard state once.
    CaptureOnce(CaptureOnceArgs),
    /// Search the clipboard archive.
    Search(SearchArgs),
    /// Show recent unique clipboard states (deduplicated by snapshot).
    Recent(RecentArgs),
    /// Show chronological clipboard capture events (one row per observation).
    Timeline(TimelineArgs),
    /// Recall the most likely clipboard item for an agent query.
    Recall(RecallArgs),
    /// Show a stored snapshot in detail.
    Get(GetArgs),
    /// Export one stored representation as raw bytes.
    Export(ExportArgs),
    /// Restore a stored snapshot back onto the clipboard.
    Restore(RestoreArgs),
    /// Irreversibly delete one stored snapshot and its capture history.
    Forget(ForgetArgs),
    /// Delete stored snapshots older than a duration.
    Purge(PurgeArgs),
    /// View and update persistent capture policy.
    Settings(SettingsArgs),
    /// Print `SQLite` and FTS5 diagnostics.
    Doctor(DoctorArgs),
}

#[derive(Debug, Args)]
#[command(after_help = WATCH_AFTER_HELP)]
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
#[command(after_help = SETUP_AFTER_HELP)]
struct SetupArgs {}

#[derive(Debug, Args)]
#[command(after_help = SERVICE_AFTER_HELP)]
struct ServiceArgs {
    #[command(subcommand)]
    command: ServiceCommand,
}

#[derive(Debug, Subcommand)]
enum ServiceCommand {
    /// Start background capture using the preferred service provider.
    Start,
    /// Stop background capture without uninstalling the service definition when possible.
    Stop,
    /// Report provider state, freshness, and service wiring.
    Status(ServiceStatusArgs),
    /// Remove the managed service definition.
    Uninstall,
}

#[derive(Debug, Args)]
#[command(after_help = SERVICE_STATUS_AFTER_HELP)]
struct ServiceStatusArgs {
    /// Emit service status as JSON.
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Debug, Args)]
#[command(after_help = CAPTURE_ONCE_AFTER_HELP)]
struct CaptureOnceArgs {
    /// Emit the captured snapshot as JSON.
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Debug, Args)]
#[command(after_help = SEARCH_AFTER_HELP)]
struct SearchArgs {
    /// Query string for the selected search mode. Auto mode handles URLs, paths, bundle ids, exact phrases, and shell fragments more robustly.
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
    filters: RetrievalFilterArgs,

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

fn parse_rfc3339_timestamp(value: &str) -> Result<String, LimitParseError> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map_err(|_| LimitParseError(format!("invalid RFC3339 timestamp '{value}'")))
        .and_then(|timestamp| {
            timestamp
                .format(&Rfc3339)
                .map_err(|error| LimitParseError(format!("format timestamp '{value}': {error}")))
        })
}

fn parse_nonnegative_bytes(value: &str) -> Result<usize, LimitParseError> {
    value
        .parse::<usize>()
        .map_err(|_| LimitParseError(format!("invalid integer value '{value}'")))
}

fn parse_duration_value(value: &str) -> Result<DurationValue, LimitParseError> {
    let trimmed = value.trim();
    if trimmed.len() < 2 {
        return Err(LimitParseError(format!(
            "invalid duration '{value}'; expected <integer><unit> like 30d, 12h, or 15m"
        )));
    }

    let (amount, unit) = trimmed.split_at(trimmed.len() - 1);
    let amount = amount.parse::<u64>().map_err(|_| {
        LimitParseError(format!(
            "invalid duration '{value}'; expected an integer amount before the unit"
        ))
    })?;
    if amount == 0 {
        return Err(LimitParseError(
            "duration must be greater than zero".to_string(),
        ));
    }

    let seconds = match unit.to_ascii_lowercase().as_str() {
        "d" => amount.saturating_mul(24 * 60 * 60),
        "h" => amount.saturating_mul(60 * 60),
        "m" => amount.saturating_mul(60),
        _ => {
            return Err(LimitParseError(format!(
                "invalid duration unit '{unit}'; expected d, h, or m"
            )))
        }
    };

    if seconds == u64::MAX {
        return Err(LimitParseError(format!(
            "duration '{value}' exceeds supported range"
        )));
    }

    Ok(DurationValue::new(trimmed.to_string(), seconds))
}

fn parse_retention_value(value: &str) -> Result<RetentionValue, LimitParseError> {
    if value.trim().eq_ignore_ascii_case("forever") {
        Ok(RetentionValue::Forever)
    } else {
        parse_duration_value(value).map(RetentionValue::Duration)
    }
}

#[derive(Debug, Clone, Args)]
pub(super) struct RetrievalFilterArgs {
    /// Include captures observed at or after this RFC3339 timestamp. When combined with `--hours`, this takes precedence.
    #[arg(long, value_parser = parse_rfc3339_timestamp)]
    since: Option<String>,

    /// Include captures observed at or before this RFC3339 timestamp.
    #[arg(long, value_parser = parse_rfc3339_timestamp)]
    until: Option<String>,

    /// Restrict results to the most recent N hours unless `--since` is provided.
    #[arg(long)]
    hours: Option<u32>,

    /// Filter by recorded frontmost app name using a case-insensitive substring match.
    #[arg(long)]
    app: Option<String>,

    /// Filter by recorded frontmost bundle id using a case-insensitive exact match.
    #[arg(long)]
    bundle_id: Option<String>,

    /// Filter by clipboard content shape. `file` means file URLs; `other` means mixed or empty snapshots.
    #[arg(long, value_enum)]
    kind: Option<RetrievalKind>,

    /// Require at least one non-empty text-like representation.
    #[arg(long, default_value_t = false)]
    has_text: bool,

    /// Require at least one URL representation.
    #[arg(long, default_value_t = false)]
    has_url: bool,

    /// Require at least one file URL representation.
    #[arg(long, default_value_t = false)]
    has_file_url: bool,

    /// Require at least one image representation.
    #[arg(long, default_value_t = false)]
    has_image: bool,

    /// Require at least one PDF representation.
    #[arg(long, default_value_t = false)]
    has_pdf: bool,

    /// Require snapshot size to be at least this many bytes.
    #[arg(long, value_parser = parse_nonnegative_bytes)]
    min_bytes: Option<usize>,

    /// Require snapshot size to be at most this many bytes.
    #[arg(long, value_parser = parse_nonnegative_bytes)]
    max_bytes: Option<usize>,
}

impl RetrievalFilterArgs {
    pub(super) fn normalized(&self) -> std::result::Result<RetrievalFilters, clap::Error> {
        validate_time_window(self.since.as_deref(), self.until.as_deref())?;
        validate_byte_window(self.min_bytes, self.max_bytes)?;

        let app = normalize_nonempty_filter_value(self.app.as_deref(), "--app")?;
        let bundle_id = normalize_nonempty_filter_value(self.bundle_id.as_deref(), "--bundle-id")?;
        let since = self.since.clone();
        let hours = if self.since.is_some() {
            None
        } else {
            self.hours
        };

        Ok(RetrievalFilters::new(
            since,
            self.until.clone(),
            hours,
            app,
            bundle_id,
            self.kind,
            self.has_text,
            self.has_url,
            self.has_file_url,
            self.has_image,
            self.has_pdf,
            self.min_bytes,
            self.max_bytes,
        ))
    }
}

#[derive(Debug, Args)]
#[command(after_help = RECENT_AFTER_HELP)]
struct RecentArgs {
    /// Maximum number of results.
    #[arg(long, default_value_t = 10, value_parser = parse_bounded_limit)]
    limit: usize,

    /// Resume a paginated result set using the opaque `next_cursor` from a prior response.
    #[arg(long)]
    cursor: Option<String>,

    #[command(flatten)]
    filters: RetrievalFilterArgs,

    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Args)]
#[command(after_help = TIMELINE_AFTER_HELP)]
struct TimelineArgs {
    /// Maximum number of results.
    #[arg(long, default_value_t = 10, value_parser = parse_bounded_limit)]
    limit: usize,

    /// Resume a paginated result set using the opaque `next_cursor` from a prior response.
    #[arg(long)]
    cursor: Option<String>,

    /// Sort timeline events chronologically ascending or descending.
    #[arg(long, value_enum, default_value_t = TimelineSort::Desc)]
    sort: TimelineSort,

    #[command(flatten)]
    filters: RetrievalFilterArgs,

    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Args)]
#[command(after_help = RECALL_AFTER_HELP)]
struct RecallArgs {
    /// Optional query describing the clipboard item to recall.
    query: Option<String>,

    /// Search mode to use when a query is present.
    #[arg(long, value_enum, default_value_t = SearchMode::Auto)]
    mode: SearchMode,

    /// Maximum number of ranked candidates to consider.
    #[arg(long, default_value_t = 5, value_parser = parse_bounded_limit)]
    limit: usize,

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
    filters: RetrievalFilterArgs,

    #[command(flatten)]
    output: RecallOutputArgs,
}

#[derive(Debug, Args)]
#[command(after_help = GET_AFTER_HELP)]
struct GetArgs {
    /// Snapshot identifier.
    snapshot_id: i64,

    /// Number of recent events to include.
    #[arg(long, default_value_t = 10, value_parser = parse_bounded_limit)]
    events: usize,

    #[command(flatten)]
    filters: RetrievalFilterArgs,

    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Args)]
#[command(after_help = EXPORT_AFTER_HELP)]
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

    /// Replace an existing regular file at the destination path.
    #[arg(long, default_value_t = false)]
    force: bool,

    #[command(flatten)]
    filters: RetrievalFilterArgs,
}

#[derive(Debug, Args)]
#[command(after_help = RESTORE_AFTER_HELP)]
struct RestoreArgs {
    /// Snapshot identifier.
    snapshot_id: i64,
}

#[derive(Debug, Args)]
#[command(after_help = FORGET_AFTER_HELP)]
struct ForgetArgs {
    /// Snapshot identifier.
    snapshot_id: i64,
}

#[derive(Debug, Args)]
#[command(after_help = PURGE_AFTER_HELP)]
struct PurgeArgs {
    /// Delete snapshots whose last observation is older than this duration (`Nd`, `Nh`, `Nm`).
    #[arg(long, value_parser = parse_duration_value)]
    older_than: DurationValue,

    /// Report what would be deleted without deleting anything.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

#[derive(Debug, Args)]
#[command(after_help = SETTINGS_AFTER_HELP)]
struct SettingsArgs {
    #[command(subcommand)]
    command: SettingsCommand,
}

#[derive(Debug, Subcommand)]
enum SettingsCommand {
    /// Show the current capture policy.
    Show(SettingsShowArgs),
    /// Persistently pause or resume capture.
    Pause(SettingsPauseArgs),
    /// Set retention to a duration or `forever`.
    Retention(SettingsRetentionArgs),
    /// Manage ignored bundle identifiers.
    Ignore(SettingsIgnoreArgs),
}

#[derive(Debug, Args)]
struct SettingsShowArgs {
    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Args)]
struct SettingsPauseArgs {
    /// `on` pauses capture, `off` resumes it.
    state: PauseState,
}

#[derive(Debug, Args)]
struct SettingsRetentionArgs {
    /// Retain snapshots for this duration, or `forever` to disable automatic pruning.
    #[arg(value_parser = parse_retention_value)]
    value: RetentionValue,
}

#[derive(Debug, Args)]
struct SettingsIgnoreArgs {
    #[command(subcommand)]
    command: SettingsIgnoreCommand,
}

#[derive(Debug, Subcommand)]
enum SettingsIgnoreCommand {
    /// Add a bundle identifier to the ignore list.
    Add(SettingsIgnoreBundleArgs),
    /// Remove a bundle identifier from the ignore list.
    Remove(SettingsIgnoreBundleArgs),
    /// List ignored bundle identifiers.
    List(SettingsIgnoreListArgs),
}

#[derive(Debug, Args)]
struct SettingsIgnoreBundleArgs {
    /// Bundle identifier to add or remove.
    bundle_id: String,
}

#[derive(Debug, Args)]
struct SettingsIgnoreListArgs {
    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Args)]
#[command(after_help = DOCTOR_AFTER_HELP)]
struct DoctorArgs {
    /// Emit diagnostics as JSON.
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Debug, Args)]
struct AgentsArgs {
    #[command(subcommand)]
    command: AgentsCommand,
}

#[derive(Debug, Subcommand)]
enum AgentsCommand {
    /// Manage OpenClaw skill integration.
    Openclaw(OpenClawArgs),
}

#[derive(Debug, Args)]
struct OpenClawArgs {
    #[command(subcommand)]
    command: OpenClawCommand,
}

#[derive(Debug, Subcommand)]
enum OpenClawCommand {
    /// Install the packaged clipboard-memory skill into OpenClaw.
    InstallSkill(OpenClawInstallSkillArgs),
    /// Remove an installed OpenClaw clipboard-memory skill.
    UninstallSkill(OpenClawUninstallSkillArgs),
    /// Print the packaged OpenClaw skill content.
    #[command(after_help = OPENCLAW_PRINT_AFTER_HELP)]
    PrintSkill,
    /// Check host PATH, installed skill state, metadata, and sandbox guidance.
    Doctor(OpenClawDoctorArgs),
}

#[derive(Debug, Args)]
#[command(after_help = OPENCLAW_INSTALL_AFTER_HELP)]
struct OpenClawInstallSkillArgs {
    /// Install into the shared OpenClaw skill directory instead of the active workspace.
    #[arg(long, default_value_t = false)]
    shared: bool,

    /// Write the skill into this exact destination directory.
    #[arg(long)]
    dest: Option<PathBuf>,

    /// Replace an existing skill directory if one is already present.
    #[arg(long, default_value_t = false)]
    force: bool,
}

#[derive(Debug, Args)]
#[command(after_help = OPENCLAW_UNINSTALL_AFTER_HELP)]
struct OpenClawUninstallSkillArgs {
    /// Remove from the shared OpenClaw skill directory instead of the active workspace.
    #[arg(long, default_value_t = false)]
    shared: bool,

    /// Remove the skill from this exact destination directory.
    #[arg(long)]
    dest: Option<PathBuf>,
}

#[derive(Debug, Args)]
#[command(after_help = OPENCLAW_DOCTOR_AFTER_HELP)]
struct OpenClawDoctorArgs {
    /// Check the shared OpenClaw skill directory instead of the active workspace.
    #[arg(long, default_value_t = false)]
    shared: bool,

    /// Check this exact destination directory instead of resolving the default target.
    #[arg(long)]
    dest: Option<PathBuf>,
}

/// Parse CLI arguments and execute the requested command.
///
/// # Errors
///
/// Returns an error if command execution fails.
pub fn run() -> std::result::Result<(), CliError> {
    run_from(std::env::args_os())
}

/// Run the CLI entrypoint with an explicit argument vector.
///
/// # Errors
///
/// Returns an error if argument parsing or command execution fails.
pub fn run_from<I, T>(args: I) -> std::result::Result<(), CliError>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = Cli::try_parse_from(args).map_err(classify_clap_error)?;
    validate_cli(&cli).map_err(classify_clap_error)?;
    let db_path = cli.db.unwrap_or_else(db_path::default_db_path);
    commands::run_command(cli.command, &db_path).map_err(classify_command_error)
}

pub fn run_cli() -> ExitCode {
    match try_run_cli(std::env::args_os()) {
        Ok(code) => code.as_exit_code(),
        Err(error) => {
            if error.use_stderr() {
                eprint!("{error}");
            } else {
                print!("{error}");
            }
            classify_clap_exit_code(error.kind()).as_exit_code()
        }
    }
}

fn try_run_cli<I, T>(args: I) -> std::result::Result<CliExitCode, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = Cli::try_parse_from(args)?;
    validate_cli(&cli)?;
    let db_path = cli.db.unwrap_or_else(db_path::default_db_path);
    match commands::run_command(cli.command, &db_path) {
        Ok(()) => Ok(CliExitCode::Ok),
        Err(error) => {
            let classified = classify_command_error(error);
            eprintln!("{}", classified.message());
            Ok(classified.exit_code())
        }
    }
}

fn validate_cli(cli: &Cli) -> std::result::Result<(), clap::Error> {
    match &cli.command {
        Command::Agents(_args) => {}
        Command::Setup(_) => {}
        Command::Service(_) => {}
        Command::Search(args) => {
            args.output.resolved()?;
            args.filters.normalized()?;
        }
        Command::Recent(args) => {
            args.output.resolved()?;
            args.filters.normalized()?;
        }
        Command::Timeline(args) => {
            args.output.resolved()?;
            args.filters.normalized()?;
        }
        Command::Recall(args) => {
            args.filters.normalized()?;
        }
        Command::Get(args) => {
            args.output.resolved()?;
            args.filters.normalized()?;
        }
        Command::Export(args) => {
            args.filters.normalized()?;
        }
        Command::Restore(_) => {}
        Command::Forget(_) => {}
        Command::Purge(_) => {}
        Command::Settings(args) => match &args.command {
            SettingsCommand::Show(args) => {
                args.output.resolved()?;
            }
            SettingsCommand::Pause(_) => {}
            SettingsCommand::Retention(_) => {}
            SettingsCommand::Ignore(args) => match &args.command {
                SettingsIgnoreCommand::Add(_) | SettingsIgnoreCommand::Remove(_) => {}
                SettingsIgnoreCommand::List(args) => {
                    args.output.resolved()?;
                }
            },
        },
        Command::Watch(_) | Command::CaptureOnce(_) | Command::Doctor(_) => {}
    }

    Ok(())
}

fn validate_time_window(
    since: Option<&str>,
    until: Option<&str>,
) -> std::result::Result<(), clap::Error> {
    let Some(since) = since else {
        return Ok(());
    };
    let Some(until) = until else {
        return Ok(());
    };

    let since = OffsetDateTime::parse(since, &Rfc3339)
        .map_err(|error| Cli::command().error(ErrorKind::InvalidValue, error.to_string()))?;
    let until = OffsetDateTime::parse(until, &Rfc3339)
        .map_err(|error| Cli::command().error(ErrorKind::InvalidValue, error.to_string()))?;

    if since > until {
        Err(Cli::command().error(
            ErrorKind::InvalidValue,
            "`--since` must be earlier than or equal to `--until`",
        ))
    } else {
        Ok(())
    }
}

fn validate_byte_window(
    min_bytes: Option<usize>,
    max_bytes: Option<usize>,
) -> std::result::Result<(), clap::Error> {
    if matches!((min_bytes, max_bytes), (Some(min), Some(max)) if min > max) {
        return Err(Cli::command().error(
            ErrorKind::InvalidValue,
            "`--min-bytes` must be less than or equal to `--max-bytes`",
        ));
    }

    Ok(())
}

fn normalize_nonempty_filter_value(
    value: Option<&str>,
    flag: &str,
) -> std::result::Result<Option<String>, clap::Error> {
    match value.map(str::trim) {
        Some("") => {
            Err(Cli::command().error(ErrorKind::InvalidValue, format!("{flag} cannot be empty")))
        }
        Some(trimmed) => Ok(Some(trimmed.to_string())),
        None => Ok(None),
    }
}

fn classify_clap_error(error: clap::Error) -> CliError {
    CliError::new(classify_clap_exit_code(error.kind()), error.to_string())
}

fn classify_clap_exit_code(kind: ErrorKind) -> CliExitCode {
    match kind {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => CliExitCode::Ok,
        _ => CliExitCode::InvalidArgs,
    }
}

fn classify_command_error(error: anyhow::Error) -> CliError {
    let message = sanitize_error_message(&error);
    let exit_code = if is_unsupported_format_error(&error, &message) {
        CliExitCode::UnsupportedFormat
    } else if is_not_found_error(&error, &message) {
        CliExitCode::NotFound
    } else if is_platform_error(&error, &message) {
        CliExitCode::PlatformError
    } else if is_invalid_argument_error(&error, &message) {
        CliExitCode::InvalidArgs
    } else if is_db_error(&error, &message) {
        CliExitCode::DbError
    } else {
        CliExitCode::Internal
    };

    CliError::new(exit_code, message)
}

fn sanitize_error_message(error: &anyhow::Error) -> String {
    let chain_messages = error
        .chain()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>();
    let message = chain_messages
        .first()
        .cloned()
        .unwrap_or_else(|| "command failed".to_string());
    let lower_chain = chain_messages.join(" | ").to_ascii_lowercase();

    if lower_chain.contains("no such column: kind")
        || lower_chain.contains("no such column: ir.kind")
        || lower_chain.contains("no such column: item_representations.kind")
    {
        "database uses an incompatible prerelease schema; move it aside and run `clipmem setup`"
            .to_string()
    } else if lower_chain.contains("no such column:") || lower_chain.contains("sql logic error") {
        "database operation failed; this may be an incompatible prerelease schema. Move the database aside and run `clipmem setup`.".to_string()
    } else if lower_chain.contains("sqlite") {
        "database operation failed; run `clipmem doctor`, and if this is an older prerelease archive, move it aside and run `clipmem setup`.".to_string()
    } else {
        message
    }
}

fn is_invalid_argument_error(error: &anyhow::Error, message: &str) -> bool {
    error.downcast_ref::<clap::Error>().is_some()
        || message.contains("cursor does not match")
        || message.contains("cursor is for command")
        || message.contains("cursor mode")
        || message.contains("invalid cursor")
        || message.contains("already exists (pass --force to replace it)")
        || message.contains("symbolic link")
        || message.contains("not a regular file")
}

fn is_not_found_error(error: &anyhow::Error, message: &str) -> bool {
    if error
        .chain()
        .any(|cause| cause.downcast_ref::<rusqlite::Error>().is_some())
    {
        return false;
    }

    message.contains(" was not found")
        || message.contains("representation not found")
        || message.starts_with("get failed for snapshot ")
        || message.contains("Missing /")
        || message.starts_with("Missing ")
}

fn is_unsupported_format_error(_error: &anyhow::Error, message: &str) -> bool {
    _error
        .downcast_ref::<output::UnsupportedFormatError>()
        .is_some()
        || message.contains("unsupported format")
}

fn is_platform_error(error: &anyhow::Error, message: &str) -> bool {
    message.contains("clipboard capture is only supported on macOS")
        || message.contains("clipboard restore is only supported on macOS")
        || message.contains("setup and service commands are only supported on macOS")
        || message.contains("capture failed")
        || message.contains("capture-once clipboard read failed")
        || message.contains("restore failed")
        || message.contains("read clipboard change count failed")
        || error.chain().any(|cause| {
            let cause = cause.to_string();
            cause.contains("clipboard capture is only supported on macOS")
                || cause.contains("clipboard restore is only supported on macOS")
        })
}

fn is_db_error(error: &anyhow::Error, message: &str) -> bool {
    error.downcast_ref::<rusqlite::Error>().is_some()
        || error
            .chain()
            .any(|cause| cause.downcast_ref::<rusqlite::Error>().is_some())
        || message.contains("database")
        || message.contains("SQLite")
        || message.contains("failed to open database")
        || message.contains("failed to write")
        || message.contains("query failed")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use clap::Parser;

    use crate::db::{SearchMode, TimelineSort};

    use super::{
        classify_command_error, Cli, CliExitCode, Command, OutputFormat, RecallOutputFormat,
        RetentionValue,
    };

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
    fn agents_openclaw_commands_parse_install_and_doctor_flags() {
        let install_cli = Cli::parse_from([
            "clipmem",
            "agents",
            "openclaw",
            "install-skill",
            "--shared",
            "--dest",
            "/tmp/clipboard-memory",
            "--force",
        ]);

        match install_cli.command {
            Command::Agents(args) => match args.command {
                super::AgentsCommand::Openclaw(args) => match args.command {
                    super::OpenClawCommand::InstallSkill(args) => {
                        assert!(args.shared);
                        assert_eq!(args.dest, Some(PathBuf::from("/tmp/clipboard-memory")));
                        assert!(args.force);
                    }
                    other => panic!("expected install-skill command, got {other:?}"),
                },
            },
            other => panic!("expected agents command, got {other:?}"),
        }

        let doctor_cli = Cli::parse_from([
            "clipmem",
            "agents",
            "openclaw",
            "doctor",
            "--dest",
            "/tmp/clipboard-memory",
        ]);
        match doctor_cli.command {
            Command::Agents(args) => match args.command {
                super::AgentsCommand::Openclaw(args) => match args.command {
                    super::OpenClawCommand::Doctor(args) => {
                        assert_eq!(args.dest, Some(PathBuf::from("/tmp/clipboard-memory")));
                        assert!(!args.shared);
                    }
                    other => panic!("expected doctor command, got {other:?}"),
                },
            },
            other => panic!("expected agents command, got {other:?}"),
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
            "clipmem", "recent", "--limit", "5", "--cursor", "abcd", "--format", "jsonl",
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
    fn timeline_command_parses_filters_and_sort() {
        let cli = Cli::parse_from([
            "clipmem",
            "timeline",
            "--since",
            "2026-04-16T09:00:00Z",
            "--until",
            "2026-04-16T10:00:00Z",
            "--hours",
            "24",
            "--limit",
            "5",
            "--cursor",
            "abcd",
            "--sort",
            "asc",
            "--format",
            "md",
        ]);

        match cli.command {
            Command::Timeline(args) => {
                assert_eq!(args.filters.since.as_deref(), Some("2026-04-16T09:00:00Z"));
                assert_eq!(args.filters.until.as_deref(), Some("2026-04-16T10:00:00Z"));
                assert_eq!(args.filters.hours, Some(24));
                assert_eq!(args.limit, 5);
                assert_eq!(args.cursor.as_deref(), Some("abcd"));
                assert_eq!(args.sort, TimelineSort::Asc);
                assert_eq!(args.output.resolved().unwrap(), OutputFormat::Md);
            }
            other => panic!("expected timeline command, got {other:?}"),
        }
    }

    #[test]
    fn timeline_command_defaults_to_desc_sort() {
        let cli = Cli::parse_from(["clipmem", "timeline"]);

        match cli.command {
            Command::Timeline(args) => {
                assert_eq!(args.sort, TimelineSort::Desc);
            }
            other => panic!("expected timeline command, got {other:?}"),
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
                assert_eq!(args.filters.hours, Some(24));
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
    fn timeline_command_rejects_inverted_time_range() {
        let error = super::run_from([
            "clipmem",
            "timeline",
            "--since",
            "2026-04-16T11:00:00Z",
            "--until",
            "2026-04-16T10:00:00Z",
        ])
        .expect_err("invalid time range should fail");

        assert!(error
            .to_string()
            .contains("`--since` must be earlier than or equal to `--until`"));
    }

    #[test]
    fn get_command_parses_shared_filters() {
        let cli = Cli::parse_from([
            "clipmem",
            "get",
            "42",
            "--events",
            "3",
            "--app",
            "terminal",
            "--bundle-id",
            "com.apple.Terminal",
            "--kind",
            "url",
            "--has-url",
            "--min-bytes",
            "20",
            "--max-bytes",
            "200",
            "--format",
            "json",
        ]);

        match cli.command {
            Command::Get(args) => {
                assert_eq!(args.snapshot_id, 42);
                assert_eq!(args.events, 3);
                assert_eq!(args.filters.app.as_deref(), Some("terminal"));
                assert_eq!(
                    args.filters.bundle_id.as_deref(),
                    Some("com.apple.Terminal")
                );
                assert!(matches!(
                    args.filters.kind,
                    Some(crate::db::RetrievalKind::Url)
                ));
                assert!(args.filters.has_url);
                assert_eq!(args.filters.min_bytes, Some(20));
                assert_eq!(args.filters.max_bytes, Some(200));
                assert_eq!(args.output.resolved().unwrap(), OutputFormat::Json);
            }
            other => panic!("expected get command, got {other:?}"),
        }
    }

    #[test]
    fn export_command_parses_shared_filters() {
        let cli = Cli::parse_from([
            "clipmem",
            "export",
            "42",
            "--item",
            "1",
            "--uti",
            "public.url",
            "--out",
            "/tmp/clipmem.url",
            "--since",
            "2026-04-16T09:00:00Z",
            "--hours",
            "24",
            "--app",
            "safari",
            "--kind",
            "file",
            "--has-file-url",
        ]);

        match cli.command {
            Command::Export(args) => {
                assert_eq!(args.snapshot_id, 42);
                assert_eq!(args.item, 1);
                assert_eq!(args.uti, "public.url");
                assert_eq!(args.out, PathBuf::from("/tmp/clipmem.url"));
                assert!(!args.force);
                assert_eq!(args.filters.since.as_deref(), Some("2026-04-16T09:00:00Z"));
                assert_eq!(args.filters.hours, Some(24));
                assert_eq!(args.filters.app.as_deref(), Some("safari"));
                assert!(matches!(
                    args.filters.kind,
                    Some(crate::db::RetrievalKind::File)
                ));
                assert!(args.filters.has_file_url);
            }
            other => panic!("expected export command, got {other:?}"),
        }
    }

    #[test]
    fn shared_filters_reject_inverted_byte_window() {
        let error = super::run_from([
            "clipmem",
            "search",
            "git",
            "--min-bytes",
            "200",
            "--max-bytes",
            "20",
        ])
        .expect_err("invalid byte range should fail");

        assert!(error
            .to_string()
            .contains("`--min-bytes` must be less than or equal to `--max-bytes`"));
    }

    #[test]
    fn shared_filters_reject_empty_app_and_bundle_id_values() {
        let app_error = super::run_from(["clipmem", "recent", "--app", "   "])
            .expect_err("empty app filter should fail");
        assert!(app_error.to_string().contains("--app cannot be empty"));

        let bundle_error = super::run_from(["clipmem", "timeline", "--bundle-id", ""])
            .expect_err("empty bundle id filter should fail");
        assert!(bundle_error
            .to_string()
            .contains("--bundle-id cannot be empty"));
    }

    #[test]
    fn shared_filters_normalize_hours_away_when_since_is_present() {
        let cli = Cli::parse_from([
            "clipmem",
            "search",
            "--since",
            "2026-04-16T09:00:00Z",
            "--hours",
            "24",
            "git",
        ]);

        match cli.command {
            Command::Search(args) => {
                let filters = args.filters.normalized().expect("filters should normalize");
                assert_eq!(filters.since(), Some("2026-04-16T09:00:00Z"));
                assert_eq!(filters.hours(), None);
            }
            other => panic!("expected search command, got {other:?}"),
        }
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
            "--force",
        ]);

        match cli.command {
            Command::Export(args) => {
                assert_eq!(args.snapshot_id, 42);
                assert_eq!(args.item, 1);
                assert_eq!(args.uti, "public.png");
                assert_eq!(args.out, PathBuf::from("/tmp/clipmem.bin"));
                assert!(args.force);
            }
            other => panic!("expected export command, got {other:?}"),
        }
    }

    #[test]
    fn restore_forget_and_purge_commands_parse_expected_arguments() {
        let restore_cli = Cli::parse_from(["clipmem", "restore", "42"]);
        match restore_cli.command {
            Command::Restore(args) => assert_eq!(args.snapshot_id, 42),
            other => panic!("expected restore command, got {other:?}"),
        }

        let forget_cli = Cli::parse_from(["clipmem", "forget", "42"]);
        match forget_cli.command {
            Command::Forget(args) => assert_eq!(args.snapshot_id, 42),
            other => panic!("expected forget command, got {other:?}"),
        }

        let purge_cli = Cli::parse_from(["clipmem", "purge", "--older-than", "30d", "--dry-run"]);
        match purge_cli.command {
            Command::Purge(args) => {
                assert_eq!(args.older_than.raw(), "30d");
                assert_eq!(args.older_than.seconds(), 30 * 24 * 60 * 60);
                assert!(args.dry_run);
            }
            other => panic!("expected purge command, got {other:?}"),
        }
    }

    #[test]
    fn settings_commands_parse_policy_variants() {
        let show_cli = Cli::parse_from(["clipmem", "settings", "show", "--format", "json"]);
        match show_cli.command {
            Command::Settings(args) => match args.command {
                super::SettingsCommand::Show(args) => {
                    assert_eq!(args.output.resolved().unwrap(), OutputFormat::Json);
                }
                other => panic!("expected settings show command, got {other:?}"),
            },
            other => panic!("expected settings command, got {other:?}"),
        }

        let pause_cli = Cli::parse_from(["clipmem", "settings", "pause", "on"]);
        match pause_cli.command {
            Command::Settings(args) => match args.command {
                super::SettingsCommand::Pause(args) => assert!(args.state.is_paused()),
                other => panic!("expected settings pause command, got {other:?}"),
            },
            other => panic!("expected settings command, got {other:?}"),
        }

        let retention_cli = Cli::parse_from(["clipmem", "settings", "retention", "forever"]);
        match retention_cli.command {
            Command::Settings(args) => match args.command {
                super::SettingsCommand::Retention(args) => {
                    assert!(matches!(args.value, RetentionValue::Forever));
                    assert_eq!(args.value.retention_seconds(), None);
                }
                other => panic!("expected settings retention command, got {other:?}"),
            },
            other => panic!("expected settings command, got {other:?}"),
        }

        let ignore_cli =
            Cli::parse_from(["clipmem", "settings", "ignore", "add", "com.apple.Terminal"]);
        match ignore_cli.command {
            Command::Settings(args) => match args.command {
                super::SettingsCommand::Ignore(args) => match args.command {
                    super::SettingsIgnoreCommand::Add(args) => {
                        assert_eq!(args.bundle_id, "com.apple.Terminal");
                    }
                    other => panic!("expected settings ignore add command, got {other:?}"),
                },
                other => panic!("expected settings ignore command, got {other:?}"),
            },
            other => panic!("expected settings command, got {other:?}"),
        }
    }

    #[test]
    fn duration_parser_accepts_single_unit_values_and_rejects_invalid_ones() {
        let days_cli = Cli::parse_from(["clipmem", "purge", "--older-than", "30d"]);
        match days_cli.command {
            Command::Purge(args) => assert_eq!(args.older_than.seconds(), 30 * 24 * 60 * 60),
            other => panic!("expected purge command, got {other:?}"),
        }

        let hours_cli = Cli::parse_from(["clipmem", "purge", "--older-than", "12h"]);
        match hours_cli.command {
            Command::Purge(args) => assert_eq!(args.older_than.seconds(), 12 * 60 * 60),
            other => panic!("expected purge command, got {other:?}"),
        }

        let minutes_cli = Cli::parse_from(["clipmem", "purge", "--older-than", "15m"]);
        match minutes_cli.command {
            Command::Purge(args) => assert_eq!(args.older_than.seconds(), 15 * 60),
            other => panic!("expected purge command, got {other:?}"),
        }

        let compound = super::run_from(["clipmem", "purge", "--older-than", "1h30m"])
            .expect_err("compound durations should fail");
        assert!(compound.to_string().contains("expected an integer amount"));

        let zero = super::run_from(["clipmem", "purge", "--older-than", "0d"])
            .expect_err("zero durations should fail");
        assert!(zero.to_string().contains("greater than zero"));
    }

    #[test]
    fn get_command_rejects_zero_event_limit() {
        let result = Cli::try_parse_from(["clipmem", "get", "42", "--events", "0"]);

        assert!(result.is_err());
    }

    #[test]
    fn command_error_classifier_marks_platform_failures() {
        let error = classify_command_error(anyhow::anyhow!(
            "clipboard capture is only supported on macOS"
        ));

        assert_eq!(error.exit_code(), CliExitCode::PlatformError);
        assert!(error.to_string().contains("only supported on macOS"));
    }
}
