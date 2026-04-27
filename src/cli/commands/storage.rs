use std::path::Path;

use anyhow::Result;

use crate::db::{ImageOptimizationReport, StorageCompactReport};

use crate::cli::commands::runtime::open_existing_db;
use crate::cli::formats::{OutputFormat, ProgressFormat};
use crate::cli::human::{render_image_optimization_human, render_storage_compact_human};
use crate::cli::output::{emit_json_or_text, print_json_line};
use crate::cli::schema::{
    StorageArgs, StorageCommand, StorageCompactArgs, StorageOptimizeImagesArgs,
};

use super::mutation_support::require_text_or_json;

pub(in crate::cli) fn storage(db_path: &Path, args: &StorageArgs) -> Result<()> {
    match &args.command {
        StorageCommand::Compact(args) => storage_compact(db_path, args),
        StorageCommand::OptimizeImages(args) => storage_optimize_images(db_path, args),
    }
}

fn storage_compact(db_path: &Path, args: &StorageCompactArgs) -> Result<()> {
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

fn storage_optimize_images(db_path: &Path, args: &StorageOptimizeImagesArgs) -> Result<()> {
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

fn render_storage_compact_text(report: &StorageCompactReport) -> String {
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

fn render_image_optimization_text(report: &ImageOptimizationReport) -> String {
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::db::{Database, ImageOptimizationReport};

    use super::{render_image_optimization_text, render_storage_compact_text};

    #[test]
    fn render_storage_compact_text_distinguishes_dry_run_from_completed_compaction() {
        let path = temp_path("compact.sqlite3");
        let mut db = Database::open_or_init(&path).unwrap();
        let dry_run = db.compact_storage(true).unwrap();
        let completed = db.compact_storage(false).unwrap();

        let dry_run_text = render_storage_compact_text(&dry_run);
        let completed_text = render_storage_compact_text(&completed);

        assert!(dry_run_text.starts_with(&format!("storage compact dry-run db={}", path.display())));
        assert!(completed_text.starts_with(&format!("storage compacted db={}", path.display())));
        assert!(completed_text.contains("before="));
        assert!(completed_text.contains("after="));
        assert!(completed_text.contains("checkpoint_busy="));

        cleanup_path(&path);
    }

    #[test]
    fn render_image_optimization_text_reports_compact_error_and_recommendation() {
        let report = ImageOptimizationReport {
            dry_run: false,
            format: "webp",
            scanned_rows: 5,
            compressed_rows: 3,
            skipped_rows: 2,
            conflict_count: 1,
            original_bytes: 10_000,
            optimized_bytes: 4_000,
            logical_saved_bytes: 6_000,
            compact_run: false,
            compact: None,
            compact_error: Some("database is locked".to_string()),
            filesystem_saved_bytes: 0,
            filesystem_growth_bytes: 128,
            compact_recommended: true,
        };

        let rendered = render_image_optimization_text(&report);

        assert!(rendered.starts_with("image optimization complete; database compaction failed"));
        assert!(rendered.contains("format=webp scanned=5 compressed=3 skipped=2 conflicts=1"));
        assert!(rendered.contains("compact_error=database is locked"));
        assert!(rendered.contains("Run `clipmem storage compact`"));
    }

    fn temp_path(name: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();

        let dir = std::env::temp_dir()
            .join("clipmem-storage-tests")
            .join(format!("{}-{timestamp}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    fn cleanup_path(path: &std::path::Path) {
        if let Ok(metadata) = std::fs::symlink_metadata(path) {
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                let _ = std::fs::remove_dir_all(path);
            } else {
                let _ = std::fs::remove_file(path);
            }
        }
        if let Some(parent) = path.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }
}
