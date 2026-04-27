use clap::{error::ErrorKind, Args, ValueEnum};

use crate::db::StatsTimeBucketEntry;

use super::errors::{value_error, CliValueError};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(super) enum ToggleState {
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
    pub(super) fn resolved(&self) -> Result<OutputFormat, CliValueError> {
        match (self.json, self.human, self.format) {
            (false, false, Some(format)) => Ok(format),
            (false, false, None) => Ok(OutputFormat::Text),
            (true, false, None) | (true, false, Some(OutputFormat::Json)) => Ok(OutputFormat::Json),
            (false, true, None) | (false, true, Some(OutputFormat::Human)) => {
                Ok(OutputFormat::Human)
            }
            (true, false, Some(format)) => Err(value_error(
                ErrorKind::ArgumentConflict,
                format!(
                    "`--json` is only compatible with `--format json`, got `--format {}`",
                    format.as_str()
                ),
            )),
            (false, true, Some(format)) => Err(value_error(
                ErrorKind::ArgumentConflict,
                format!(
                    "`--human` is only compatible with `--format human`, got `--format {}`",
                    format.as_str()
                ),
            )),
            (true, true, _) => Err(value_error(
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
    pub(super) fn resolved(&self) -> Result<RecallOutputFormat, CliValueError> {
        match (self.human, self.format) {
            (false, Some(format)) => Ok(format),
            (false, None) => Ok(RecallOutputFormat::Md),
            (true, None) | (true, Some(RecallOutputFormat::Human)) => Ok(RecallOutputFormat::Human),
            (true, Some(format)) => Err(value_error(
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
    pub(super) fn resolved(&self) -> Result<StatsOutputFormat, CliValueError> {
        match (self.json, self.human, self.format) {
            (false, false, Some(format)) => Ok(format),
            (false, false, None) => Ok(StatsOutputFormat::Text),
            (true, false, None) | (true, false, Some(StatsOutputFormat::Json)) => {
                Ok(StatsOutputFormat::Json)
            }
            (false, true, None) | (false, true, Some(StatsOutputFormat::Human)) => {
                Ok(StatsOutputFormat::Human)
            }
            (true, false, Some(format)) => Err(value_error(
                ErrorKind::ArgumentConflict,
                format!(
                    "`--json` is only compatible with `--format json`, got `--format {}`",
                    format.as_str()
                ),
            )),
            (false, true, Some(format)) => Err(value_error(
                ErrorKind::ArgumentConflict,
                format!(
                    "`--human` is only compatible with `--format human`, got `--format {}`",
                    format.as_str()
                ),
            )),
            (true, true, _) => Err(value_error(
                ErrorKind::ArgumentConflict,
                "`--human` cannot be combined with `--json`",
            )),
        }
    }
}

impl ToggleState {
    #[must_use]
    pub(super) fn is_on(self) -> bool {
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

#[cfg(test)]
mod tests {
    use super::{
        format_duration_compact, format_duration_seconds, OutputArgs, OutputFormat,
        RecallOutputArgs, RecallOutputFormat, StatsOutputArgs, StatsOutputFormat, ToggleState,
    };

    #[test]
    fn output_args_defaults_to_text() {
        let args = OutputArgs {
            format: None,
            json: false,
            human: false,
        };

        assert_eq!(args.resolved().unwrap(), OutputFormat::Text);
    }

    #[test]
    fn output_args_accepts_matching_aliases_and_rejects_conflicts() {
        assert_eq!(
            OutputArgs {
                format: Some(OutputFormat::Json),
                json: true,
                human: false,
            }
            .resolved()
            .unwrap(),
            OutputFormat::Json
        );
        assert_eq!(
            OutputArgs {
                format: Some(OutputFormat::Human),
                json: false,
                human: true,
            }
            .resolved()
            .unwrap(),
            OutputFormat::Human
        );

        let json_conflict = OutputArgs {
            format: Some(OutputFormat::Md),
            json: true,
            human: false,
        }
        .resolved()
        .unwrap_err()
        .to_string();
        let human_conflict = OutputArgs {
            format: Some(OutputFormat::Json),
            json: false,
            human: true,
        }
        .resolved()
        .unwrap_err()
        .to_string();
        let alias_conflict = OutputArgs {
            format: None,
            json: true,
            human: true,
        }
        .resolved()
        .unwrap_err()
        .to_string();

        assert!(json_conflict.contains("`--json` is only compatible"));
        assert!(human_conflict.contains("`--human` is only compatible"));
        assert!(alias_conflict.contains("`--human` cannot be combined with `--json`"));
    }

    #[test]
    fn recall_output_args_defaults_to_markdown_and_rejects_human_conflicts() {
        assert_eq!(
            RecallOutputArgs {
                format: None,
                human: false,
            }
            .resolved()
            .unwrap(),
            RecallOutputFormat::Md
        );
        assert_eq!(
            RecallOutputArgs {
                format: Some(RecallOutputFormat::Human),
                human: true,
            }
            .resolved()
            .unwrap(),
            RecallOutputFormat::Human
        );

        let conflict = RecallOutputArgs {
            format: Some(RecallOutputFormat::Json),
            human: true,
        }
        .resolved()
        .unwrap_err()
        .to_string();

        assert!(conflict.contains("`--human` is only compatible"));
    }

    #[test]
    fn stats_output_args_defaults_to_text_and_rejects_alias_conflicts() {
        assert_eq!(
            StatsOutputArgs {
                format: None,
                json: false,
                human: false,
            }
            .resolved()
            .unwrap(),
            StatsOutputFormat::Text
        );
        assert_eq!(
            StatsOutputArgs {
                format: Some(StatsOutputFormat::Json),
                json: true,
                human: false,
            }
            .resolved()
            .unwrap(),
            StatsOutputFormat::Json
        );
        assert_eq!(
            StatsOutputArgs {
                format: Some(StatsOutputFormat::Human),
                json: false,
                human: true,
            }
            .resolved()
            .unwrap(),
            StatsOutputFormat::Human
        );

        let conflict = StatsOutputArgs {
            format: Some(StatsOutputFormat::Text),
            json: true,
            human: false,
        }
        .resolved()
        .unwrap_err()
        .to_string();

        assert!(conflict.contains("`--json` is only compatible"));
    }

    #[test]
    fn format_duration_compact_prefers_days_hours_minutes_then_seconds() {
        assert_eq!(format_duration_compact(172_800), "2d");
        assert_eq!(format_duration_compact(7_200), "2h");
        assert_eq!(format_duration_compact(300), "5m");
        assert_eq!(format_duration_compact(301), "301s");
    }

    #[test]
    fn format_duration_seconds_renders_largest_two_units() {
        assert_eq!(format_duration_seconds(176_400), "2d 1h");
        assert_eq!(format_duration_seconds(7_380), "2h 3m");
        assert_eq!(format_duration_seconds(185), "3m 5s");
        assert_eq!(format_duration_seconds(42), "42s");
    }

    #[test]
    fn toggle_state_is_on_matches_only_on() {
        assert!(ToggleState::On.is_on());
        assert!(!ToggleState::Off.is_on());
    }
}
