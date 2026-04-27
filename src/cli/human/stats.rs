use super::*;

pub(in crate::cli) fn render_stats_human(envelope: &StatsEnvelope) -> String {
    let theme = HumanTheme::detect();
    let stats = &envelope.stats;
    let mut out = header(&theme, "clipmem Archive Stats");
    let _ = writeln!(
        out,
        "Generated {}  Filters: {}",
        format_timestamp_short(&envelope.generated_at),
        render_filter_summary(&envelope.applied_filters)
    );
    out.push('\n');

    push_kpi(&mut out, "Snapshots", format_count(stats.snapshot_count()));
    push_kpi(
        &mut out,
        "Capture events",
        format_count(stats.capture_event_count()),
    );
    push_kpi(
        &mut out,
        "Unique apps",
        format_count(stats.unique_app_count()),
    );
    push_kpi(
        &mut out,
        "Total size",
        format_bytes(stats.total_bytes() as u64),
    );
    push_kpi(
        &mut out,
        "Avg snapshot",
        format_bytes(stats.average_bytes_per_snapshot().round().max(0.0) as u64),
    );
    push_kpi(
        &mut out,
        "Avg captures",
        format!("{:.2}", stats.average_captures_per_snapshot()),
    );
    push_kpi(
        &mut out,
        "Dedupe ratio",
        format_percent(stats.dedupe_ratio()),
    );
    push_kpi(
        &mut out,
        "Observed",
        format!(
            "{} to {}",
            stats
                .first_observed_at()
                .map(format_timestamp_short)
                .unwrap_or_else(|| "none".to_string()),
            stats
                .last_observed_at()
                .map(format_timestamp_short)
                .unwrap_or_else(|| "none".to_string())
        ),
    );
    push_kpi(
        &mut out,
        "Archive span",
        stats
            .archive_span_seconds()
            .map(|seconds| format_duration_seconds(seconds.max(0) as u64))
            .unwrap_or_else(|| "0s".to_string()),
    );
    let dedupe_pct = (stats.dedupe_ratio() * 100.0).clamp(0.0, 100.0);
    let dedupe_bar = theme.bar(dedupe_pct.round() as usize, 100, 24);
    let _ = writeln!(
        out,
        "Dedupe meter:     {} {}",
        dedupe_bar,
        theme.score(stats.dedupe_ratio(), &format!("{dedupe_pct:.1}%"))
    );
    out.push('\n');

    push_content_mix(&mut out, &theme, stats);
    push_top_apps(&mut out, &theme, stats);
    push_activity(&mut out, stats);
    push_leaderboards(&mut out, &theme, stats);
    out
}

pub(in crate::cli) fn push_content_mix(out: &mut String, theme: &HumanTheme, stats: &StatsReport) {
    let _ = writeln!(out, "{}", theme.section("Content Mix"));
    if stats.kind_breakdown().is_empty() {
        out.push_str("none\n\n");
        return;
    }
    out.push_str(&separator(WIDTH, false));
    out.push('\n');
    let _ = writeln!(
        out,
        "{:<14}  {:>10}  {:>10}  Impact",
        "Kind", "Snapshots", "Size"
    );
    out.push_str(&separator(WIDTH, false));
    out.push('\n');
    let max = stats
        .kind_breakdown()
        .iter()
        .map(|entry| entry.snapshot_count())
        .max()
        .unwrap_or(1);
    for entry in stats.kind_breakdown() {
        let _ = writeln!(
            out,
            "{:<14}  {:>10}  {:>10}  {}",
            truncate_cell(entry.kind(), 14),
            format_count(entry.snapshot_count()),
            format_bytes(entry.total_bytes() as u64),
            theme.bar(entry.snapshot_count(), max, BAR_WIDTH)
        );
    }
    out.push('\n');
}

pub(in crate::cli) fn push_top_apps(out: &mut String, theme: &HumanTheme, stats: &StatsReport) {
    let _ = writeln!(out, "{}", theme.section("Top Apps"));
    if stats.top_apps().is_empty() {
        out.push_str("none\n\n");
        return;
    }
    out.push_str(&separator(WIDTH, false));
    out.push('\n');
    let _ = writeln!(out, "{:<30}  {:>10}  Impact", "App", "Events");
    out.push_str(&separator(WIDTH, false));
    out.push('\n');
    let max = stats
        .top_apps()
        .iter()
        .map(|entry| entry.capture_event_count())
        .max()
        .unwrap_or(1);
    for entry in stats.top_apps() {
        let _ = writeln!(
            out,
            "{:<30}  {:>10}  {}",
            truncate_cell(entry.app(), 30),
            format_count(entry.capture_event_count()),
            theme.bar(entry.capture_event_count(), max, BAR_WIDTH)
        );
    }
    out.push('\n');
}

pub(in crate::cli) fn push_activity(out: &mut String, stats: &StatsReport) {
    out.push_str("Activity\n");
    out.push_str(&separator(60, false));
    out.push('\n');
    push_kpi(
        out,
        "Busiest hour",
        peak_bucket(stats.busiest_hours())
            .map(|entry| {
                format!(
                    "{}:00 UTC ({} events)",
                    entry.bucket(),
                    entry.capture_event_count()
                )
            })
            .unwrap_or_else(|| "none".to_string()),
    );
    push_kpi(
        out,
        "Busiest weekday",
        peak_bucket(stats.busiest_weekdays())
            .map(|entry| {
                format!(
                    "{} ({} events)",
                    entry.bucket(),
                    entry.capture_event_count()
                )
            })
            .unwrap_or_else(|| "none".to_string()),
    );
    out.push('\n');
}

pub(in crate::cli) fn push_leaderboards(out: &mut String, theme: &HumanTheme, stats: &StatsReport) {
    let _ = writeln!(out, "{}", theme.section("Leaderboards"));
    if let Some(snapshot) = stats.most_recopied_snapshot() {
        let _ = writeln!(out, "Most re-copied");
        push_leaderboard_row(out, theme, snapshot);
    } else {
        out.push_str("Most re-copied: none\n");
    }
    push_leaderboard(out, theme, "Largest snapshots", stats.largest_snapshots());
    push_leaderboard(
        out,
        theme,
        "Most captured snapshots",
        stats.most_captured_snapshots(),
    );
}

pub(in crate::cli) fn push_leaderboard(
    out: &mut String,
    theme: &HumanTheme,
    title: &str,
    entries: &[StatsSnapshotLeaderboardEntry],
) {
    let _ = writeln!(out);
    let _ = writeln!(out, "{title}");
    if entries.is_empty() {
        out.push_str("none\n");
        return;
    }
    for entry in entries {
        push_leaderboard_row(out, theme, entry);
    }
}

pub(in crate::cli) fn push_leaderboard_row(
    out: &mut String,
    theme: &HumanTheme,
    entry: &StatsSnapshotLeaderboardEntry,
) {
    let app = entry.app_name().unwrap_or("unknown");
    let preview = if entry.preview_text().trim().is_empty() {
        "(no preview)"
    } else {
        entry.preview_text().trim()
    };
    let _ = writeln!(
        out,
        "  [{}] {:>5} captures  {:<7} {:>8}  {}  {}",
        theme.id(entry.snapshot_id()),
        entry.capture_count(),
        entry.kind(),
        format_bytes(entry.total_bytes() as u64),
        format_timestamp_short(entry.last_observed_at()),
        truncate_cell(&format!("{} - {}", app, preview.replace('\n', " ")), 42)
    );
}
