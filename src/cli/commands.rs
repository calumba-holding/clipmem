use std::path::Path;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use serde::Serialize;

use crate::app::{format_watch_capture_line, process_watch_snapshot, WatchState};
use crate::db::{Database, SearchMode, SearchResults};
use crate::model::{CaptureStoreResult, ClipboardSnapshot};
use crate::platform::{capture_snapshot, current_change_count};

use super::output::{
    emit_json_or_text, render_capture_once_text, render_doctor_text, render_hits_text,
    render_search_results_text, render_snapshot_text,
};
use super::{
    CaptureOnceArgs, Command, DoctorArgs, ExportArgs, GetArgs, RecentArgs, SearchArgs, WatchArgs,
};

#[derive(Debug, Serialize)]
struct CaptureOnceOutput {
    store: CaptureStoreResult,
    snapshot: ClipboardSnapshot,
}

pub(super) fn run_command(command: Command, db_path: &Path) -> Result<()> {
    match command {
        Command::Watch(args) => watch(db_path, &args),
        Command::CaptureOnce(args) => capture_once(db_path, &args),
        Command::Search(args) => search(db_path, &args),
        Command::Recent(args) => recent(db_path, &args),
        Command::Get(args) => show_snapshot(db_path, &args),
        Command::Export(args) => export_snapshot_bytes(db_path, &args),
        Command::Doctor(args) => doctor(db_path, &args),
    }
}

