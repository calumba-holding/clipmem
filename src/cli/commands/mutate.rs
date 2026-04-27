use std::fs::{File, OpenOptions};
use std::path::Path;

use anyhow::{anyhow, Context, Result};

use crate::db::{
    CapturePolicy, CaptureSettings, ImageOptimizationReport, OcrRunReport, OcrStatusReport,
    PurgeReport, SnapshotDeletionReport, StorageCompactReport,
};
use crate::platform::restore_items;

use crate::cli::commands::retrieval::normalize_retrieval_filters;
use crate::cli::commands::runtime::{open_existing_db, open_or_init_db};
use crate::cli::commands::types::{
    ExportOutput, RestoreOutput, SettingsIgnoreListOutput, SettingsView,
};
use crate::cli::formats::{format_duration_compact, OutputFormat, ProgressFormat};
use crate::cli::human::{
    render_doctor_human, render_export_human, render_forget_human, render_image_optimization_human,
    render_ocr_run_human, render_ocr_status_human, render_purge_human, render_restore_human,
    render_settings_ignore_list_human, render_settings_view_human, render_storage_compact_human,
};
use crate::cli::output::{
    emit_json_or_text, print_json_line, render_doctor_text, UnsupportedFormatError,
};
use crate::cli::schema::{
    DoctorArgs, ExportArgs, ForgetArgs, OcrArgs, OcrCommand, OcrRunArgs, OcrStatusArgs, PurgeArgs,
    RestoreArgs, SettingsApiKeyFilterArgs, SettingsArgs, SettingsCommand, SettingsIgnoreArgs,
    SettingsIgnoreCommand, SettingsIgnoreListArgs, SettingsOcrArgs, SettingsPauseArgs,
    SettingsRetentionArgs, SettingsShowArgs, StorageArgs, StorageCommand, StorageCompactArgs,
    StorageOptimizeImagesArgs,
};

pub(in crate::cli) fn export_snapshot_bytes(db_path: &Path, args: &ExportArgs) -> Result<()> {
    let format = require_text_or_json(args.output.resolved()?, "export")?;
    let filters = normalize_retrieval_filters(&args.filters)?;
    let db = open_existing_db(db_path)?;
    anyhow::Context::with_context(db.find_snapshot(args.snapshot_id, 1), || {
        format!("export failed for snapshot {}", args.snapshot_id)
    })?
    .ok_or_else(|| anyhow!("snapshot {} was not found", args.snapshot_id))?;
    if !db.snapshot_matches_filters(args.snapshot_id, &filters)? {
        return Err(anyhow!(
            "snapshot {} does not satisfy the active filters",
            args.snapshot_id
        ));
    }
    let representation = db
        .find_representation_bytes(args.snapshot_id, args.item, &args.uti)?
        .ok_or_else(|| {
            anyhow!(
                "representation not found for snapshot {} item {} uti {}",
                args.snapshot_id,
                args.item,
                args.uti
            )
        })?;

    if let Some(parent) = args.out.parent() {
        anyhow::Context::with_context(std::fs::create_dir_all(parent), || {
            format!("failed to create {}", parent.display())
        })?;
    }

    let mut output = create_export_destination(&args.out, args.force)?;
    use std::io::Write as _;
    anyhow::Context::with_context(output.write_all(representation.raw_bytes()), || {
        format!("failed to write export file {}", args.out.display())
    })?;

    let output = ExportOutput {
        snapshot_id: args.snapshot_id,
        item_index: args.item,
        uti: args.uti.clone(),
        byte_count: representation.byte_len(),
        raw_sha256: representation.raw_sha256().to_string(),
        out: args.out.display().to_string(),
    };
    match format {
        OutputFormat::Json => emit_json_or_text(true, &output, render_export_text)?,
        OutputFormat::Human => print!("{}", render_export_human(&output)),
        OutputFormat::Text => print!("{}", render_export_text(&output)),
        _ => unreachable!("unsupported export format should be rejected earlier"),
    }

    Ok(())
}

