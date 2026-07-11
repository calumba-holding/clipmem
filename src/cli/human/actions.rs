use std::fmt::Write;

use crate::cli::display::format_duration_seconds;
use crate::cli::human::{
    format_bytes, format_count, format_timestamp_short, header, push_ignored_bundle_table,
    push_kpi, push_optional_text, push_provider_row, render_bool_status, separator, truncate_cell,
    HumanTheme, WIDTH,
};
use crate::cli::output::{
    CaptureOnceOutput, ExportOutput, RestoreOutput, SettingsIgnoreListOutput, SettingsView,
};
use crate::cli::service::ServiceStatusReport;
use crate::db::{
    ImageOptimizationReport, OcrRunReport, OcrStatusReport, PurgeReport, SnapshotDeletionReport,
    StorageCompactReport,
};
use crate::model::DoctorReport;

pub(in crate::cli) fn render_capture_once_human(output: &CaptureOnceOutput) -> String {
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

pub(in crate::cli) fn render_restore_human(output: &RestoreOutput) -> String {
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

pub(in crate::cli) fn render_export_human(output: &ExportOutput) -> String {
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

pub(in crate::cli) fn render_forget_human(report: &SnapshotDeletionReport) -> String {
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

pub(in crate::cli) fn render_purge_human(report: &PurgeReport) -> String {
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

pub(in crate::cli) fn render_storage_compact_human(report: &StorageCompactReport) -> String {
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

pub(in crate::cli) fn render_image_optimization_human(report: &ImageOptimizationReport) -> String {
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

pub(in crate::cli) fn render_settings_view_human(view: &SettingsView) -> String {
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

pub(in crate::cli) fn render_settings_ignore_list_human(
    output: &SettingsIgnoreListOutput,
) -> String {
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

pub(in crate::cli) fn render_ocr_status_human(report: &OcrStatusReport) -> String {
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

pub(in crate::cli) fn render_ocr_run_human(report: &OcrRunReport) -> String {
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

pub(in crate::cli) fn render_service_status_human(report: &ServiceStatusReport) -> String {
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

pub(in crate::cli) fn render_doctor_human(report: &DoctorReport) -> String {
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
    if let Some(integrity) = report.integrity() {
        push_kpi(
            &mut out,
            "Orphan representations",
            integrity.orphan_representation_count().to_string(),
        );
        push_kpi(
            &mut out,
            "Foreign key violations",
            integrity.foreign_key_violation_count().to_string(),
        );
        push_kpi(
            &mut out,
            "Projection mismatches",
            integrity.projection_mismatch_count().to_string(),
        );
    } else {
        push_kpi(&mut out, "Integrity checks", "Skipped".to_string());
    }
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
