use std::fmt::Write;
use std::io::IsTerminal;

use colored::Colorize;

use crate::db::{StatsReport, StatsSnapshotLeaderboardEntry, StatsTimeBucketEntry};

use super::super::output::RecallMatchConfidence;

pub(crate) const WIDTH: usize = 96;
pub(crate) const BAR_WIDTH: usize = 18;

#[derive(Debug, Clone, Copy)]
pub(crate) struct HumanTheme {
    color: bool,
}

impl HumanTheme {
    pub(crate) fn detect() -> Self {
        let force_color = std::env::var_os("CLICOLOR_FORCE").is_some();
        let no_color = std::env::var_os("NO_COLOR").is_some();
        Self {
            color: force_color || (!no_color && std::io::stdout().is_terminal()),
        }
    }

    pub(crate) fn title(self, text: &str) -> String {
        if self.color {
            text.bold().green().to_string()
        } else {
            text.to_string()
        }
    }

    pub(crate) fn section(self, text: &str) -> String {
        if self.color {
            text.bold().to_string()
        } else {
            text.to_string()
        }
    }

    pub(crate) fn id(self, text: impl ToString) -> String {
        let text = text.to_string();
        if self.color {
            text.bright_cyan().bold().to_string()
        } else {
            text
        }
    }

    pub(crate) fn score(self, value: f64, text: &str) -> String {
        if !self.color {
            return text.to_string();
        }
        if value >= 0.8 {
            text.green().bold().to_string()
        } else if value >= 0.5 {
            text.yellow().bold().to_string()
        } else {
            text.red().bold().to_string()
        }
    }

    pub(crate) fn warning(self, text: &str) -> String {
        if self.color {
            text.yellow().bold().to_string()
        } else {
            text.to_string()
        }
    }

    pub(crate) fn bar(self, value: usize, max: usize, width: usize) -> String {
        let bar = bar(value, max, width);
        if self.color {
            bar.cyan().to_string()
        } else {
            bar
        }
    }
}

pub(crate) fn header(theme: &HumanTheme, title: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", theme.title(title));
    out.push_str(&separator(WIDTH.min(72), true));
    out.push('\n');
    out
}

pub(crate) fn render_filter_summary(value: &serde_json::Value) -> String {
    let Some(object) = value.as_object() else {
        return "none".to_string();
    };
    let pairs = object
        .iter()
        .filter(|(_, value)| !value.is_null() && value != &&serde_json::Value::Bool(false))
        .map(|(key, value)| {
            let rendered = match value {
                serde_json::Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            format!("{key}={rendered}")
        })
        .collect::<Vec<_>>();
    if pairs.is_empty() {
        "none".to_string()
    } else {
        truncate_cell(&pairs.join(", "), 74)
    }
}

pub(crate) fn render_confidence(confidence: &RecallMatchConfidence) -> &'static str {
    match confidence {
        RecallMatchConfidence::High => "high",
        RecallMatchConfidence::Medium => "medium",
        RecallMatchConfidence::Low => "low",
    }
}

pub(crate) fn render_bool_status(value: bool) -> String {
    if value {
        "ok".to_string()
    } else {
        "fail".to_string()
    }
}

pub(crate) fn render_score_cell(theme: &HumanTheme, score: Option<f64>) -> String {
    human_score(score)
        .map(|score| theme.score(score, &format!("{:>6.1}%", score * 100.0)))
        .unwrap_or_else(|| "      -".to_string())
}

pub(crate) fn human_score(score: Option<f64>) -> Option<f64> {
    let score = score?;
    score
        .is_finite()
        .then_some(score)
        .filter(|score| (0.0..=1.0).contains(score))
}

pub(crate) fn first_non_empty<'a>(values: &[&'a str]) -> &'a str {
    values
        .iter()
        .copied()
        .find(|value| !value.trim().is_empty())
        .unwrap_or("")
}

pub(crate) fn peak_bucket(buckets: &[StatsTimeBucketEntry]) -> Option<&StatsTimeBucketEntry> {
    buckets
        .iter()
        .max_by_key(|entry| {
            (
                entry.capture_event_count,
                std::cmp::Reverse(entry.bucket.as_str()),
            )
        })
        .filter(|entry| entry.capture_event_count > 0)
}

pub(crate) fn separator(width: usize, heavy: bool) -> String {
    if heavy {
        "═".repeat(width)
    } else {
        "─".repeat(width)
    }
}

pub(crate) fn bar(value: usize, max: usize, width: usize) -> String {
    if max == 0 || width == 0 {
        return String::new();
    }
    let filled = ((value as f64 / max as f64) * width as f64).round() as usize;
    let filled = filled.min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

pub(crate) fn format_count(value: usize) -> String {
    if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}K", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

pub(crate) fn format_bytes(value: u64) -> String {
    const KB: f64 = 1024.0;
    let value = value as f64;
    if value >= KB * KB * KB {
        format!("{:.1}GB", value / KB / KB / KB)
    } else if value >= KB * KB {
        format!("{:.1}MB", value / KB / KB)
    } else if value >= KB {
        format!("{:.1}KB", value / KB)
    } else {
        format!("{}B", value as u64)
    }
}

pub(crate) fn format_percent(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

pub(crate) fn format_duration_seconds(seconds: u64) -> String {
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

pub(crate) fn format_timestamp_short(timestamp: &str) -> String {
    let trimmed = timestamp.trim();
    if trimmed.len() >= 16 && trimmed.as_bytes().get(10) == Some(&b'T') {
        trimmed[5..16].replace('T', " ")
    } else if trimmed.len() >= 16 && trimmed.as_bytes().get(10) == Some(&b' ') {
        trimmed[5..16].to_string()
    } else {
        truncate_cell(trimmed, 16)
    }
}

pub(crate) fn truncate_multiline(text: &str, limit: usize) -> String {
    truncate_cell(&text.replace('\n', " "), limit)
}

pub(crate) fn truncate_cell(text: &str, width: usize) -> String {
    let text = text.trim();
    let count = text.chars().count();
    if count <= width {
        return text.to_string();
    }
    if width <= 3 {
        return ".".repeat(width);
    }
    let mut out = text.chars().take(width - 3).collect::<String>();
    out.push_str("...");
    out
}