pub(in crate::cli) fn restore_snapshot(db_path: &Path, args: &RestoreArgs) -> Result<()> {
    let format = require_text_or_json(args.output.resolved()?, "restore")?;
    let db = open_existing_db(db_path)?;
    let snapshot = anyhow::Context::with_context(db.find_snapshot(args.snapshot_id, 1), || {
        format!("restore failed for snapshot {}", args.snapshot_id)
    })?
    .ok_or_else(|| anyhow!("snapshot {} was not found", args.snapshot_id))?;
    db.register_pending_restore(snapshot.sha256())
        .context("register pending restore")?;
    let report = match restore_items(snapshot.items()) {
        Ok(report) => report,
        Err(err) => {
            if let Err(clear_err) = db.clear_pending_restore(snapshot.sha256()) {
                eprintln!("pending restore cleanup failed: {clear_err:#}");
            }
            return Err(err).context("restore failed");
        }
    };
    let output = RestoreOutput {
        snapshot_id: args.snapshot_id,
        item_count: report.item_count(),
        representation_count: report.representation_count(),
        total_bytes: report.total_bytes(),
    };
    match format {
        OutputFormat::Json => emit_json_or_text(true, &output, render_restore_text)?,
        OutputFormat::Human => print!("{}", render_restore_human(&output)),
        OutputFormat::Text => print!("{}", render_restore_text(&output)),
        _ => unreachable!("unsupported restore format should be rejected earlier"),
    }

    Ok(())
}

pub(in crate::cli) fn forget_snapshot(db_path: &Path, args: &ForgetArgs) -> Result<()> {
    let format = require_text_or_json(args.output.resolved()?, "forget")?;
    let mut db = open_existing_db(db_path)?;
    let report = anyhow::Context::with_context(db.forget_snapshot(args.snapshot_id), || {
        format!("forget failed for snapshot {}", args.snapshot_id)
    })?
    .ok_or_else(|| anyhow!("snapshot {} was not found", args.snapshot_id))?;
    match format {
        OutputFormat::Json => emit_json_or_text(true, &report, render_forget_text)?,
        OutputFormat::Human => print!("{}", render_forget_human(&report)),
        OutputFormat::Text => print!("{}", render_forget_text(&report)),
        _ => unreachable!("unsupported forget format should be rejected earlier"),
    }

    Ok(())
}

pub(in crate::cli) fn purge_snapshots(db_path: &Path, args: &PurgeArgs) -> Result<()> {
    let format = require_text_or_json(args.output.resolved()?, "purge")?;
    let mut db = open_existing_db(db_path)?;
    let report = anyhow::Context::with_context(
        db.purge_snapshots_older_than(args.older_than.seconds(), args.dry_run),
        || format!("purge failed for duration {}", args.older_than.raw()),
    )?;
    match format {
        OutputFormat::Json => emit_json_or_text(true, &report, render_purge_text)?,
        OutputFormat::Human => print!("{}", render_purge_human(&report)),
        OutputFormat::Text => print!("{}", render_purge_text(&report)),
        _ => unreachable!("unsupported purge format should be rejected earlier"),
    }

    Ok(())
}

pub(in crate::cli) fn storage(db_path: &Path, args: &StorageArgs) -> Result<()> {
    match &args.command {
        StorageCommand::Compact(args) => storage_compact(db_path, args),
        StorageCommand::OptimizeImages(args) => storage_optimize_images(db_path, args),
    }
}

pub(in crate::cli) fn storage_compact(db_path: &Path, args: &StorageCompactArgs) -> Result<()> {
    let format = require_text_or_json(args.output.resolved()?, "storage compact")?;
    let mut db = open_existing_db(db_path)?;
    let report = db.compact_storage(args.dry_run)?;
    match format {
        OutputFormat::Json => emit_json_or_text(true, &report, render_storage_compact_text)?,
        OutputFormat::Human => print!("{}", render_storage_compact_human(&report)),
        OutputFormat::Text => print!("{}", render_storage_compact_text(&report)),
        _ => unreachable!("unsupported storage compact format should be rejected earlier"),
    }
    Ok(())
}

