use super::*;
use crate::db::StatsTimeBucketEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(super) enum OutputFormat {
    Text,
    Json,
    Jsonl,
    Md,
    Toon,
    Human,
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
            Self::Human => "human",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(super) enum RecallOutputFormat {
    Md,
    Json,
    Toon,
    Human,
}

impl RecallOutputFormat {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Md => "md",
            Self::Json => "json",
            Self::Toon => "toon",
            Self::Human => "human",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(super) enum StatsOutputFormat {
    Text,
    Json,
    Human,
}

impl StatsOutputFormat {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
            Self::Human => "human",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(super) enum ProgressFormat {
    Jsonl,
}

impl ProgressFormat {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Jsonl => "jsonl",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DurationValue {
    pub(in crate::cli) raw: String,
    pub(in crate::cli) seconds: u64,
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
    /// Output format: `text` for terminal use, `json` for stable parsing, `jsonl` for pipelines, `md` for compact review, `toon` for flat list output only, and `human` for polished terminal display (default: text).
    #[arg(long, value_enum)]
    pub(in crate::cli) format: Option<OutputFormat>,

    /// Compatibility alias for `--format json`.
    #[arg(long, default_value_t = false)]
    pub(in crate::cli) json: bool,

    /// Compatibility alias for `--format human`.
    #[arg(long, default_value_t = false)]
    pub(in crate::cli) human: bool,
}

impl OutputArgs {
    pub(super) fn resolved(&self) -> std::result::Result<OutputFormat, clap::Error> {
        match (self.json, self.human, self.format) {
            (false, false, Some(format)) => Ok(format),
            (false, false, None) => Ok(OutputFormat::Text),
            (true, false, None) | (true, false, Some(OutputFormat::Json)) => Ok(OutputFormat::Json),
            (false, true, None) | (false, true, Some(OutputFormat::Human)) => {
                Ok(OutputFormat::Human)
            }
            (true, false, Some(format)) => Err(Cli::command().error(
                ErrorKind::ArgumentConflict,
                format!(
                    "`--json` is only compatible with `--format json`, got `--format {}`",
                    format.as_str()
                ),
            )),
            (false, true, Some(format)) => Err(Cli::command().error(
                ErrorKind::ArgumentConflict,
                format!(
                    "`--human` is only compatible with `--format human`, got `--format {}`",
                    format.as_str()
                ),
            )),
            (true, true, _) => Err(Cli::command().error(
                ErrorKind::ArgumentConflict,
                "`--human` cannot be combined with `--json`",
            )),
        }
    }
}

#[derive(Debug, Clone, Args)]
pub(super) struct RecallOutputArgs {
    /// Output format: `md` for direct agent use, `json` for structured parsing, `toon` for flattened tabular recall output, or `human` for polished terminal display (default: md).
    #[arg(long, value_enum)]
    pub(in crate::cli) format: Option<RecallOutputFormat>,

    /// Compatibility alias for `--format human`.
    #[arg(long, default_value_t = false)]
    pub(in crate::cli) human: bool,
}

impl RecallOutputArgs {
    pub(super) fn resolved(&self) -> std::result::Result<RecallOutputFormat, clap::Error> {
        match (self.human, self.format) {
            (false, Some(format)) => Ok(format),
            (false, None) => Ok(RecallOutputFormat::Md),
            (true, None) | (true, Some(RecallOutputFormat::Human)) => Ok(RecallOutputFormat::Human),
            (true, Some(format)) => Err(Cli::command().error(
                ErrorKind::ArgumentConflict,
                format!(
                    "`--human` is only compatible with `--format human`, got `--format {}`",
                    format.as_str()
                ),
            )),
        }
    }
}

#[derive(Debug, Clone, Args)]
pub(super) struct StatsOutputArgs {
    /// Output format: `text` for terminal use, `json` for stable parsing, or `human` for polished terminal display (default: text).
    #[arg(long, value_enum)]
    pub(in crate::cli) format: Option<StatsOutputFormat>,

    /// Compatibility alias for `--format json`.
    #[arg(long, default_value_t = false)]
    pub(in crate::cli) json: bool,

    /// Compatibility alias for `--format human`.
    #[arg(long, default_value_t = false)]
    pub(in crate::cli) human: bool,
}

impl StatsOutputArgs {
    pub(super) fn resolved(&self) -> std::result::Result<StatsOutputFormat, clap::Error> {
        match (self.json, self.human, self.format) {
            (false, false, Some(format)) => Ok(format),
            (false, false, None) => Ok(StatsOutputFormat::Text),
            (true, false, None) | (true, false, Some(StatsOutputFormat::Json)) => {
                Ok(StatsOutputFormat::Json)
            }
            (false, true, None) | (false, true, Some(StatsOutputFormat::Human)) => {
                Ok(StatsOutputFormat::Human)
            }
            (true, false, Some(format)) => Err(Cli::command().error(
                ErrorKind::ArgumentConflict,
                format!(
                    "`--json` is only compatible with `--format json`, got `--format {}`",
                    format.as_str()
                ),
            )),
            (false, true, Some(format)) => Err(Cli::command().error(
                ErrorKind::ArgumentConflict,
                format!(
                    "`--human` is only compatible with `--format human`, got `--format {}`",
                    format.as_str()
                ),
            )),
            (true, true, _) => Err(Cli::command().error(
                ErrorKind::ArgumentConflict,
                "`--human` cannot be combined with `--json`",
            )),
        }
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

pub(in crate::cli) fn format_duration_compact(seconds: u64) -> String {
    let day = 24 * 60 * 60;
    let hour = 60 * 60;
    let minute = 60;

    if seconds.is_multiple_of(day) {
        format!("{}d", seconds / day)
    } else if seconds.is_multiple_of(hour) {
        format!("{}h", seconds / hour)
    } else if seconds.is_multiple_of(minute) {
        format!("{}m", seconds / minute)
    } else {
        format!("{seconds}s")
    }
}

pub(in crate::cli) fn format_duration_seconds(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let remainder = seconds % 60;

    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {remainder}s")
    } else {
        format!("{remainder}s")
    }
}

pub(in crate::cli) fn peak_bucket(
    buckets: &[StatsTimeBucketEntry],
) -> Option<&StatsTimeBucketEntry> {
    buckets
        .iter()
        .max_by_key(|entry| {
            (
                entry.capture_event_count(),
                std::cmp::Reverse(entry.bucket()),
            )
        })
        .filter(|entry| entry.capture_event_count() > 0)
}
