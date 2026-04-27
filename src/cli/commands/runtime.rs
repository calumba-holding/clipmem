use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering as AtomicOrdering;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::app::{format_watch_capture_line, mark_change_handled, WatchState};
use crate::db::{CaptureStoreOutcome, Database};
use crate::model::ClipboardSnapshot;
use crate::platform::{capture_snapshot, current_change_count};

use super::types::{
    capture_skip_reason_label, CaptureOnceOutput, CaptureOnceSkippedOutput, CaptureOnceStoredOutput,
};
use super::OCR_WORKER_RUNNING;
use crate::cli::errors::{db_error, platform_error};
use crate::cli::human::render_capture_once_human;
use crate::cli::output::{emit_json_or_text, render_capture_once_text};
use crate::cli::schema::{CaptureOnceArgs, WatchArgs};

pub(in crate::cli) fn watch(db_path: &Path, args: &WatchArgs) -> Result<()> {
    let mut db = open_or_init_db(db_path)?;
    let interval_ms = args.interval_ms.max(50);
    let mut state = WatchState::new();

    loop {
        if let Err(err) = run_watch_iteration(&mut db, args, &mut state) {
            eprintln!("{err:#}");
        }

        thread::sleep(Duration::from_millis(interval_ms));
    }
}

pub(in crate::cli) fn run_watch_iteration(
    db: &mut Database,
    args: &WatchArgs,
    state: &mut WatchState,
) -> Result<()> {
    run_watch_iteration_with_capture(db, args, state, current_change_count, capture_snapshot)
}

pub(in crate::cli) fn run_watch_iteration_with_capture<CountFn, CaptureFn>(
    db: &mut Database,
    args: &WatchArgs,
    state: &mut WatchState,
    current_change_count_fn: CountFn,
    capture_snapshot_fn: CaptureFn,
) -> Result<()>
where
    CountFn: FnOnce() -> Result<i64>,
    CaptureFn: FnOnce() -> Result<ClipboardSnapshot>,
{
    let change_count = current_change_count_fn()
        .map_err(|error| platform_error(format!("read clipboard change count failed: {error}")))?;

    if !crate::app::should_capture_change(change_count, args.skip_initial, state) {
        return Ok(());
    }

    let snapshot = capture_snapshot_fn()
        .map_err(|error| platform_error(format!("capture failed: {error}")))?;
    let settings = db
        .capture_settings()
        .context("load capture settings failed")?;
    if settings.paused() {
        mark_change_handled(snapshot.change_count(), state);
        return Ok(());
    }
    if db
        .bundle_id_is_ignored(snapshot.frontmost_app_bundle_id())
        .context("load ignored bundle ids failed")?
    {
        mark_change_handled(snapshot.change_count(), state);
        return Ok(());
    }
    let outcome = anyhow::Context::context(
        db.store_watched_capture_if_allowed(&snapshot),
        "database write failed",
    )?;
    match outcome {
        CaptureStoreOutcome::Stored(result) => {
            if settings.ocr_enabled() {
                let snapshot_id = result.snapshot_id();
                if let Err(err) = db.enqueue_ocr_for_snapshot(snapshot_id) {
                    eprintln!("ocr enqueue failed for snapshot {snapshot_id}: {err:#}");
                } else {
                    start_ocr_worker(db.path().to_path_buf());
                }
            }
            mark_change_handled(snapshot.change_count(), state);
            db.apply_retention_policy()
                .context("apply retention policy failed")?;
            if !args.quiet {
                println!("{}", format_watch_capture_line(&snapshot, &result));
            }
        }
        CaptureStoreOutcome::Skipped(reason) => {
            mark_change_handled(snapshot.change_count(), state);
            if !args.quiet {
                println!(
                    "skipped capture reason={} kind={} bytes={} source={}",
                    capture_skip_reason_label(reason),
                    snapshot.snapshot_kind(),
                    snapshot.total_bytes(),
                    snapshot
                        .frontmost_app_name()
                        .or(snapshot.frontmost_app_bundle_id())
                        .unwrap_or("unknown app")
                );
            }
        }
    }

    Ok(())
}