pub(in crate::cli) fn storage_optimize_images(
    db_path: &Path,
    args: &StorageOptimizeImagesArgs,
) -> Result<()> {
    if matches!(args.progress, Some(ProgressFormat::Jsonl)) {
        let mut db = open_existing_db(db_path)?;
        db.optimize_images_with_progress(args.dry_run, args.limit, !args.no_compact, |event| {
            print_json_line(&event)
        })?;
        return Ok(());
    }

    let format = require_text_or_json(args.output.resolved()?, "storage optimize-images")?;
    let mut db = open_existing_db(db_path)?;
    let report = db.optimize_images(args.dry_run, args.limit, !args.no_compact)?;
    match format {
        OutputFormat::Json => emit_json_or_text(true, &report, render_image_optimization_text)?,
        OutputFormat::Human => print!("{}", render_image_optimization_human(&report)),
        OutputFormat::Text => print!("{}", render_image_optimization_text(&report)),
        _ => unreachable!("unsupported storage optimize-images format should be rejected earlier"),
    }
    Ok(())
}

pub(in crate::cli) fn settings(db_path: &Path, args: &SettingsArgs) -> Result<()> {
    match &args.command {
        SettingsCommand::Show(args) => settings_show(db_path, args),
        SettingsCommand::Pause(args) => settings_pause(db_path, args),
        SettingsCommand::ApiKeyFilter(args) => settings_api_key_filter(db_path, args),
        SettingsCommand::Ocr(args) => settings_ocr(db_path, args),
        SettingsCommand::Retention(args) => settings_retention(db_path, args),
        SettingsCommand::Ignore(args) => settings_ignore(db_path, args),
    }
}

pub(in crate::cli) fn ocr(db_path: &Path, args: &OcrArgs) -> Result<()> {
    match &args.command {
        OcrCommand::Status(args) => ocr_status(db_path, args),
        OcrCommand::Run(args) => ocr_run(db_path, args),
    }
}

pub(in crate::cli) fn ocr_status(db_path: &Path, args: &OcrStatusArgs) -> Result<()> {
    let format = require_text_or_json(args.output.resolved()?, "ocr status")?;
    let db = open_or_init_db(db_path)?;
    let report = db.ocr_status_report()?;
    match format {
        OutputFormat::Json => emit_json_or_text(true, &report, render_ocr_status_text)?,
        OutputFormat::Human => print!("{}", render_ocr_status_human(&report)),
        OutputFormat::Text => print!("{}", render_ocr_status_text(&report)),
        _ => unreachable!("unsupported ocr status format should be rejected earlier"),
    }
    Ok(())
}

pub(in crate::cli) fn ocr_run(db_path: &Path, args: &OcrRunArgs) -> Result<()> {
    let format = require_text_or_json(args.output.resolved()?, "ocr run")?;
    let mut db = open_or_init_db(db_path)?;
    let engine = crate::ocr::default_engine();
    let report = crate::ocr::run_ocr_jobs(
        &mut db,
        &engine,
        args.limit,
        args.snapshot,
        args.retry_failed,
    )?;
    match format {
        OutputFormat::Json => emit_json_or_text(true, &report, render_ocr_run_text)?,
        OutputFormat::Human => print!("{}", render_ocr_run_human(&report)),
        OutputFormat::Text => print!("{}", render_ocr_run_text(&report)),
        _ => unreachable!("unsupported ocr run format should be rejected earlier"),
    }
    Ok(())
}

