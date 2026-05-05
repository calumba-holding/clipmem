use clap::{error::ErrorKind, Args, ValueEnum};

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
        resolve_json_human_aliases(
            self.format,
            self.json,
            self.human,
            OutputFormat::Text,
            OutputFormat::Json,
            OutputFormat::Human,
            OutputFormat::as_str,
        )
    }
}

#[derive(Debug, Clone, Args)]
pub(super) struct RecallOutputArgs {
    /// Output format: `md` for direct agent use, `json` for structured parsing, `toon` for flattened tabular recall output, or `human` for polished terminal display (default: md).
    #[arg(long, value_enum)]
    pub(in crate::cli) format: Option<RecallOutputFormat>,

    /// Compatibility alias for `--format json`.
    #[arg(long, default_value_t = false)]
    pub(in crate::cli) json: bool,

    /// Compatibility alias for `--format human`.
    #[arg(long, default_value_t = false)]
    pub(in crate::cli) human: bool,
}

impl RecallOutputArgs {
    pub(super) fn resolved(&self) -> Result<RecallOutputFormat, CliValueError> {
        resolve_json_human_aliases(
            self.format,
            self.json,
            self.human,
            RecallOutputFormat::Md,
            RecallOutputFormat::Json,
            RecallOutputFormat::Human,
            RecallOutputFormat::as_str,
        )
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
        resolve_json_human_aliases(
            self.format,
            self.json,
            self.human,
            StatsOutputFormat::Text,
            StatsOutputFormat::Json,
            StatsOutputFormat::Human,
            StatsOutputFormat::as_str,
        )
    }
}

fn resolve_json_human_aliases<F>(
    format: Option<F>,
    json: bool,
    human: bool,
    default_format: F,
    json_format: F,
    human_format: F,
    format_label: fn(F) -> &'static str,
) -> Result<F, CliValueError>
where
    F: Copy + PartialEq,
{
    match (json, human) {
        (false, false) => Ok(format.unwrap_or(default_format)),
        (true, false) => resolve_alias_format(format, "--json", json_format, format_label),
        (false, true) => resolve_alias_format(format, "--human", human_format, format_label),
        (true, true) => Err(value_error(
            ErrorKind::ArgumentConflict,
            "`--human` cannot be combined with `--json`",
        )),
    }
}

fn resolve_alias_format<F>(
    format: Option<F>,
    alias: &str,
    alias_format: F,
    format_label: fn(F) -> &'static str,
) -> Result<F, CliValueError>
where
    F: Copy + PartialEq,
{
    match format {
        None => Ok(alias_format),
        Some(format) if format == alias_format => Ok(format),
        Some(format) => Err(value_error(
            ErrorKind::ArgumentConflict,
            format!(
                "`{alias}` is only compatible with `--format {}`, got `--format {}`",
                format_label(alias_format),
                format_label(format)
            ),
        )),
    }
}

impl ToggleState {
    #[must_use]
    pub(super) fn is_on(self) -> bool {
        matches!(self, Self::On)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        OutputArgs, OutputFormat, RecallOutputArgs, RecallOutputFormat, StatsOutputArgs,
        StatsOutputFormat, ToggleState,
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
                json: false,
                human: false,
            }
            .resolved()
            .unwrap(),
            RecallOutputFormat::Md
        );
        assert_eq!(
            RecallOutputArgs {
                format: Some(RecallOutputFormat::Human),
                json: false,
                human: true,
            }
            .resolved()
            .unwrap(),
            RecallOutputFormat::Human
        );
        assert_eq!(
            RecallOutputArgs {
                format: Some(RecallOutputFormat::Json),
                json: true,
                human: false,
            }
            .resolved()
            .unwrap(),
            RecallOutputFormat::Json
        );

        let conflict = RecallOutputArgs {
            format: Some(RecallOutputFormat::Json),
            json: false,
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
    fn toggle_state_is_on_matches_only_on() {
        assert!(ToggleState::On.is_on());
        assert!(!ToggleState::Off.is_on());
    }
}
