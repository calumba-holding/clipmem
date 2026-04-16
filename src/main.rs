use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use clipmem::app::{format_watch_capture_line, process_watch_snapshot, WatchState};
use clipmem::db::{Database, SearchMode};
use clipmem::model::{
    CaptureStoreResult, ClipboardSnapshot, DoctorReport, SearchHit, SnapshotDetails,
};
use clipmem::paths::default_db_path;
use clipmem::platform::capture_snapshot;
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(name = "clipmem")]
#[command(version)]
#[command(about = "macOS clipboard memory backed by SQLite")]
struct Cli {
    /// Path to the `SQLite` database.
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Continuously watch the clipboard and archive changes.
    Watch(WatchArgs),
    /// Capture the current clipboard state once.
    CaptureOnce(CaptureOnceArgs),
    /// Search the clipboard archive.
    Search(SearchArgs),
    /// Show recently captured clipboard states.
    Recent(RecentArgs),
    /// Show a stored snapshot in detail.
    Get(GetArgs),
    /// Print `SQLite` and FTS5 diagnostics.
    Doctor(DoctorArgs),
}

#[derive(Debug, Args)]
struct WatchArgs {
    /// Poll interval in milliseconds.
    #[arg(long, default_value_t = 400)]
    interval_ms: u64,

    /// Do not print one-line status messages for each capture.
    #[arg(long, default_value_t = false)]
    quiet: bool,

    /// Skip capturing the clipboard state that already exists when the watcher starts.
    #[arg(long, default_value_t = false)]
    skip_initial: bool,
}

#[derive(Debug, Args)]
struct CaptureOnceArgs {
    /// Emit the captured snapshot as JSON.
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Debug, Args)]
struct SearchArgs {
    /// Query string for the selected search mode.
    query: String,

    /// Search mode to execute.
    #[arg(long, value_enum, default_value_t = SearchModeArg::Auto)]
    mode: SearchModeArg,

    /// Maximum number of results.
    #[arg(long, default_value_t = 10)]
    limit: usize,

    /// Emit results as JSON.
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SearchModeArg {
    Auto,
    Fts,
    Literal,
}

impl From<SearchModeArg> for SearchMode {
    fn from(value: SearchModeArg) -> Self {
        match value {
            SearchModeArg::Auto => Self::Auto,
            SearchModeArg::Fts => Self::Fts,
            SearchModeArg::Literal => Self::Literal,
        }
    }
}

#[derive(Debug, Args)]
struct RecentArgs {
    /// Maximum number of results.
    #[arg(long, default_value_t = 10)]
    limit: usize,

    /// Restrict results to the most recent N hours.
    #[arg(long)]
    hours: Option<u32>,

    /// Emit results as JSON.
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Debug, Args)]
struct GetArgs {
    /// Snapshot identifier.
    snapshot_id: i64,

    /// Number of recent events to include.
    #[arg(long, default_value_t = 10)]
    events: usize,

    /// Emit the snapshot as JSON.
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Debug, Args)]
struct DoctorArgs {
    /// Emit diagnostics as JSON.
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Debug, Serialize)]
struct CaptureOnceOutput {
    store: CaptureStoreResult,
    snapshot: ClipboardSnapshot,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let db_path = cli.db.unwrap_or_else(default_db_path);

    match cli.command {
        Command::Watch(args) => watch(&db_path, &args),
        Command::CaptureOnce(args) => capture_once(&db_path, &args),
        Command::Search(args) => search(&db_path, &args),
        Command::Recent(args) => recent(&db_path, &args),
        Command::Get(args) => show_snapshot(&db_path, &args),
        Command::Doctor(args) => doctor(&db_path, &args),
    }
}