fn watch(db_path: &Path, args: &WatchArgs) -> Result<()> {
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

fn run_watch_iteration(db: &mut Database, args: &WatchArgs, state: &mut WatchState) -> Result<()> {
    run_watch_iteration_with_capture(
        db,
        args,
        state,
        || current_change_count(),
        || capture_snapshot(),
    )
}

fn run_watch_iteration_with_capture<CountFn, CaptureFn>(
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
    let change_count =
        anyhow::Context::context(current_change_count_fn(), "read clipboard change count failed")?;

    if !crate::app::should_capture_change(change_count, args.skip_initial, state) {
        return Ok(());
    }

    let snapshot = anyhow::Context::context(capture_snapshot_fn(), "capture failed")?;
    let result =
        anyhow::Context::context(process_watch_snapshot(db, &snapshot, state), "database write failed")?;

    if !args.quiet {
        if let Some(result) = result {
            println!("{}", format_watch_capture_line(&snapshot, &result));
        }
    }

    Ok(())
}

fn capture_once(db_path: &Path, args: &CaptureOnceArgs) -> Result<()> {
    let mut db = open_or_init_db(db_path)?;
    let snapshot =
        anyhow::Context::context(capture_snapshot(), "capture-once clipboard read failed")?;
    let store = anyhow::Context::context(
        db.store_capture(&snapshot),
        "capture-once database write failed",
    )?;
    let payload = CaptureOnceOutput { store, snapshot };
    emit_json_or_text(args.json, &payload, |output| {
        render_capture_once_text(&output.store, &output.snapshot)
    })?;

    Ok(())
}

fn search(db_path: &Path, args: &SearchArgs) -> Result<()> {
    let db = open_existing_db(db_path)?;
    let results = anyhow::Context::with_context(query_search_results(&db, args), || {
        format!("search failed for query '{}'", args.query)
    })?;
    emit_json_or_text(args.json, &results, render_search_results_text)?;

    Ok(())
}

fn recent(db_path: &Path, args: &RecentArgs) -> Result<()> {
    let db = open_existing_db(db_path)?;
    let hits = anyhow::Context::with_context(db.recent(args.limit, args.hours), || {
        args.hours.map_or_else(
            || "recent query failed".to_string(),
            |hours| format!("recent query failed for the last {hours} hours"),
        )
    })?;
    emit_json_or_text(args.json, &hits, |value| render_hits_text(value))?;

    Ok(())
}

fn show_snapshot(db_path: &Path, args: &GetArgs) -> Result<()> {
    let db = open_existing_db(db_path)?;
    let snapshot =
        anyhow::Context::with_context(db.find_snapshot(args.snapshot_id, args.events), || {
            format!("get failed for snapshot {}", args.snapshot_id)
        })?
        .ok_or_else(|| anyhow::anyhow!("snapshot {} was not found", args.snapshot_id))?;
    emit_json_or_text(args.json, &snapshot, render_snapshot_text)?;

    Ok(())
}

fn export_snapshot_bytes(db_path: &Path, args: &ExportArgs) -> Result<()> {
    let db = open_existing_db(db_path)?;
    let representation = db
        .find_representation_bytes(args.snapshot_id, args.item, &args.uti)?
        .ok_or_else(|| {
            anyhow::anyhow!(
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

    anyhow::Context::with_context(std::fs::write(&args.out, representation.raw_bytes()), || {
        format!("failed to write {}", args.out.display())
    })?;

    println!(
        "exported snapshot={} item={} uti={} bytes={} sha256={} out={}",
        args.snapshot_id,
        args.item,
        args.uti,
        representation.byte_len(),
        representation.raw_sha256(),
        args.out.display()
    );

    Ok(())
}

fn doctor(db_path: &Path, args: &DoctorArgs) -> Result<()> {
    let db = open_existing_db(db_path)?;
    let report = anyhow::Context::context(db.doctor(), "doctor diagnostics failed")?;
    emit_json_or_text(args.json, &report, render_doctor_text)?;

    Ok(())
}

fn open_existing_db(path: &Path) -> Result<Database> {
    anyhow::Context::with_context(Database::open_existing(path), || {
        format!("failed to open database at {}", path.display())
    })
}

fn open_or_init_db(path: &Path) -> Result<Database> {
    anyhow::Context::with_context(Database::open_or_init(path), || {
        format!("failed to open database at {}", path.display())
    })
}

fn query_search_results(db: &Database, args: &SearchArgs) -> Result<SearchResults> {
    match args.mode {
        SearchMode::Auto => db.search_auto(&args.query, args.limit),
        SearchMode::Fts => db.search_fts(&args.query, args.limit),
        SearchMode::Literal => db.search_literal(&args.query, args.limit),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::fs;
    use std::path::PathBuf;
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        query_search_results, run_watch_iteration_with_capture, SearchArgs, WatchArgs, WatchState,
    };
    use crate::db::{Database, SearchMode};
    use crate::model::{build_item, build_representation, build_snapshot, CaptureContext};

    fn temp_db_path(test_name: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();

        std::env::temp_dir()
            .join("clipmem-main-tests")
            .join(format!("{test_name}-{}-{timestamp}.sqlite3", process::id()))
    }

    fn cleanup_db(path: &std::path::Path) {
        for suffix in ["", "-shm", "-wal"] {
            let candidate = if suffix.is_empty() {
                path.to_path_buf()
            } else {
                PathBuf::from(format!("{}{suffix}", path.display()))
            };
            let _ = fs::remove_file(candidate);
        }

        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    fn search_query_dispatches_literal_mode_against_database() {
        let path = temp_db_path("search-literal");
        let mut db = Database::open_or_init(&path).expect("test database should open");
        let first = build_snapshot(
            CaptureContext::new(1)
                .with_frontmost_app_name("Editor")
                .with_frontmost_app_bundle_id("com.example.Editor"),
            vec![build_item(
                0,
                vec![build_representation(
                    "public.utf8-plain-text".to_string(),
                    None,
                    b"Discount: 50 percent off".to_vec(),
                )],
            )],
        );
        let second = build_snapshot(
            CaptureContext::new(2)
                .with_frontmost_app_name("Editor")
                .with_frontmost_app_bundle_id("com.example.Editor"),
            vec![build_item(
                0,
                vec![build_representation(
                    "public.utf8-plain-text".to_string(),
                    None,
                    b"Discount: 50%".to_vec(),
                )],
            )],
        );
        db.store_capture(&first)
            .expect("first search fixture should store");
        db.store_capture(&second)
            .expect("second search fixture should store");

        let results = query_search_results(
            &db,
            &SearchArgs {
                query: "50%".to_string(),
                mode: SearchMode::Literal,
                limit: 10,
                json: false,
            },
        )
        .expect("search should succeed");

        assert_eq!(results.mode_used(), SearchMode::Literal);
        assert_eq!(results.hits().len(), 1);
        assert_eq!(results.hits()[0].preview_text(), "Discount: 50%");

        cleanup_db(&path);
    }

    #[test]
    fn watch_iteration_skips_full_capture_when_change_count_is_unchanged() {
        let path = temp_db_path("watch-no-capture");
        let mut db = Database::open_or_init(&path).expect("test database should open");
        let mut state = WatchState::new();
        let args = WatchArgs {
            interval_ms: 250,
            quiet: true,
            skip_initial: false,
        };
        let capture_calls = Cell::new(0);

        let first = run_watch_iteration_with_capture(
            &mut db,
            &args,
            &mut state,
            || Ok(7),
            || {
                capture_calls.set(capture_calls.get() + 1);
                Ok(build_snapshot(CaptureContext::new(7), Vec::new()))
            },
        );
        let second = run_watch_iteration_with_capture(
            &mut db,
            &args,
            &mut state,
            || Ok(7),
            || {
                capture_calls.set(capture_calls.get() + 1);
                Ok(build_snapshot(CaptureContext::new(7), Vec::new()))
            },
        );

        assert!(first.is_ok());
        assert!(second.is_ok());
        assert_eq!(capture_calls.get(), 1);
        cleanup_db(&path);
    }

    #[test]
    fn watch_iteration_skips_initial_capture_without_materialising_snapshot() {
        let path = temp_db_path("watch-skip-initial");
        let mut db = Database::open_or_init(&path).expect("test database should open");
        let mut state = WatchState::new();
        let args = WatchArgs {
            interval_ms: 250,
            quiet: true,
            skip_initial: true,
        };
        let capture_calls = Cell::new(0);

        let result = run_watch_iteration_with_capture(
            &mut db,
            &args,
            &mut state,
            || Ok(7),
            || {
                capture_calls.set(capture_calls.get() + 1);
                Ok(build_snapshot(CaptureContext::new(7), Vec::new()))
            },
        );

        assert!(result.is_ok());
        assert_eq!(capture_calls.get(), 0);
        cleanup_db(&path);
    }
}