pub(in crate::cli) fn start_ocr_worker(db_path: PathBuf) {
    if OCR_WORKER_RUNNING.swap(true, AtomicOrdering::AcqRel) {
        return;
    }

    thread::spawn(move || {
        struct ResetWorkerFlag;

        impl Drop for ResetWorkerFlag {
            fn drop(&mut self) {
                OCR_WORKER_RUNNING.store(false, AtomicOrdering::Release);
            }
        }

        let _reset = ResetWorkerFlag;
        let run = || -> Result<()> {
            let mut worker_db = Database::open_or_init(&db_path)?;
            let engine = crate::ocr::default_engine();
            loop {
                let report = crate::ocr::run_ocr_jobs(&mut worker_db, &engine, 1, None, false)?;
                if report.processed() == 0 {
                    break;
                }
            }
            Ok(())
        };
        if let Err(err) = run() {
            eprintln!("ocr failed: {err:#}");
        }
    });
}

pub(in crate::cli) fn capture_once(db_path: &Path, args: &CaptureOnceArgs) -> Result<()> {
    let mut db = open_or_init_db(db_path)?;
    let snapshot = capture_snapshot()
        .map_err(|error| platform_error(format!("capture-once clipboard read failed: {error}")))?;
    let payload = match anyhow::Context::context(
        db.store_capture_if_allowed(&snapshot),
        "capture-once database write failed",
    )? {
        CaptureStoreOutcome::Stored(store) => {
            db.apply_retention_policy()
                .context("apply retention policy failed")?;
            CaptureOnceOutput::Stored(CaptureOnceStoredOutput { store, snapshot })
        }
        CaptureStoreOutcome::Skipped(reason) => {
            CaptureOnceOutput::Skipped(CaptureOnceSkippedOutput {
                status: "skipped",
                reason,
                kind: snapshot.snapshot_kind().as_str().to_string(),
                total_bytes: snapshot.total_bytes(),
                frontmost_app_name: snapshot.frontmost_app_name().map(str::to_string),
                frontmost_app_bundle_id: snapshot.frontmost_app_bundle_id().map(str::to_string),
            })
        }
    };
    if args.human {
        print!("{}", render_capture_once_human(&payload));
    } else {
        emit_json_or_text(args.json, &payload, render_capture_once_text)?;
    }

    Ok(())
}

pub(in crate::cli) fn open_existing_db(path: &Path) -> Result<Database> {
    if !path.is_file() {
        return Err(db_error(format!(
            "database does not exist at {}. Run `clipmem setup` to initialize capture.",
            path.display()
        )));
    }
    match Database::open_existing(path) {
        Ok(db) => {
            if let Err(error) = db.ensure_supported_schema_shape() {
                if error
                    .chain()
                    .any(|cause| cause.to_string().contains("prerelease schema"))
                {
                    return Err(db_error(
                        "database operation failed; this may be an incompatible prerelease schema. Move the database aside and run `clipmem setup`."
                    ));
                }
                return Err(error);
            }
            Ok(db)
        }
        Err(error) => {
            if error
                .chain()
                .any(|cause| cause.to_string().contains("incompatible prerelease schema"))
            {
                Err(db_error(format!(
                    "incompatible prerelease schema detected at {}. Move the database aside and run `clipmem setup`.",
                    path.display()
                )))
            } else {
                Err(error).with_context(|| format!("failed to open database at {}", path.display()))
            }
        }
    }
}

pub(in crate::cli) fn open_or_init_db(path: &Path) -> Result<Database> {
    anyhow::Context::with_context(Database::open_or_init(path), || {
        format!("failed to open database at {}", path.display())
    })
}
