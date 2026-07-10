use crate::cli::human::HumanTheme;
use crate::cli::output::RecallMatchConfidence;

pub(in crate::cli) fn render_filter_summary(value: &serde_json::Value) -> String {
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

pub(in crate::cli) fn render_confidence(confidence: &RecallMatchConfidence) -> &'static str {
    match confidence {
        RecallMatchConfidence::High => "high",
        RecallMatchConfidence::Medium => "medium",
        RecallMatchConfidence::Low => "low",
        RecallMatchConfidence::QueryMatch => "query_match",
    }
}

pub(in crate::cli) fn render_bool_status(value: bool) -> String {
    if value {
        "ok".to_string()
    } else {
        "fail".to_string()
    }
}

pub(in crate::cli) fn render_score_cell(theme: &HumanTheme, score: Option<f64>) -> String {
    human_score(score)
        .map(|score| theme.score(score, &format!("{:>6.1}%", score * 100.0)))
        .unwrap_or_else(|| "      -".to_string())
}

pub(in crate::cli) fn human_score(score: Option<f64>) -> Option<f64> {
    let score = score?;
    score
        .is_finite()
        .then_some(score)
        .filter(|score| (0.0..=1.0).contains(score))
}

pub(in crate::cli) fn first_non_empty<'a>(values: &[&'a str]) -> &'a str {
    values
        .iter()
        .copied()
        .find(|value| !value.trim().is_empty())
        .unwrap_or("")
}

pub(in crate::cli) fn separator(width: usize, heavy: bool) -> String {
    if heavy {
        "═".repeat(width)
    } else {
        "─".repeat(width)
    }
}

pub(in crate::cli) fn bar(value: usize, max: usize, width: usize) -> String {
    if max == 0 || width == 0 {
        return String::new();
    }
    let filled = ((value as f64 / max as f64) * width as f64).round() as usize;
    let filled = filled.min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

pub(in crate::cli) fn format_count(value: usize) -> String {
    if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}K", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

pub(in crate::cli) fn format_bytes(value: u64) -> String {
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

pub(in crate::cli) fn format_percent(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

pub(in crate::cli) fn format_timestamp_short(timestamp: &str) -> String {
    let trimmed = timestamp.trim();
    if trimmed.len() >= 16 && trimmed.as_bytes().get(10) == Some(&b'T') {
        trimmed[5..16].replace('T', " ")
    } else if trimmed.len() >= 16 && trimmed.as_bytes().get(10) == Some(&b' ') {
        trimmed[5..16].to_string()
    } else {
        truncate_cell(trimmed, 16)
    }
}

pub(in crate::cli) fn truncate_multiline(text: &str, limit: usize) -> String {
    truncate_cell_mapped(text, limit, |ch| if ch == '\n' { ' ' } else { ch })
}

pub(in crate::cli) fn truncate_cell(text: &str, width: usize) -> String {
    truncate_cell_mapped(text, width, std::convert::identity)
}

fn truncate_cell_mapped(text: &str, width: usize, map_char: impl Fn(char) -> char) -> String {
    let text = text.trim();
    let mut chars = text.chars();
    for _ in 0..width {
        if chars.next().is_none() {
            return text.chars().map(map_char).collect();
        }
    }
    if chars.next().is_none() {
        return text.chars().map(map_char).collect();
    }

    if width <= 3 {
        return ".".repeat(width);
    }

    let mut out = String::with_capacity(width);
    for ch in text.chars().take(width - 3) {
        out.push(map_char(ch));
    }
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::{truncate_cell, truncate_multiline};

    #[test]
    fn truncate_cell_preserves_existing_width_behavior() {
        assert_eq!(truncate_cell(" git status ", 20), "git status");
        assert_eq!(truncate_cell("abcdef", 6), "abcdef");
        assert_eq!(truncate_cell("abcdef", 5), "ab...");
        assert_eq!(truncate_cell("abcdef", 3), "...");
        assert_eq!(truncate_cell("abcdef", 0), "");
    }

    #[test]
    fn truncate_multiline_replaces_newlines_without_full_input_copy() {
        assert_eq!(truncate_multiline("git\nstatus", 20), "git status");
        assert_eq!(
            truncate_multiline("git\nstatus\nwith\nlong\noutput", 12),
            "git statu..."
        );
    }
}

#[cfg(test)]
mod profile_tests {
    use super::truncate_cell;
    use std::time::{Duration, Instant};

    #[test]
    #[ignore = "profiling harness for human CLI cell truncation"]
    fn profile_large_human_cell_truncation() {
        let cells = large_human_cells(10_000, 4_000);

        let before = median_duration(11, 340_000, || {
            cells
                .iter()
                .map(|cell| truncate_cell_before_for_profile(cell, 34).len())
                .sum()
        });
        let after = median_duration(11, 340_000, || {
            cells.iter().map(|cell| truncate_cell(cell, 34).len()).sum()
        });

        eprintln!(
            "human_cell_truncate_full_count_before={before:?} human_cell_truncate_bounded_after={after:?}"
        );
    }

    fn median_duration(
        runs: usize,
        expected_total: usize,
        mut f: impl FnMut() -> usize,
    ) -> Duration {
        let mut samples = Vec::with_capacity(runs);
        for _ in 0..runs {
            let started = Instant::now();
            let total = f();
            assert_eq!(total, expected_total);
            samples.push(started.elapsed());
        }
        samples.sort();
        samples[samples.len() / 2]
    }

    fn large_human_cells(count: usize, chars_per_cell: usize) -> Vec<String> {
        (0..count)
            .map(|index| {
                let mut cell = String::with_capacity(chars_per_cell + 32);
                cell.push_str("clipboard row ");
                cell.push_str(&index.to_string());
                cell.push(' ');
                while cell.len() < chars_per_cell {
                    cell.push_str("large preview text ");
                }
                cell
            })
            .collect()
    }

    fn truncate_cell_before_for_profile(text: &str, width: usize) -> String {
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
}