fn watch(db_path: &Path, args: &WatchArgs) -> Result<()> {
    let mut db = Database::open_or_init(db_path)
        .with_context(|| format!("failed to open database at {}", db_path.display()))?;
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
    let snapshot = capture_snapshot().context("capture failed")?;
    let result = process_watch_snapshot(db, &snapshot, args.skip_initial, state)
        .context("database write failed")?;

    if !args.quiet {
        if let Some(result) = result {
            println!("{}", format_watch_capture_line(&snapshot, &result));
        }
    }

    Ok(())
}

fn capture_once(db_path: &Path, args: &CaptureOnceArgs) -> Result<()> {
    let mut db = Database::open_or_init(db_path)
        .with_context(|| format!("failed to open database at {}", db_path.display()))?;
    let snapshot = capture_snapshot()?;
    let store = db.store_capture(&snapshot)?;

    if args.json {
        let payload = CaptureOnceOutput { store, snapshot };
        print_json(&payload)?;
    } else {
        println!(
            "stored snapshot {} via event {} ({})",
            store.snapshot_id,
            store.event_id,
            if store.inserted_new_snapshot {
                "new content"
            } else {
                "already known content"
            }
        );
        println!("kind: {}", snapshot.snapshot_kind);
        println!("bytes: {}", snapshot.total_bytes);
        if let Some(app) = snapshot.frontmost_app_name.as_deref() {
            println!("frontmost app: {app}");
        }
        if !snapshot.preview_text.is_empty() {
            println!("preview: {}", snapshot.preview_text);
        }
    }

    Ok(())
}

fn search(db_path: &Path, args: &SearchArgs) -> Result<()> {
    let db = Database::open_existing(db_path)
        .with_context(|| format!("failed to open database at {}", db_path.display()))?;
    let hits = match SearchMode::from(args.mode) {
        SearchMode::Auto => db.search_auto(&args.query, args.limit)?,
        SearchMode::Fts => db.search_fts(&args.query, args.limit)?,
        SearchMode::Literal => db.search_literal(&args.query, args.limit)?,
    };

    if args.json {
        print_json(&hits)?;
    } else {
        print_hits(&hits);
    }

    Ok(())
}

fn recent(db_path: &Path, args: &RecentArgs) -> Result<()> {
    let db = Database::open_existing(db_path)
        .with_context(|| format!("failed to open database at {}", db_path.display()))?;
    let hits = db.recent(args.limit, args.hours)?;

    if args.json {
        print_json(&hits)?;
    } else {
        print_hits(&hits);
    }

    Ok(())
}

fn show_snapshot(db_path: &Path, args: &GetArgs) -> Result<()> {
    let db = Database::open_existing(db_path)
        .with_context(|| format!("failed to open database at {}", db_path.display()))?;
    let snapshot = db
        .find_snapshot(args.snapshot_id, args.events)?
        .with_context(|| format!("snapshot {} was not found", args.snapshot_id))?;

    if args.json {
        print_json(&snapshot)?;
    } else {
        print_snapshot(&snapshot);
    }

    Ok(())
}

fn doctor(db_path: &Path, args: &DoctorArgs) -> Result<()> {
    let db = Database::open_existing(db_path)
        .with_context(|| format!("failed to open database at {}", db_path.display()))?;
    let report = db.doctor()?;

    if args.json {
        print_json(&report)?;
    } else {
        print_doctor(&report);
    }

    Ok(())
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(value)?;
    println!("{json}");
    Ok(())
}

fn print_hits(hits: &[SearchHit]) {
    for hit in hits {
        let app = hit
            .last_frontmost_app_name
            .as_deref()
            .or(hit.last_frontmost_app_bundle_id.as_deref())
            .unwrap_or("unknown app");

        println!(
            "[{}] {} · last seen {} · captures {} · {} · {} bytes · {} items",
            hit.snapshot_id,
            hit.snapshot_kind,
            hit.last_observed_at,
            hit.capture_count,
            app,
            hit.total_bytes,
            hit.item_count
        );

        if !hit.preview_text.is_empty() {
            println!("  preview: {}", hit.preview_text);
        }
        if !hit.snippet.is_empty() && hit.snippet != hit.preview_text {
            println!("  match:   {}", hit.snippet);
        }
        if let Some(score) = hit.score {
            println!("  score:   {score:.3}");
        }
        println!();
    }
}

