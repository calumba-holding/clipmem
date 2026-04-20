use std::fmt::Write;
use std::io::IsTerminal;

use colored::Colorize;

use crate::db::{
    ImageOptimizationReport, OcrRunReport, OcrStatusReport, PurgeReport, SnapshotDeletionReport,
    StatsReport, StatsSnapshotLeaderboardEntry, StatsTimeBucketEntry, StorageCompactReport,
};
use crate::model::{DoctorReport, SnapshotDetails};

use super::commands::{
    CaptureOnceOutput, ExportOutput, RestoreOutput, SettingsIgnoreListOutput, SettingsView,
};
use super::output::{
    GetEnvelope, ListEnvelope, ListRow, RecallEnvelope, RecallMatchConfidence, StatsEnvelope,
};
use super::service::{ServiceProviderStatus, ServiceStatusReport};

const WIDTH: usize = 96;
const BAR_WIDTH: usize = 18;

#[derive(Debug, Clone, Copy)]
struct HumanTheme {
    color: bool,
}

impl HumanTheme {
    fn detect() -> Self {
        let force_color = std::env::var_os("CLICOLOR_FORCE").is_some();
        let no_color = std::env::var_os("NO_COLOR").is_some();
        Self {
            color: force_color || (!no_color && std::io::stdout().is_terminal()),
        }
    }

    fn title(self, text: &str) -> String {
        if self.color {
            text.bold().green().to_string()
        } else {
            text.to_string()
        }
    }

    fn section(self, text: &str) -> String {
        if self.color {
            text.bold().to_string()
        } else {
            text.to_string()
        }
    }

    fn id(self, text: impl ToString) -> String {
        let text = text.to_string();
        if self.color {
            text.bright_cyan().bold().to_string()
        } else {
            text
        }
    }

