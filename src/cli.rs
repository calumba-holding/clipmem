use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;

use crate::app::{format_watch_capture_line, process_watch_snapshot, WatchState};
use crate::db::{Database, SearchMode};
use crate::model::{
    CaptureStoreResult, ClipboardSnapshot, DoctorReport, SearchHit, SnapshotDetails,
};
use crate::paths::default_db_path;
use crate::platform::capture_snapshot;

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

/// Parse CLI arguments and execute the requested command.
///
/// # Errors
///
/// Returns an error if command execution fails.
pub fn run() -> Result<()> {
    let cli = Cli::parse();
    run_with_cli(cli)
}

fn run_with_cli(cli: Cli) -> Result<()> {
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
    let snapshot = anyhow::Context::context(capture_snapshot(), "capture failed")?;
    let result = anyhow::Context::context(
        process_watch_snapshot(db, &snapshot, args.skip_initial, state),
        "database write failed",
    )?;

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
    let hits = anyhow::Context::with_context(query_search_hits(&db, args), || {
        format!("search failed for query '{}'", args.query)
    })?;
    emit_json_or_text(args.json, &hits, |value| render_hits_text(value))?;

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

fn emit_json_or_text<T>(json: bool, value: &T, render_text: impl FnOnce(&T) -> String) -> Result<()>
where
    T: Serialize,
{
    if json {
        print_json(value)
    } else {
        print!("{}", render_text(value));
        Ok(())
    }
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(value)?;
    println!("{json}");
    Ok(())
}

fn query_search_hits(db: &Database, args: &SearchArgs) -> Result<Vec<SearchHit>> {
    match SearchMode::from(args.mode) {
        SearchMode::Auto => db.search_auto(&args.query, args.limit),
        SearchMode::Fts => db.search_fts(&args.query, args.limit),
        SearchMode::Literal => db.search_literal(&args.query, args.limit),
    }
}

fn render_capture_once_text(store: &CaptureStoreResult, snapshot: &ClipboardSnapshot) -> String {
    let mut out = String::new();
    let store_state = if store.inserted_new_snapshot {
        "new content"
    } else {
        "already known content"
    };
    let _ = writeln!(
        out,
        "stored snapshot {} via event {} ({store_state})",
        store.snapshot_id, store.event_id
    );
    let _ = writeln!(out, "kind: {}", snapshot.snapshot_kind);
    let _ = writeln!(out, "bytes: {}", snapshot.total_bytes);
    if let Some(app) = snapshot.frontmost_app_name.as_deref() {
        let _ = writeln!(out, "frontmost app: {app}");
    }
    if !snapshot.preview_text.is_empty() {
        let _ = writeln!(out, "preview: {}", snapshot.preview_text);
    }
    out
}

fn render_hits_text(hits: &[SearchHit]) -> String {
    let mut out = String::new();
    for hit in hits {
        let app = hit
            .last_frontmost_app_name
            .as_deref()
            .or(hit.last_frontmost_app_bundle_id.as_deref())
            .unwrap_or("unknown app");

        let _ = writeln!(
            out,
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
            let _ = writeln!(out, "  preview: {}", hit.preview_text);
        }
        if !hit.snippet.is_empty() && hit.snippet != hit.preview_text {
            let _ = writeln!(out, "  match:   {}", hit.snippet);
        }
        if let Some(score) = hit.score {
            let _ = writeln!(out, "  score:   {score:.3}");
        }
        push_blank_line(&mut out);
    }
    out
}

fn render_snapshot_text(snapshot: &SnapshotDetails) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "snapshot {} · sha256={} · kind={} · captures={}",
        snapshot.snapshot_id, snapshot.sha256, snapshot.snapshot_kind, snapshot.capture_count
    );
    let _ = writeln!(
        out,
        "first seen: {} · last seen: {}",
        snapshot.first_observed_at, snapshot.last_observed_at
    );
    let _ = writeln!(
        out,
        "bytes: {} · items: {}",
        snapshot.total_bytes, snapshot.item_count
    );
    if let Some(app) = snapshot
        .last_frontmost_app_name
        .as_deref()
        .or(snapshot.last_frontmost_app_bundle_id.as_deref())
    {
        let _ = writeln!(out, "last frontmost app: {app}");
    }
    if !snapshot.preview_text.is_empty() {
        let _ = writeln!(out, "preview: {}", snapshot.preview_text);
    }
    push_blank_line(&mut out);

    for item in &snapshot.items {
        let _ = writeln!(
            out,
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
            let _ = writeln!(out, "  preview: {}", item.preview_text);
        }
        for rep in &item.representations {
            let _ = writeln!(
                out,
                "  - {} · kind={} · {} bytes · sha256={}",
                rep.uti, rep.kind, rep.byte_len, rep.raw_sha256
            );
            if let Some(text) = rep.text_value.as_deref() {
                let _ = writeln!(out, "    text: {text}");
            }
        }
        push_blank_line(&mut out);
    }

    if !snapshot.recent_events.is_empty() {
        let _ = writeln!(out, "recent events:");
        for event in &snapshot.recent_events {
            let source = event
                .frontmost_app_name
                .as_deref()
                .or(event.frontmost_app_bundle_id.as_deref())
                .unwrap_or("unknown app");
            let _ = writeln!(
                out,
                "  event {} · {} · change_count={} · {}",
                event.event_id, event.observed_at, event.change_count, source
            );
        }
    }

    out
}

fn push_blank_line(out: &mut String) {
    let _ = Write::write_char(out, '\n');
}