pub(in crate::cli) fn settings_show(db_path: &Path, args: &SettingsShowArgs) -> Result<()> {
    let format = require_text_or_json(args.output.resolved()?, "settings show")?;
    let db = open_or_init_db(db_path)?;
    let view = settings_view(db.capture_policy()?);
    match format {
        OutputFormat::Json => emit_json_or_text(true, &view, render_settings_view_text)?,
        OutputFormat::Human => print!("{}", render_settings_view_human(&view)),
        OutputFormat::Text => print!("{}", render_settings_view_text(&view)),
        _ => unreachable!("unsupported settings show format should be rejected earlier"),
    }
    Ok(())
}

pub(in crate::cli) fn settings_pause(db_path: &Path, args: &SettingsPauseArgs) -> Result<()> {
    let db = open_or_init_db(db_path)?;
    let settings = db.set_paused(args.state.is_paused())?;
    let view = settings_view(CapturePolicy::new(settings, db.list_ignored_bundle_ids()?));
    emit_json_or_text(false, &view, render_settings_view_text)?;
    Ok(())
}

pub(in crate::cli) fn settings_api_key_filter(
    db_path: &Path,
    args: &SettingsApiKeyFilterArgs,
) -> Result<()> {
    let db = open_or_init_db(db_path)?;
    let settings = db.set_api_key_filter_enabled(args.state.is_paused())?;
    let view = settings_view(CapturePolicy::new(settings, db.list_ignored_bundle_ids()?));
    emit_json_or_text(false, &view, render_settings_view_text)?;
    Ok(())
}

pub(in crate::cli) fn settings_ocr(db_path: &Path, args: &SettingsOcrArgs) -> Result<()> {
    let db = open_or_init_db(db_path)?;
    let settings = db.set_ocr_enabled(args.state.is_paused())?;
    let view = settings_view(CapturePolicy::new(settings, db.list_ignored_bundle_ids()?));
    emit_json_or_text(false, &view, render_settings_view_text)?;
    Ok(())
}

pub(in crate::cli) fn settings_retention(
    db_path: &Path,
    args: &SettingsRetentionArgs,
) -> Result<()> {
    let db = open_or_init_db(db_path)?;
    let settings = db.set_retention_seconds(args.value.retention_seconds())?;
    let view = settings_view(CapturePolicy::new(settings, db.list_ignored_bundle_ids()?));
    emit_json_or_text(false, &view, render_settings_view_text)?;
    Ok(())
}

pub(in crate::cli) fn settings_ignore(db_path: &Path, args: &SettingsIgnoreArgs) -> Result<()> {
    match &args.command {
        SettingsIgnoreCommand::Add(args) => settings_ignore_add(db_path, &args.bundle_id),
        SettingsIgnoreCommand::Remove(args) => settings_ignore_remove(db_path, &args.bundle_id),
        SettingsIgnoreCommand::List(args) => settings_ignore_list(db_path, args),
    }
}

pub(in crate::cli) fn settings_ignore_add(db_path: &Path, bundle_id: &str) -> Result<()> {
    let db = open_or_init_db(db_path)?;
    db.add_ignored_bundle_id(bundle_id)?;
    let output = SettingsIgnoreListOutput {
        ignored_bundle_ids: db.list_ignored_bundle_ids()?,
    };
    emit_json_or_text(false, &output, render_settings_ignore_list_text)?;
    Ok(())
}

pub(in crate::cli) fn settings_ignore_remove(db_path: &Path, bundle_id: &str) -> Result<()> {
    let db = open_or_init_db(db_path)?;
    db.remove_ignored_bundle_id(bundle_id)?;
    let output = SettingsIgnoreListOutput {
        ignored_bundle_ids: db.list_ignored_bundle_ids()?,
    };
    emit_json_or_text(false, &output, render_settings_ignore_list_text)?;
    Ok(())
}