    fn score(self, value: f64, text: &str) -> String {
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

    fn warning(self, text: &str) -> String {
        if self.color {
            text.yellow().bold().to_string()
        } else {
            text.to_string()
        }
    }

    fn bar(self, value: usize, max: usize, width: usize) -> String {
        let bar = bar(value, max, width);
        if self.color {
            bar.cyan().to_string()
        } else {
            bar
        }
    }
}

pub(super) fn render_list_human(envelope: &ListEnvelope) -> String {
    let theme = HumanTheme::detect();
    let title = match envelope.command {
        "search" => "clipmem Search",
        "recent" => "clipmem Recent",
        "timeline" => "clipmem Timeline",
        other => return render_unknown_list_human(other, envelope),
    };
    let mut out = header(&theme, title);
    push_meta(&mut out, envelope);

    if envelope.results.is_empty() {
        out.push_str("No results.\n");
        return out;
    }

    if envelope.command == "timeline" {
        push_timeline_table(&mut out, &theme, &envelope.results);
    } else {
        push_snapshot_table(&mut out, &theme, &envelope.results);
    }

    out
}

pub(super) fn render_stats_human(envelope: &StatsEnvelope) -> String {
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

    push_kpi(&mut out, "Snapshots", format_count(stats.snapshot_count));
    push_kpi(
        &mut out,
        "Capture events",
        format_count(stats.capture_event_count),
    );
    push_kpi(
        &mut out,
        "Unique apps",
        format_count(stats.unique_app_count),
    );
    push_kpi(
        &mut out,
        "Total size",
        format_bytes(stats.total_bytes as u64),
    );
    push_kpi(
        &mut out,
        "Avg snapshot",
        format_bytes(stats.average_bytes_per_snapshot.round().max(0.0) as u64),
    );
    push_kpi(
        &mut out,
        "Avg captures",
        format!("{:.2}", stats.average_captures_per_snapshot),
    );
    push_kpi(&mut out, "Dedupe ratio", format_percent(stats.dedupe_ratio));
    push_kpi(
        &mut out,
        "Observed",
        format!(
            "{} to {}",
            stats
                .first_observed_at
                .as_deref()
                .map(format_timestamp_short)
                .unwrap_or_else(|| "none".to_string()),
            stats
                .last_observed_at
                .as_deref()
                .map(format_timestamp_short)
                .unwrap_or_else(|| "none".to_string())
        ),
    );
    push_kpi(
        &mut out,
        "Archive span",
        stats
            .archive_span_seconds
            .map(|seconds| format_duration_seconds(seconds.max(0) as u64))
            .unwrap_or_else(|| "0s".to_string()),
    );
    let dedupe_pct = (stats.dedupe_ratio * 100.0).clamp(0.0, 100.0);
    let dedupe_bar = theme.bar(dedupe_pct.round() as usize, 100, 24);
    let _ = writeln!(
        out,
        "Dedupe meter:     {} {}",
        dedupe_bar,
        theme.score(stats.dedupe_ratio, &format!("{dedupe_pct:.1}%"))
    );
    out.push('\n');

    push_content_mix(&mut out, &theme, stats);
    push_top_apps(&mut out, &theme, stats);
    push_activity(&mut out, stats);
    push_leaderboards(&mut out, &theme, stats);
    out
}

pub(super) fn render_recall_human(envelope: &RecallEnvelope) -> String {
    let theme = HumanTheme::detect();
    let best = &envelope.best_candidate;
    let mut out = header(&theme, "clipmem Recall");
    let _ = writeln!(
        out,
        "Query: {}  Filters: {}",
        envelope.query.as_deref().unwrap_or("(recent clipboard)"),
        render_filter_summary(&envelope.applied_filters)
    );
    out.push('\n');

    let _ = writeln!(out, "{}", theme.section("Best Match"));
    out.push_str(&separator(60, false));
    out.push('\n');
    if let Some(quoted_text) = &envelope.quoted_text {
        out.push_str(&truncate_multiline(quoted_text, 480));
        out.push('\n');
    } else if !best.best_text.trim().is_empty() {
        out.push_str(&truncate_multiline(&best.best_text, 480));
        out.push('\n');
    } else {
        out.push_str("No direct text was recovered for this clipboard item.\n");
    }
    out.push('\n');
    let score = envelope.best_match_score.unwrap_or(0.0);
    let _ = writeln!(
        out,
        "Provenance: snapshot {}  observed {}  app {}  confidence {}  score {}",
        theme.id(best.snapshot_id),
        format_timestamp_short(&best.observed_at),
        best.app_name
            .as_deref()
            .or(best.app_bundle_id.as_deref())
            .unwrap_or("unknown app"),
        render_confidence(&envelope.best_match_confidence),
        theme.score(score, &format!("{:.1}%", score * 100.0))
    );
    let _ = writeln!(out, "Why: {}", envelope.why_selected);
    if !best.urls.is_empty() {
        let _ = writeln!(out, "URLs: {}", truncate_cell(&best.urls.join(", "), 88));
    }
    if !best.file_paths.is_empty() {
        let _ = writeln!(
            out,
            "Files: {}",
            truncate_cell(&best.file_paths.join(", "), 88)
        );
    }

    if !envelope.alternatives.is_empty() {
        out.push('\n');
        let _ = writeln!(out, "{}", theme.section("Alternatives"));
        out.push_str(&separator(WIDTH, false));
        out.push('\n');
        let _ = writeln!(
            out,
            "{:>3}  {:>7}  {:<14}  {:<18}  {:>7}  Preview",
            "#", "ID", "Seen", "App", "Score"
        );
        out.push_str(&separator(WIDTH, false));
        out.push('\n');
        for (idx, row) in envelope.alternatives.iter().take(10).enumerate() {
            let score_text = render_score_cell(&theme, row.score);
            let app = row
                .app_name
                .as_deref()
                .or(row.app_bundle_id.as_deref())
                .unwrap_or("unknown");
            let _ = writeln!(
                out,
                "{:>2}.  {:>7}  {:<14}  {:<18}  {}  {}",
                idx + 1,
                theme.id(row.snapshot_id),
                format_timestamp_short(&row.observed_at),
                truncate_cell(app, 18),
                score_text,
                truncate_cell(&row.snippet, 36)
            );
        }
    }

    out
}

pub(super) fn render_get_human(envelope: &GetEnvelope) -> String {
    let theme = HumanTheme::detect();
    let snapshot = &envelope.snapshot;
    let mut out = header(&theme, "clipmem Snapshot");
    let _ = writeln!(
        out,
        "Generated {}  Filters: {}",
        format_timestamp_short(&envelope.generated_at),
        render_filter_summary(&envelope.applied_filters)
    );
    out.push('\n');

    push_kpi(&mut out, "Snapshot", theme.id(snapshot.snapshot_id()));
    push_kpi(
        &mut out,
        "Kind",
        snapshot.snapshot_kind().as_str().to_string(),
    );
    push_kpi(&mut out, "SHA-256", truncate_cell(snapshot.sha256(), 24));
    push_kpi(
        &mut out,
        "First seen",
        format_timestamp_short(snapshot.first_observed_at()),
    );
    push_kpi(
        &mut out,
        "Last seen",
        format_timestamp_short(snapshot.last_observed_at()),
    );
    push_kpi(&mut out, "Captures", format_count(snapshot.capture_count()));
    push_kpi(&mut out, "Items", format_count(snapshot.item_count()));
    push_kpi(
        &mut out,
        "Size",
        format_bytes(snapshot.total_bytes() as u64),
    );
    if let Some(app) = snapshot
        .last_frontmost_app_name()
        .or(snapshot.last_frontmost_app_bundle_id())
    {
        push_kpi(&mut out, "Last app", app.to_string());
    }
    out.push('\n');

    push_optional_text(&mut out, &theme, "Best Text", snapshot.best_text(), 360);
    if let Some(best_text_uti) = snapshot.best_text_uti() {
        push_kpi(&mut out, "Best text UTI", best_text_uti.to_string());
    }
    push_optional_text(
        &mut out,
        &theme,
        "Text Summary",
        snapshot.text_summary(),
        280,
    );
    if !snapshot.urls().is_empty() {
        push_optional_text(&mut out, &theme, "URLs", &snapshot.urls().join(", "), 280);
    }
    if !snapshot.file_paths().is_empty() {
        push_optional_text(
            &mut out,
            &theme,
            "Files",
            &snapshot.file_paths().join(", "),
            280,
        );
    }
    if let Some(ocr_text) = snapshot.ocr_text() {
        push_optional_text(&mut out, &theme, "OCR", ocr_text, 280);
    }
    if let Some(ocr_status) = snapshot.ocr_status() {
        push_kpi(&mut out, "OCR status", ocr_status.to_string());
    }

    push_snapshot_items(&mut out, &theme, snapshot);
    push_recent_events(&mut out, snapshot);
    out
}

pub(super) fn render_capture_once_human(output: &CaptureOnceOutput) -> String {
    let theme = HumanTheme::detect();
    let mut out = header(&theme, "clipmem Capture");
    match output {
        CaptureOnceOutput::Stored(output) => {
            let state = if output.store.inserted_new_snapshot() {
                "new content"
            } else {
                "already known content"
            };
            push_kpi(&mut out, "Status", "stored".to_string());
            push_kpi(&mut out, "Snapshot", theme.id(output.store.snapshot_id()));
            push_kpi(&mut out, "Event", theme.id(output.store.event_id()));
            push_kpi(&mut out, "State", state.to_string());
            push_kpi(
                &mut out,
                "Kind",
                output.snapshot.snapshot_kind().as_str().to_string(),
            );
            push_kpi(
                &mut out,
                "Size",
                format_bytes(output.snapshot.total_bytes() as u64),
            );
            if let Some(app) = output.snapshot.frontmost_app_name() {
                push_kpi(&mut out, "Frontmost app", app.to_string());
            }
            push_optional_text(
                &mut out,
                &theme,
                "Preview",
                output.snapshot.preview_text(),
                240,
            );
        }
        CaptureOnceOutput::Skipped(output) => {
            push_kpi(&mut out, "Status", theme.warning("skipped"));
            push_kpi(&mut out, "Reason", output.reason.as_str().to_string());
            push_kpi(&mut out, "Kind", output.kind.clone());
            push_kpi(&mut out, "Size", format_bytes(output.total_bytes as u64));
            if let Some(app) = &output.frontmost_app_name {
                push_kpi(&mut out, "Frontmost app", app.clone());
            }
        }
    }
    out
}

pub(super) fn render_restore_human(output: &RestoreOutput) -> String {
    let theme = HumanTheme::detect();
    let mut out = header(&theme, "clipmem Restore");
    push_kpi(&mut out, "Snapshot", theme.id(output.snapshot_id));
    push_kpi(&mut out, "Items", format_count(output.item_count));
    push_kpi(
        &mut out,
        "Representations",
        format_count(output.representation_count),
    );
    push_kpi(&mut out, "Size", format_bytes(output.total_bytes as u64));
    out
}

pub(super) fn render_export_human(output: &ExportOutput) -> String {
    let theme = HumanTheme::detect();
    let mut out = header(&theme, "clipmem Export");
    push_kpi(&mut out, "Snapshot", theme.id(output.snapshot_id));
    push_kpi(&mut out, "Item", output.item_index.to_string());
    push_kpi(&mut out, "UTI", output.uti.clone());
    push_kpi(&mut out, "Size", format_bytes(output.byte_count as u64));
    push_kpi(&mut out, "SHA-256", truncate_cell(&output.raw_sha256, 24));
    push_kpi(&mut out, "Output", output.out.clone());
    out
}

pub(super) fn render_forget_human(report: &SnapshotDeletionReport) -> String {
    let theme = HumanTheme::detect();
    let mut out = header(&theme, "clipmem Forget");
    push_kpi(&mut out, "Snapshot", theme.id(report.snapshot_id()));
    push_kpi(&mut out, "Items", format_count(report.item_count()));
    push_kpi(
        &mut out,
        "Representations",
        format_count(report.representation_count()),
    );
    push_kpi(
        &mut out,
        "Capture events",
        format_count(report.capture_event_count()),
    );
    push_kpi(
        &mut out,
        "Deleted size",
        format_bytes(report.total_bytes() as u64),
    );
    out
}

pub(super) fn render_purge_human(report: &PurgeReport) -> String {
    let theme = HumanTheme::detect();
    let mut out = header(
        &theme,
        if report.dry_run() {
            "clipmem Purge Preview"
        } else {
            "clipmem Purge"
        },
    );
    push_kpi(
        &mut out,
        "Older than",
        format_duration_seconds(report.older_than_seconds()),
    );
    push_kpi(&mut out, "Snapshots", format_count(report.snapshot_count()));
    push_kpi(&mut out, "Items", format_count(report.item_count()));
    push_kpi(
        &mut out,
        "Representations",
        format_count(report.representation_count()),
    );
    push_kpi(
        &mut out,
        "Capture events",
        format_count(report.capture_event_count()),
    );
    push_kpi(&mut out, "Size", format_bytes(report.total_bytes() as u64));
    out
}

pub(super) fn render_storage_compact_human(report: &StorageCompactReport) -> String {
    let theme = HumanTheme::detect();
    let mut out = header(
        &theme,
        if report.dry_run {
            "clipmem Storage Compact Preview"
        } else {
            "clipmem Storage Compact"
        },
    );
    push_kpi(&mut out, "Database", report.db_path.clone());
    push_kpi(&mut out, "Before", format_bytes(report.total_before_bytes));
    push_kpi(&mut out, "After", format_bytes(report.total_after_bytes));
    push_kpi(&mut out, "Reclaimed", format_bytes(report.reclaimed_bytes));
    push_kpi(
        &mut out,
        "Est. reclaimable",
        format_bytes(report.estimated_reclaimable_bytes),
    );
    push_kpi(&mut out, "Pages", format_count(report.page_count));
    push_kpi(&mut out, "Free pages", format_count(report.freelist_count));
    push_kpi(
        &mut out,
        "Checkpoint",
        format!(
            "busy={} log={} checkpointed={}",
            report.checkpoint.busy, report.checkpoint.log, report.checkpoint.checkpointed
        ),
    );
    push_kpi(&mut out, "Completed", report.completed.to_string());
    out
}

pub(super) fn render_image_optimization_human(report: &ImageOptimizationReport) -> String {
    let theme = HumanTheme::detect();
    let mut out = header(
        &theme,
        if report.dry_run {
            "clipmem Image Optimization Preview"
        } else {
            "clipmem Image Optimization"
        },
    );
    push_kpi(&mut out, "Format", report.format.to_string());
    push_kpi(&mut out, "Scanned", format_count(report.scanned_rows));
    push_kpi(&mut out, "Compressed", format_count(report.compressed_rows));
    push_kpi(&mut out, "Skipped", format_count(report.skipped_rows));
    push_kpi(&mut out, "Conflicts", format_count(report.conflict_count));
    push_kpi(
        &mut out,
        "Original size",
        format_bytes(report.original_bytes as u64),
    );
    push_kpi(
        &mut out,
        "Optimized size",
        format_bytes(report.optimized_bytes as u64),
    );
    push_kpi(
        &mut out,
        "Logical saved",
        format_bytes(report.logical_saved_bytes as u64),
    );
    push_kpi(&mut out, "Compaction run", report.compact_run.to_string());
    push_kpi(
        &mut out,
        "Filesystem saved",
        format_bytes(report.filesystem_saved_bytes),
    );
    if report.filesystem_growth_bytes > 0 {
        push_kpi(
            &mut out,
            "Filesystem growth",
            theme.warning(&format_bytes(report.filesystem_growth_bytes)),
        );
    }
    if let Some(error) = &report.compact_error {
        push_kpi(&mut out, "Compact error", theme.warning(error));
    }
    if report.compact_recommended {
        push_kpi(
            &mut out,
            "Next step",
            "Run `clipmem storage compact` to return freed pages.".to_string(),
        );
    }
    out
}

pub(super) fn render_settings_view_human(view: &SettingsView) -> String {
    let theme = HumanTheme::detect();
    let mut out = header(&theme, "clipmem Settings");
    push_kpi(&mut out, "Paused", view.paused.to_string());
    push_kpi(
        &mut out,
        "API key filter",
        view.api_key_filter_enabled.to_string(),
    );
    push_kpi(&mut out, "OCR", view.ocr_enabled.to_string());
    push_kpi(&mut out, "Retention", view.retention.clone());
    push_kpi(
        &mut out,
        "Ignored bundles",
        format_count(view.ignored_bundle_ids.len()),
    );
    push_ignored_bundle_table(&mut out, &view.ignored_bundle_ids);
    out
}

pub(super) fn render_settings_ignore_list_human(output: &SettingsIgnoreListOutput) -> String {
    let theme = HumanTheme::detect();
    let mut out = header(&theme, "clipmem Ignored Bundles");
    push_kpi(
        &mut out,
        "Ignored bundles",
        format_count(output.ignored_bundle_ids.len()),
    );
    push_ignored_bundle_table(&mut out, &output.ignored_bundle_ids);
    out
}

pub(super) fn render_ocr_status_human(report: &OcrStatusReport) -> String {
    let theme = HumanTheme::detect();
    let mut out = header(&theme, "clipmem OCR Status");
    push_kpi(&mut out, "Pending", format_count(report.pending()));
    push_kpi(&mut out, "Ready", format_count(report.ready()));
    push_kpi(&mut out, "Failed", format_count(report.failed()));
    push_kpi(&mut out, "Skipped", format_count(report.skipped()));
    push_kpi(
        &mut out,
        "Snapshots w/ OCR",
        format_count(report.snapshots_with_ocr_text()),
    );
    out
}

pub(super) fn render_ocr_run_human(report: &OcrRunReport) -> String {
    let theme = HumanTheme::detect();
    let mut out = header(&theme, "clipmem OCR Run");
    push_kpi(&mut out, "Processed", format_count(report.processed()));
    push_kpi(&mut out, "Ready", format_count(report.ready()));
    push_kpi(&mut out, "Failed", format_count(report.failed()));
    push_kpi(&mut out, "Skipped", format_count(report.skipped()));
    push_kpi(
        &mut out,
        "Remaining",
        format_count(report.remaining_pending()),
    );
    out
}

pub(super) fn render_service_status_human(report: &ServiceStatusReport) -> String {
    let theme = HumanTheme::detect();
    let mut out = header(&theme, "clipmem Service Status");
    push_kpi(&mut out, "Binary", report.binary_path.clone());
    push_kpi(&mut out, "Database", report.db_path.clone());
    push_kpi(
        &mut out,
        "Preferred",
        format!(
            "{} ({})",
            report.preferred_provider, report.preferred_provider_reason
        ),
    );
    push_kpi(&mut out, "Conflict", report.conflict.to_string());
    push_kpi(&mut out, "Database exists", report.db_exists.to_string());
    if let Some(size) = report.db_size_bytes {
        push_kpi(&mut out, "Database size", format_bytes(size));
    }
    if let Some(observed_at) = &report.recent_capture_at {
        push_kpi(
            &mut out,
            "Recent capture",
            format_timestamp_short(observed_at),
        );
    }
    if let Some(fresh) = report.recent_capture_within_last_hour {
        push_kpi(&mut out, "Fresh", fresh.to_string());
    }
    if let Some(paused) = report.paused {
        push_kpi(&mut out, "Paused", paused.to_string());
    }
    if let Some(retention) = report.retention.as_ref() {
        push_kpi(&mut out, "Retention", retention.clone());
    }
    if let Some(error) = &report.db_error {
        push_kpi(&mut out, "DB error", theme.warning(error));
    }
    out.push('\n');
    let _ = writeln!(out, "{}", theme.section("Providers"));
    out.push_str(&separator(WIDTH, false));
    out.push('\n');
    let _ = writeln!(
        out,
        "{:<13}  {:<16}  {:>9}  {:>7}  {:>7}  {:>8}  PID",
        "Provider", "State", "Installed", "Loaded", "Running", "Label"
    );
    out.push_str(&separator(WIDTH, false));
    out.push('\n');
    push_provider_row(&mut out, &report.homebrew);
    push_provider_row(&mut out, &report.launchagent);

    if !report.notes.is_empty() {
        out.push('\n');
        let _ = writeln!(out, "{}", theme.section("Notes"));
        for note in &report.notes {
            let _ = writeln!(out, "- {note}");
        }
    }

    out
}

pub(super) fn render_doctor_human(report: &DoctorReport) -> String {
    let theme = HumanTheme::detect();
    let mut out = header(&theme, "clipmem Doctor");
    push_kpi(&mut out, "Database", report.db_path().to_string());
    push_kpi(
        &mut out,
        "SQLite version",
        report.sqlite_version().to_string(),
    );
    push_kpi(&mut out, "Journal mode", report.journal_mode().to_string());
    push_kpi(
        &mut out,
        "FTS5 compiled",
        render_bool_status(report.fts5_compile_option_present()),
    );
    push_kpi(
        &mut out,
        "FTS5 works",
        render_bool_status(report.fts5_create_virtual_table_ok()),
    );
    if !report.compile_options().is_empty() {
        out.push('\n');
        let _ = writeln!(out, "{}", theme.section("Compile Options"));
        for option in report.compile_options().iter().take(24) {
            let _ = writeln!(out, "- {option}");
        }
        if report.compile_options().len() > 24 {
            let _ = writeln!(
                out,
                "... +{} more options",
                report.compile_options().len() - 24
            );
        }
    }
    out
}

fn render_unknown_list_human(command: &str, envelope: &ListEnvelope) -> String {
    let theme = HumanTheme::detect();
    let mut out = header(&theme, &format!("clipmem {command}"));
    push_meta(&mut out, envelope);
    out
}

fn header(theme: &HumanTheme, title: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", theme.title(title));
    out.push_str(&separator(WIDTH.min(72), true));
    out.push('\n');
    out
}

fn push_meta(out: &mut String, envelope: &ListEnvelope) {
    let _ = writeln!(
        out,
        "Generated {}  Filters: {}  Results: {}",
        format_timestamp_short(&envelope.generated_at),
        render_filter_summary(&envelope.applied_filters),
        envelope.results.len()
    );
    if envelope.truncated {
        let cursor = envelope
            .next_cursor
            .as_deref()
            .map(|cursor| truncate_cell(cursor, 36))
            .unwrap_or_else(|| "unavailable".to_string());
        let _ = writeln!(
            out,
            "More results available. Cursor: {cursor} (use `--format json` for the full token)"
        );
    }
    out.push('\n');
}

fn push_snapshot_table(out: &mut String, theme: &HumanTheme, rows: &[ListRow]) {
    let has_score = rows.iter().any(|row| match row {
        ListRow::Snapshot(row) => human_score(row.score).is_some(),
        ListRow::Timeline(_) => false,
    });
    out.push_str(&separator(WIDTH, false));
    out.push('\n');
    if has_score {
        let _ = writeln!(
            out,
            "{:>3}  {:>7}  {:<14}  {:<16}  {:<7}  {:>8}  {:>8}  {:>7}  Preview",
            "#", "ID", "Seen", "App", "Kind", "Size", "Captures", "Score"
        );
    } else {
        let _ = writeln!(
            out,
            "{:>3}  {:>7}  {:<14}  {:<16}  {:<7}  {:>8}  {:>8}  Preview",
            "#", "ID", "Seen", "App", "Kind", "Size", "Captures"
        );
    }
    out.push_str(&separator(WIDTH, false));
    out.push('\n');

    for (idx, row) in rows.iter().enumerate() {
        let ListRow::Snapshot(row) = row else {
            continue;
        };
        let app = row
            .app_name
            .as_deref()
            .or(row.app_bundle_id.as_deref())
            .unwrap_or("unknown");
        let text = first_non_empty(&[&row.best_text, &row.preview_text, &row.text_summary]);
        if has_score {
            let score_text = render_score_cell(theme, row.score);
            let _ = writeln!(
                out,
                "{:>2}.  {:>7}  {:<14}  {:<16}  {:<7}  {:>8}  {:>8}  {}  {}",
                idx + 1,
                theme.id(row.snapshot_id),
                format_timestamp_short(&row.observed_at),
                truncate_cell(app, 16),
                row.kind,
                format_bytes(row.total_bytes as u64),
                format_count(row.capture_count),
                score_text,
                truncate_cell(text, 24)
            );
        } else {
            let _ = writeln!(
                out,
                "{:>2}.  {:>7}  {:<14}  {:<16}  {:<7}  {:>8}  {:>8}  {}",
                idx + 1,
                theme.id(row.snapshot_id),
                format_timestamp_short(&row.observed_at),
                truncate_cell(app, 16),
                row.kind,
                format_bytes(row.total_bytes as u64),
                format_count(row.capture_count),
                truncate_cell(text, 34)
            );
        }
    }
}

fn push_timeline_table(out: &mut String, theme: &HumanTheme, rows: &[ListRow]) {
    out.push_str(&separator(WIDTH, false));
    out.push('\n');
    let _ = writeln!(
        out,
        "{:>3}  {:>7}  {:>8}  {:<14}  {:<16}  {:<7}  {:>8}  Preview",
        "#", "Event", "Snapshot", "Seen", "App", "Kind", "Size"
    );
    out.push_str(&separator(WIDTH, false));
    out.push('\n');
    for (idx, row) in rows.iter().enumerate() {
        let ListRow::Timeline(row) = row else {
            continue;
        };
        let app = row
            .app_name
            .as_deref()
            .or(row.app_bundle_id.as_deref())
            .unwrap_or("unknown");
        let text = first_non_empty(&[&row.best_text, &row.preview_text, &row.text_summary]);
        let _ = writeln!(
            out,
            "{:>2}.  {:>7}  {:>8}  {:<14}  {:<16}  {:<7}  {:>8}  {}",
            idx + 1,
            theme.id(row.event_id),
            row.snapshot_id,
            format_timestamp_short(&row.observed_at),
            truncate_cell(app, 16),
            row.kind,
            format_bytes(row.total_bytes as u64),
            truncate_cell(text, 28)
        );
    }
}

fn push_content_mix(out: &mut String, theme: &HumanTheme, stats: &StatsReport) {
    let _ = writeln!(out, "{}", theme.section("Content Mix"));
    if stats.kind_breakdown.is_empty() {
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
        .kind_breakdown
        .iter()
        .map(|entry| entry.snapshot_count)
        .max()
        .unwrap_or(1);
    for entry in &stats.kind_breakdown {
        let _ = writeln!(
            out,
            "{:<14}  {:>10}  {:>10}  {}",
            truncate_cell(&entry.kind, 14),
            format_count(entry.snapshot_count),
            format_bytes(entry.total_bytes as u64),
            theme.bar(entry.snapshot_count, max, BAR_WIDTH)
        );
    }
    out.push('\n');
}

fn push_top_apps(out: &mut String, theme: &HumanTheme, stats: &StatsReport) {
    let _ = writeln!(out, "{}", theme.section("Top Apps"));
    if stats.top_apps.is_empty() {
        out.push_str("none\n\n");
        return;
    }
    out.push_str(&separator(WIDTH, false));
    out.push('\n');
    let _ = writeln!(out, "{:<30}  {:>10}  Impact", "App", "Events");
    out.push_str(&separator(WIDTH, false));
    out.push('\n');
    let max = stats
        .top_apps
        .iter()
        .map(|entry| entry.capture_event_count)
        .max()
        .unwrap_or(1);
    for entry in &stats.top_apps {
        let _ = writeln!(
            out,
            "{:<30}  {:>10}  {}",
            truncate_cell(&entry.app, 30),
            format_count(entry.capture_event_count),
            theme.bar(entry.capture_event_count, max, BAR_WIDTH)
        );
    }
    out.push('\n');
}

fn push_activity(out: &mut String, stats: &StatsReport) {
    out.push_str("Activity\n");
    out.push_str(&separator(60, false));
    out.push('\n');
    push_kpi(
        out,
        "Busiest hour",
        peak_bucket(&stats.busiest_hours)
            .map(|entry| {
                format!(
                    "{}:00 UTC ({} events)",
                    entry.bucket, entry.capture_event_count
                )
            })
            .unwrap_or_else(|| "none".to_string()),
    );
    push_kpi(
        out,
        "Busiest weekday",
        peak_bucket(&stats.busiest_weekdays)
            .map(|entry| format!("{} ({} events)", entry.bucket, entry.capture_event_count))
            .unwrap_or_else(|| "none".to_string()),
    );
    out.push('\n');
}

fn push_leaderboards(out: &mut String, theme: &HumanTheme, stats: &StatsReport) {
    let _ = writeln!(out, "{}", theme.section("Leaderboards"));
    if let Some(snapshot) = &stats.most_recopied_snapshot {
        let _ = writeln!(out, "Most re-copied");
        push_leaderboard_row(out, theme, snapshot);
    } else {
        out.push_str("Most re-copied: none\n");
    }
    push_leaderboard(out, theme, "Largest snapshots", &stats.largest_snapshots);
    push_leaderboard(
        out,
        theme,
        "Most captured snapshots",
        &stats.most_captured_snapshots,
    );
}

fn push_leaderboard(
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

fn push_leaderboard_row(
    out: &mut String,
    theme: &HumanTheme,
    entry: &StatsSnapshotLeaderboardEntry,
) {
    let app = entry.app_name.as_deref().unwrap_or("unknown");
    let preview = if entry.preview_text.trim().is_empty() {
        "(no preview)"
    } else {
        entry.preview_text.trim()
    };
    let _ = writeln!(
        out,
        "  [{}] {:>5} captures  {:<7} {:>8}  {}  {}",
        theme.id(entry.snapshot_id),
        entry.capture_count,
        entry.kind,
        format_bytes(entry.total_bytes as u64),
        format_timestamp_short(&entry.last_observed_at),
        truncate_cell(&format!("{} - {}", app, preview.replace('\n', " ")), 42)
    );
}

fn push_snapshot_items(out: &mut String, theme: &HumanTheme, snapshot: &SnapshotDetails) {
    out.push('\n');
    let _ = writeln!(out, "{}", theme.section("Items"));
    if snapshot.items().is_empty() {
        out.push_str("No items.\n");
        return;
    }
    out.push_str(&separator(WIDTH, false));
    out.push('\n');
    let _ = writeln!(
        out,
        "{:>4}  {:<22}  {:<7}  {:>8}  {:<12}  Preview",
        "Item", "UTI", "Kind", "Size", "SHA"
    );
    out.push_str(&separator(WIDTH, false));
    out.push('\n');
    for item in snapshot.items() {
        if item.representations().is_empty() {
            let _ = writeln!(
                out,
                "{:>4}  {:<22}  {:<7}  {:>8}  {:<12}  {}",
                item.item_index(),
                item.primary_uti().unwrap_or("unknown"),
                item.primary_kind().as_str(),
                format_bytes(item.total_bytes() as u64),
                "",
                truncate_cell(item.preview_text(), 28)
            );
            continue;
        }
        for (idx, rep) in item.representations().iter().enumerate() {
            let preview = rep.text_value().unwrap_or_else(|| item.preview_text());
            let _ = writeln!(
                out,
                "{:>4}  {:<22}  {:<7}  {:>8}  {:<12}  {}",
                if idx == 0 {
                    item.item_index().to_string()
                } else {
                    String::new()
                },
                truncate_cell(rep.uti(), 22),
                rep.kind().as_str(),
                format_bytes(rep.byte_len() as u64),
                truncate_cell(rep.raw_sha256(), 12),
                truncate_cell(preview, 28)
            );
        }
    }
}

fn push_recent_events(out: &mut String, snapshot: &SnapshotDetails) {
    out.push('\n');
    out.push_str("Recent Events\n");
    if snapshot.recent_events().is_empty() {
        out.push_str("No recent events.\n");
        return;
    }
    out.push_str(&separator(WIDTH, false));
    out.push('\n');
    let _ = writeln!(
        out,
        "{:>7}  {:<14}  {:>12}  Source",
        "Event", "Observed", "Change"
    );
    out.push_str(&separator(WIDTH, false));
    out.push('\n');
    for event in snapshot.recent_events() {
        let source = event
            .frontmost_app_name()
            .or(event.frontmost_app_bundle_id())
            .unwrap_or("unknown");
        let _ = writeln!(
            out,
            "{:>7}  {:<14}  {:>12}  {}",
            event.event_id(),
            format_timestamp_short(event.observed_at()),
            event.change_count(),
            truncate_cell(source, 46)
        );
    }
}

fn push_ignored_bundle_table(out: &mut String, bundle_ids: &[String]) {
    if bundle_ids.is_empty() {
        return;
    }
    out.push('\n');
    out.push_str("Ignored Bundle IDs\n");
    out.push_str(&separator(60, false));
    out.push('\n');
    for (idx, bundle_id) in bundle_ids.iter().enumerate() {
        let _ = writeln!(out, "{:>2}. {}", idx + 1, bundle_id);
    }
}

fn push_provider_row(out: &mut String, provider: &ServiceProviderStatus) {
    let _ = writeln!(
        out,
        "{:<13}  {:<16}  {:>9}  {:>7}  {:>7}  {:<8}  {}",
        provider.provider.as_str(),
        format!("{:?}", provider.state).to_ascii_lowercase(),
        provider.installed,
        provider.loaded,
        provider.running,
        truncate_cell(&provider.label, 8),
        provider
            .pid
            .map(|pid| pid.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
}

fn push_optional_text(out: &mut String, theme: &HumanTheme, label: &str, text: &str, limit: usize) {
    if text.trim().is_empty() {
        return;
    }
    let _ = writeln!(out, "{}", theme.section(label));
    out.push_str(&truncate_multiline(text, limit));
    out.push('\n');
}

fn push_kpi(out: &mut String, label: &str, value: String) {
    let _ = writeln!(out, "{:<18} {}", format!("{label}:"), value);
}

fn render_filter_summary(value: &serde_json::Value) -> String {
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

fn render_confidence(confidence: &RecallMatchConfidence) -> &'static str {
    match confidence {
        RecallMatchConfidence::High => "high",
        RecallMatchConfidence::Medium => "medium",
        RecallMatchConfidence::Low => "low",
    }
}

fn render_bool_status(value: bool) -> String {
    if value {
        "ok".to_string()
    } else {
        "fail".to_string()
    }
}

fn render_score_cell(theme: &HumanTheme, score: Option<f64>) -> String {
    human_score(score)
        .map(|score| theme.score(score, &format!("{:>6.1}%", score * 100.0)))
        .unwrap_or_else(|| "      -".to_string())
}

fn human_score(score: Option<f64>) -> Option<f64> {
    let score = score?;
    score
        .is_finite()
        .then_some(score)
        .filter(|score| (0.0..=1.0).contains(score))
}

fn first_non_empty<'a>(values: &[&'a str]) -> &'a str {
    values
        .iter()
        .copied()
        .find(|value| !value.trim().is_empty())
        .unwrap_or("")
}

fn peak_bucket(buckets: &[StatsTimeBucketEntry]) -> Option<&StatsTimeBucketEntry> {
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

fn separator(width: usize, heavy: bool) -> String {
    if heavy {
        "═".repeat(width)
    } else {
        "─".repeat(width)
    }
}

fn bar(value: usize, max: usize, width: usize) -> String {
    if max == 0 || width == 0 {
        return String::new();
    }
    let filled = ((value as f64 / max as f64) * width as f64).round() as usize;
    let filled = filled.min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

fn format_count(value: usize) -> String {
    if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}K", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

fn format_bytes(value: u64) -> String {
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

fn format_percent(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

fn format_duration_seconds(seconds: u64) -> String {
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

fn format_timestamp_short(timestamp: &str) -> String {
    let trimmed = timestamp.trim();
    if trimmed.len() >= 16 && trimmed.as_bytes().get(10) == Some(&b'T') {
        trimmed[5..16].replace('T', " ")
    } else if trimmed.len() >= 16 && trimmed.as_bytes().get(10) == Some(&b' ') {
        trimmed[5..16].to_string()
    } else {
        truncate_cell(trimmed, 16)
    }
}

fn truncate_multiline(text: &str, limit: usize) -> String {
    truncate_cell(&text.replace('\n', " "), limit)
}

fn truncate_cell(text: &str, width: usize) -> String {
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