fn render_doctor_text(report: &DoctorReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "database: {}", report.db_path);
    let _ = writeln!(out, "sqlite version: {}", report.sqlite_version);
    let _ = writeln!(out, "journal mode: {}", report.journal_mode);
    let _ = writeln!(
        out,
        "fts5 compile option present: {}",
        report.fts5_compile_option_present
    );
    let _ = writeln!(
        out,
        "fts5 temp table creation works: {}",
        report.fts5_create_virtual_table_ok
    );

    if !report.compile_options.is_empty() {
        out.push('\n');
        let _ = writeln!(out, "compile options:");
        for opt in &report.compile_options {
            let _ = writeln!(out, "  {opt}");
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

    use clap::Parser;

    use crate::clipboard::{build_item, build_representation, build_snapshot};
    use crate::db::Database;
    use crate::model::{
        CaptureContext, CaptureEvent, CaptureStoreResult, DoctorReport, SearchHit, SnapshotDetails,
        SnapshotKind,
    };

    use super::{
        query_search_hits, render_capture_once_text, render_doctor_text, render_hits_text,
        render_snapshot_text, Cli, Command, SearchArgs, SearchModeArg,
    };

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

        let hits = query_search_hits(
            &db,
            &SearchArgs {
                query: "50%".to_string(),
                mode: SearchModeArg::Literal,
                limit: 10,
                json: false,
            },
        )
        .expect("search should succeed");

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].preview_text, "Discount: 50%");

        cleanup_db(&path);
    }

    #[test]
    fn render_capture_once_text_reports_summary_lines() {
        let text = render_capture_once_text(
            &CaptureStoreResult {
                snapshot_id: 9,
                event_id: 12,
                inserted_new_snapshot: true,
            },
            &build_snapshot(
                CaptureContext::new(3)
                    .with_frontmost_app_name("Terminal")
                    .with_frontmost_app_bundle_id("com.apple.Terminal"),
                vec![build_item(
                    0,
                    vec![build_representation(
                        "public.utf8-plain-text".to_string(),
                        None,
                        b"git status".to_vec(),
                    )],
                )],
            ),
        );

        assert!(text.contains("stored snapshot 9 via event 12 (new content)"));
        assert!(text.contains("kind: plain_text"));
        assert!(text.contains("frontmost app: Terminal"));
        assert!(text.contains("preview: git status"));
    }

    #[test]
    fn render_hits_text_includes_snippet_and_score() {
        let text = render_hits_text(&[SearchHit {
            snapshot_id: 42,
            sha256: "deadbeef".to_string(),
            snapshot_kind: SnapshotKind::PlainText,
            preview_text: "git status".to_string(),
            snippet: "git ⟦status⟧".to_string(),
            capture_count: 2,
            last_observed_at: "2026-04-16 18:00:00".to_string(),
            last_frontmost_app_name: Some("Terminal".to_string()),
            last_frontmost_app_bundle_id: Some("com.apple.Terminal".to_string()),
            total_bytes: 10,
            item_count: 1,
            score: Some(0.125),
        }]);

        assert!(text.contains("[42] plain_text"));
        assert!(text.contains("preview: git status"));
        assert!(text.contains("match:   git ⟦status⟧"));
        assert!(text.contains("score:   0.125"));
    }

    #[test]
    fn render_snapshot_text_lists_items_and_events() {
        let text = render_snapshot_text(&SnapshotDetails {
            snapshot_id: 7,
            sha256: "abc123".to_string(),
            snapshot_kind: SnapshotKind::PlainText,
            preview_text: "git clone".to_string(),
            search_text: "git clone".to_string(),
            item_count: 1,
            total_bytes: 9,
            created_at: "2026-04-16 18:00:00".to_string(),
            capture_count: 1,
            first_observed_at: "2026-04-16 18:00:00".to_string(),
            last_observed_at: "2026-04-16 18:00:00".to_string(),
            last_frontmost_app_name: Some("Terminal".to_string()),
            last_frontmost_app_bundle_id: Some("com.apple.Terminal".to_string()),
            recent_events: vec![CaptureEvent {
                event_id: 9,
                observed_at: "2026-04-16 18:00:00".to_string(),
                change_count: 4,
                frontmost_app_name: Some("Terminal".to_string()),
                frontmost_app_bundle_id: Some("com.apple.Terminal".to_string()),
            }],
            items: vec![build_item(
                0,
                vec![build_representation(
                    "public.utf8-plain-text".to_string(),
                    None,
                    b"git clone".to_vec(),
                )],
            )],
        });

        assert!(text.contains("snapshot 7 · sha256=abc123 · kind=plain_text · captures=1"));
        assert!(text.contains("item 0 · kind=plain_text"));
        assert!(text.contains("text: git clone"));
        assert!(text.contains("event 9 · 2026-04-16 18:00:00 · change_count=4 · Terminal"));
    }

    #[test]
    fn render_doctor_text_lists_compile_options_when_present() {
        let text = render_doctor_text(&DoctorReport {
            db_path: "/tmp/clipmem.sqlite3".to_string(),
            sqlite_version: "3.46.0".to_string(),
            journal_mode: "wal".to_string(),
            fts5_compile_option_present: true,
            fts5_create_virtual_table_ok: true,
            compile_options: vec!["ENABLE_FTS5".to_string()],
        });

        assert!(text.contains("database: /tmp/clipmem.sqlite3"));
        assert!(text.contains("compile options:"));
        assert!(text.contains("ENABLE_FTS5"));
    }
}