fn print_snapshot(snapshot: &SnapshotDetails) {
    println!(
        "snapshot {} · sha256={} · kind={} · captures={}",
        snapshot.snapshot_id, snapshot.sha256, snapshot.snapshot_kind, snapshot.capture_count
    );
    println!(
        "first seen: {} · last seen: {}",
        snapshot.first_observed_at, snapshot.last_observed_at
    );
    println!(
        "bytes: {} · items: {}",
        snapshot.total_bytes, snapshot.item_count
    );
    if let Some(app) = snapshot
        .last_frontmost_app_name
        .as_deref()
        .or(snapshot.last_frontmost_app_bundle_id.as_deref())
    {
        println!("last frontmost app: {app}");
    }
    if !snapshot.preview_text.is_empty() {
        println!("preview: {}", snapshot.preview_text);
    }
    println!();

    for item in &snapshot.items {
        println!(
            "item {} · kind={} · bytes={}{}",
            item.item_index,
            item.primary_kind,
            item.total_bytes,
            item.primary_uti
                .as_deref()
                .map(|uti| format!(" · primary type={uti}"))
                .unwrap_or_default()
        );
        if !item.preview_text.is_empty() {
            println!("  preview: {}", item.preview_text);
        }
        for rep in &item.representations {
            println!(
                "  - {} · kind={} · {} bytes · sha256={}",
                rep.uti, rep.kind, rep.byte_len, rep.raw_sha256
            );
            if let Some(text) = rep.text_value.as_deref() {
                println!("    text: {text}");
            }
        }
        println!();
    }

    if !snapshot.recent_events.is_empty() {
        println!("recent events:");
        for event in &snapshot.recent_events {
            let source = event
                .frontmost_app_name
                .as_deref()
                .or(event.frontmost_app_bundle_id.as_deref())
                .unwrap_or("unknown app");
            println!(
                "  event {} · {} · change_count={} · {}",
                event.event_id, event.observed_at, event.change_count, source
            );
        }
    }
}

fn print_doctor(report: &DoctorReport) {
    println!("database: {}", report.db_path);
    println!("sqlite version: {}", report.sqlite_version);
    println!("journal mode: {}", report.journal_mode);
    println!(
        "fts5 compile option present: {}",
        report.fts5_compile_option_present
    );
    println!(
        "fts5 temp table creation works: {}",
        report.fts5_create_virtual_table_ok
    );

    if !report.compile_options.is_empty() {
        println!();
        println!("compile options:");
        for opt in &report.compile_options {
            println!("  {opt}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use clap::Parser;

    use super::{Cli, Command};

    #[test]
    fn watch_command_parses_global_db_and_runtime_flags() {
        let cli = Cli::parse_from([
            "clipmem",
            "--db",
            "/tmp/clipmem.sqlite3",
            "watch",
            "--interval-ms",
            "250",
            "--quiet",
            "--skip-initial",
        ]);

        assert_eq!(cli.db, Some(PathBuf::from("/tmp/clipmem.sqlite3")));
        match cli.command {
            Command::Watch(args) => {
                assert_eq!(args.interval_ms, 250);
                assert!(args.quiet);
                assert!(args.skip_initial);
            }
            other => panic!("expected watch command, got {other:?}"),
        }
    }

    #[test]
    fn search_command_parses_explicit_mode() {
        let cli = Cli::parse_from(["clipmem", "search", "--mode", "literal", "50%"]);

        match cli.command {
            Command::Search(args) => {
                assert!(matches!(args.mode, super::SearchModeArg::Literal));
                assert_eq!(args.query, "50%");
            }
            other => panic!("expected search command, got {other:?}"),
        }
    }
}
