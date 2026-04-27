use std::path::Path;

use anyhow::Result;

use crate::db::{OcrRunReport, OcrStatusReport};

use crate::cli::formats::OutputFormat;
use crate::cli::human::{render_ocr_run_human, render_ocr_status_human};
use crate::cli::presentation::emit_json_or_text;
use crate::cli::schema::{OcrArgs, OcrCommand, OcrRunArgs, OcrStatusArgs};

use super::mutation_support::require_text_or_json;
use super::runtime::open_or_init_db;

pub(in crate::cli) fn ocr(db_path: &Path, args: &OcrArgs) -> Result<()> {
    match &args.command {
        OcrCommand::Status(args) => ocr_status(db_path, args),
        OcrCommand::Run(args) => ocr_run(db_path, args),
    }
}

fn ocr_status(db_path: &Path, args: &OcrStatusArgs) -> Result<()> {
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

fn ocr_run(db_path: &Path, args: &OcrRunArgs) -> Result<()> {
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

fn render_ocr_status_text(report: &OcrStatusReport) -> String {
    format!(
        "ocr pending={} ready={} failed={} skipped={} snapshots_with_text={}\n",
        report.pending(),
        report.ready(),
        report.failed(),
        report.skipped(),
        report.snapshots_with_ocr_text()
    )
}

fn render_ocr_run_text(report: &OcrRunReport) -> String {
    format!(
        "ocr processed={} ready={} failed={} skipped={} remaining_pending={}\n",
        report.processed(),
        report.ready(),
        report.failed(),
        report.skipped(),
        report.remaining_pending()
    )
}
