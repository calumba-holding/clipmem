mod db;
mod model;
mod platform;
mod util;

use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use serde::Serialize;

use crate::db::Database;
use crate::model::{CaptureStoreResult, ClipboardSnapshot, DoctorReport, SearchHit, SnapshotDetails};
use crate::platform::capture_snapshot;
use crate::util::default_db_path;

#[derive(Debug, Parser)]
#[command(name = "clipmem")]
#[command(version)]
#[command(about = "macOS clipboard memory backed by SQLite")]
struct Cli {
    /// Path to the SQLite database.
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
    /// Print SQLite and FTS5 diagnostics.
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
    /// SQLite FTS5 query string.
    query: String,

    /// Maximum number of results.
    #[arg(long, default_value_t = 10)]
    limit: usize,

    /// Emit results as JSON.
    #[arg(long, default_value_t = false)]
    json: bool,
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
        Command::Watch(args) => watch(db_path, args),
        Command::CaptureOnce(args) => capture_once(db_path, args),
        Command::Search(args) => search(db_path, args),
        Command::Recent(args) => recent(db_path, args),
        Command::Get(args) => get_snapshot(db_path, args),
        Command::Doctor(args) => doctor(db_path, args),
    }
}

fn watch(db_path: PathBuf, args: WatchArgs) -> Result<()> {
    let mut db = Database::open(&db_path)
        .with_context(|| format!("failed to open database at {}", db_path.display()))?;

    let interval_ms = args.interval_ms.max(50);
    let mut last_handled_change_count: Option<i64> = None;
    let mut first_loop = true;

    loop {
        let snapshot = match capture_snapshot() {
            Ok(snapshot) => snapshot,
            Err(err) => {
                eprintln!("capture failed: {err:#}");
                thread::sleep(Duration::from_millis(interval_ms));
                continue;
            }
        };

        if first_loop && args.skip_initial {
            last_handled_change_count = Some(snapshot.change_count);
            first_loop = false;
            thread::sleep(Duration::from_millis(interval_ms));
            continue;
        }
        first_loop = false;

        if last_handled_change_count == Some(snapshot.change_count) {
            thread::sleep(Duration::from_millis(interval_ms));
            continue;
        }

        let result = match db.store_capture(&snapshot) {
            Ok(result) => result,
            Err(err) => {
                eprintln!("database write failed: {err:#}");
                thread::sleep(Duration::from_millis(interval_ms));
                continue;
            }
        };

        last_handled_change_count = Some(snapshot.change_count);

        if !args.quiet {
            let source = snapshot
                .frontmost_app_name
                .as_deref()
                .or(snapshot.frontmost_app_bundle_id.as_deref())
                .unwrap_or("unknown app");

            println!(
                "event={} snapshot={} {} kind={} bytes={} source={} preview={}",
                result.event_id,
                result.snapshot_id,
                if result.inserted_new_snapshot { "new" } else { "seen" },
                snapshot.snapshot_kind,
                snapshot.total_bytes,
                source,
                snapshot.preview_text
            );
        }

        thread::sleep(Duration::from_millis(interval_ms));
    }
}

fn capture_once(db_path: PathBuf, args: CaptureOnceArgs) -> Result<()> {
    let mut db = Database::open(&db_path)
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

fn search(db_path: PathBuf, args: SearchArgs) -> Result<()> {
    let db = Database::open(&db_path)
        .with_context(|| format!("failed to open database at {}", db_path.display()))?;
    let hits = db.search(&args.query, args.limit)?;

    if args.json {
        print_json(&hits)?;
    } else {
        print_hits(&hits);
    }

    Ok(())
}

fn recent(db_path: PathBuf, args: RecentArgs) -> Result<()> {
    let db = Database::open(&db_path)
        .with_context(|| format!("failed to open database at {}", db_path.display()))?;
    let hits = db.recent(args.limit, args.hours)?;

    if args.json {
        print_json(&hits)?;
    } else {
        print_hits(&hits);
    }

    Ok(())
}

fn get_snapshot(db_path: PathBuf, args: GetArgs) -> Result<()> {
    let db = Database::open(&db_path)
        .with_context(|| format!("failed to open database at {}", db_path.display()))?;
    let snapshot = db
        .get_snapshot(args.snapshot_id, args.events)?
        .with_context(|| format!("snapshot {} was not found", args.snapshot_id))?;

    if args.json {
        print_json(&snapshot)?;
    } else {
        print_snapshot(&snapshot);
    }

    Ok(())
}

fn doctor(db_path: PathBuf, args: DoctorArgs) -> Result<()> {
    let db = Database::open(&db_path)
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
                rep.uti, rep.classification, rep.byte_len, rep.raw_sha256
            );
            if let Some(text) = rep.text_value.as_deref() {
                println!("    text: {}", text);
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
    println!("fts5 compile option present: {}", report.fts5_compile_option_present);
    println!("fts5 temp table creation works: {}", report.fts5_create_virtual_table_ok);

    if !report.compile_options.is_empty() {
        println!();
        println!("compile options:");
        for opt in &report.compile_options {
            println!("  {opt}");
        }
    }
}