pub(in crate::cli) fn settings_ignore_list(
    db_path: &Path,
    args: &SettingsIgnoreListArgs,
) -> Result<()> {
    let format = require_text_or_json(args.output.resolved()?, "settings ignore list")?;
    let db = open_or_init_db(db_path)?;
    let output = SettingsIgnoreListOutput {
        ignored_bundle_ids: db.list_ignored_bundle_ids()?,
    };
    match format {
        OutputFormat::Json => emit_json_or_text(true, &output, render_settings_ignore_list_text)?,
        OutputFormat::Human => print!("{}", render_settings_ignore_list_human(&output)),
        OutputFormat::Text => print!("{}", render_settings_ignore_list_text(&output)),
        _ => unreachable!("unsupported settings ignore list format should be rejected earlier"),
    }
    Ok(())
}

pub(in crate::cli) fn create_export_destination(path: &Path, force: bool) -> Result<File> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            let file_type = metadata.file_type();
            if file_type.is_symlink() {
                return Err(anyhow!(
                    "export destination {} is a symbolic link; choose a regular file path instead",
                    path.display()
                ));
            }
            if !metadata.is_file() {
                return Err(anyhow!(
                    "export destination {} is not a regular file",
                    path.display()
                ));
            }
            if !force {
                return Err(anyhow!(
                    "export destination {} already exists (pass --force to replace it)",
                    path.display()
                ));
            }

            std::fs::remove_file(path)
                .with_context(|| format!("failed to replace {}", path.display()))?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
        }
    }

    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("failed to create export file {}", path.display()))
}

pub(in crate::cli) fn doctor(db_path: &Path, args: &DoctorArgs) -> Result<()> {
    let db = open_existing_db(db_path)?;
    let report = anyhow::Context::context(db.doctor(), "doctor diagnostics failed")?;
    if args.human {
        print!("{}", render_doctor_human(&report));
    } else {
        emit_json_or_text(args.json, &report, render_doctor_text)?;
    }

    Ok(())
}

pub(in crate::cli) fn require_text_or_json(
    format: OutputFormat,
    command_name: &str,
) -> Result<OutputFormat> {
    match format {
        OutputFormat::Text | OutputFormat::Json | OutputFormat::Human => Ok(format),
        other => Err(UnsupportedFormatError::new(format!(
            "{command_name} only supports `text`, `json`, and `human` output, got `{}`",
            other.as_str()
        ))
        .into()),
    }
}

pub(in crate::cli) fn settings_view(policy: CapturePolicy) -> SettingsView {
    SettingsView {
        paused: policy.settings().paused(),
        api_key_filter_enabled: policy.settings().api_key_filter_enabled(),
        ocr_enabled: policy.settings().ocr_enabled(),
        retention_seconds: policy.settings().retention_seconds(),
        retention: render_retention_value(policy.settings()),
        ignored_bundle_ids: policy.ignored_bundle_ids().to_vec(),
    }
}

pub(in crate::cli) fn render_ocr_status_text(report: &OcrStatusReport) -> String {
    format!(
        "ocr pending={} ready={} failed={} skipped={} snapshots_with_text={}\n",
        report.pending(),
        report.ready(),
        report.failed(),
        report.skipped(),
        report.snapshots_with_ocr_text()
    )
}

pub(in crate::cli) fn render_ocr_run_text(report: &OcrRunReport) -> String {
    format!(
        "ocr processed={} ready={} failed={} skipped={} remaining_pending={}\n",
        report.processed(),
        report.ready(),
        report.failed(),
        report.skipped(),
        report.remaining_pending()
    )
}

pub(in crate::cli) fn render_restore_text(output: &RestoreOutput) -> String {
    format!(
        "restored snapshot={} items={} representations={} bytes={}\n",
        output.snapshot_id, output.item_count, output.representation_count, output.total_bytes
    )
}

pub(in crate::cli) fn render_export_text(output: &ExportOutput) -> String {
    format!(
        "exported snapshot={} item={} uti={} bytes={} sha256={} out={}\n",
        output.snapshot_id,
        output.item_index,
        output.uti,
        output.byte_count,
        output.raw_sha256,
        output.out
    )
}

pub(in crate::cli) fn render_forget_text(report: &SnapshotDeletionReport) -> String {
    format!(
        "forgot snapshot={} items={} representations={} capture_events={} bytes={}\n",
        report.snapshot_id(),
        report.item_count(),
        report.representation_count(),
        report.capture_event_count(),
        report.total_bytes()
    )
}

pub(in crate::cli) fn render_purge_text(report: &PurgeReport) -> String {
    let action = if report.dry_run() {
        "purge dry-run"
    } else {
        "purged"
    };
    format!(
        "{} older_than={} snapshots={} items={} representations={} capture_events={} bytes={}\n",
        action,
        format_duration_compact(report.older_than_seconds()),
        report.snapshot_count(),
        report.item_count(),
        report.representation_count(),
        report.capture_event_count(),
        report.total_bytes()
    )
}

pub(in crate::cli) fn render_storage_compact_text(report: &StorageCompactReport) -> String {
    let action = if report.dry_run {
        "storage compact dry-run"
    } else {
        "storage compacted"
    };
    format!(
        "{} db={} before={} after={} reclaimed={} estimated_reclaimable={} page_count={} freelist_count={} checkpoint_busy={} checkpoint_log={} checkpointed={}\n",
        action,
        report.db_path,
        report.total_before_bytes,
        report.total_after_bytes,
        report.reclaimed_bytes,
        report.estimated_reclaimable_bytes,
        report.page_count,
        report.freelist_count,
        report.checkpoint.busy,
        report.checkpoint.log,
        report.checkpoint.checkpointed
    )
}

pub(in crate::cli) fn render_image_optimization_text(report: &ImageOptimizationReport) -> String {
    let action = if report.dry_run {
        "image optimization dry-run"
    } else if report.compact_error.is_some() {
        "image optimization complete; database compaction failed"
    } else {
        "image optimization complete"
    };
    let recommendation = if report.compact_recommended {
        " Run `clipmem storage compact` to return freed pages to the filesystem."
    } else {
        ""
    };
    let compact_error = report
        .compact_error
        .as_ref()
        .map(|error| format!(" compact_error={error}"))
        .unwrap_or_default();
    format!(
        "{} format={} scanned={} compressed={} skipped={} conflicts={} original_bytes={} optimized_bytes={} logical_saved_bytes={} compact_run={} filesystem_saved_bytes={} filesystem_growth_bytes={}{}{}\n",
        action,
        report.format,
        report.scanned_rows,
        report.compressed_rows,
        report.skipped_rows,
        report.conflict_count,
        report.original_bytes,
        report.optimized_bytes,
        report.logical_saved_bytes,
        report.compact_run,
        report.filesystem_saved_bytes,
        report.filesystem_growth_bytes,
        compact_error,
        recommendation
    )
}

pub(in crate::cli) fn render_settings_view_text(view: &SettingsView) -> String {
    let mut out = String::new();
    out.push_str(&format!("paused: {}\n", view.paused));
    out.push_str(&format!(
        "api key filter: {}\n",
        view.api_key_filter_enabled
    ));
    out.push_str(&format!("ocr: {}\n", view.ocr_enabled));
    out.push_str(&format!("retention: {}\n", view.retention));
    out.push_str(&format!(
        "ignored bundle ids: {}\n",
        view.ignored_bundle_ids.len()
    ));
    for bundle_id in &view.ignored_bundle_ids {
        out.push_str(&format!("  - {bundle_id}\n"));
    }
    out
}

pub(in crate::cli) fn render_settings_ignore_list_text(
    output: &SettingsIgnoreListOutput,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "ignored bundle ids: {}\n",
        output.ignored_bundle_ids.len()
    ));
    for bundle_id in &output.ignored_bundle_ids {
        out.push_str(&format!("  - {bundle_id}\n"));
    }
    out
}

pub(in crate::cli) fn render_retention_value(settings: &CaptureSettings) -> String {
    settings
        .retention_seconds()
        .map(format_duration_compact)
        .unwrap_or_else(|| "forever".to_string())
}
