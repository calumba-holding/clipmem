use std::collections::HashMap;
use std::fs;
use std::hint::black_box;
use std::path::Path;
use std::path::PathBuf;
use std::process;
use std::process::Command;
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Result};
use clipmem::archive::{Database, RetrievalFilters, SearchMode};
use clipmem::capture::{
    build_item, build_representation, build_snapshot, CaptureContext, ClipboardSnapshot,
};
use rusqlite::Connection;
use serde_json::Value;

#[path = "search_benchmark/reporting.rs"]
mod reporting;

use reporting::{print_db_latency_report, print_quality_report};

const DEFAULT_FILLER_SNAPSHOTS: usize = 5_000;
const CLI_LATENCY_REPETITIONS: usize = 5;
const DB_LATENCY_REPETITIONS: usize = 25;

#[derive(Debug)]
struct EvalCase {
    name: &'static str,
    command: RetrievalCommand,
    query: &'static str,
    flags: &'static [&'static str],
    expected_key: &'static str,
    expected_mode: Option<&'static str>,
}

#[derive(Debug, Clone, Copy)]
enum RetrievalCommand {
    Recall,
    Search,
}

impl RetrievalCommand {
    fn as_str(self) -> &'static str {
        match self {
            Self::Recall => "recall",
            Self::Search => "search",
        }
    }
}

#[derive(Debug)]
struct EvalOutcome {
    name: &'static str,
    command: RetrievalCommand,
    expected_id: i64,
    rank: Option<usize>,
    top1_id: Option<i64>,
    mode_used: Option<String>,
    expected_mode: Option<&'static str>,
    median_cli_latency: Duration,
    p95_cli_latency: Duration,
}

#[derive(Debug)]
struct DbLatencyOutcome {
    name: &'static str,
    mode: SearchMode,
    median_latency: Duration,
    p95_latency: Duration,
    result_count: usize,
}

#[test]
#[ignore = "benchmark harness; run with `cargo test --test search_benchmark -- --ignored --nocapture`"]
fn benchmark_search_quality_and_latency() -> Result<()> {
    let path = temp_db_path("search-benchmark");
    let filler_count = std::env::var("CLIPMEM_SEARCH_BENCH_FILLERS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_FILLER_SNAPSHOTS);
    let ids = seed_benchmark_archive(&path, filler_count)?;
    let cases = eval_cases();
    assert_eq!(
        cases.len(),
        75,
        "search benchmark should cover 75 eval cases"
    );

    let mut outcomes = Vec::new();
    for case in &cases {
        outcomes.push(run_eval_case(&path, &ids, case)?);
    }
    let db_latency = measure_database_latency(&path)?;

    print_quality_report(filler_count, &outcomes);
    print_db_latency_report(&db_latency);

    cleanup_db(&path);
    Ok(())
}

fn seed_benchmark_archive(path: &Path, filler_count: usize) -> Result<HashMap<String, i64>> {
    let mut db = Database::open_or_init(path)?;
    let mut ids = HashMap::new();

    for index in 0..filler_count {
        let snapshot = app_text_snapshot(
            index as i64 + 1,
            filler_app_name(index),
            filler_bundle_id(index),
            &format!(
                "background clipboard filler {index:05} project notes lorem ipsum \
                 archive benchmark ordinary copied text"
            ),
        );
        let stored = db.store_capture(&snapshot)?;
        set_observed_at(
            path,
            stored.snapshot_id(),
            stored.event_id(),
            &format!("2026-05-30 09:{:02}:00", index % 60),
        )?;
    }

    let named = [
        (
            "git_status",
            app_text_snapshot(
                20_001,
                "Terminal",
                "com.apple.Terminal",
                "git status --short && git diff --stat",
            ),
            "2026-05-31 10:00:00",
        ),
        (
            "shell_commit",
            app_text_snapshot(
                20_002,
                "Terminal",
                "com.apple.Terminal",
                "git commit -m \"Fix search benchmark\" && git push",
            ),
            "2026-05-31 10:01:00",
        ),
        (
            "launchctl",
            app_text_snapshot(
                20_003,
                "Terminal",
                "com.apple.Terminal",
                "launchctl bootstrap gui/501 ~/Library/LaunchAgents/io.openclaw.clipmem.watch.plist",
            ),
            "2026-05-31 10:02:00",
        ),
        (
            "url_repo",
            app_rich_snapshot(
                20_004,
                "Safari",
                "com.apple.Safari",
                "Repository issue link copied for release triage",
                "https://example.com/repo/issues/42?label=clipmem",
                "file:///Users/test/Desktop/repo%20notes.txt",
            ),
            "2026-05-31 10:03:00",
        ),
        (
            "file_path",
            app_rich_snapshot(
                20_005,
                "Finder",
                "com.apple.finder",
                "Finder selected release plan document",
                "https://files.example.invalid/release",
                "file:///Users/test/Work/Release%20Plan.txt",
            ),
            "2026-05-31 10:04:00",
        ),
        (
            "percent_discount",
            app_text_snapshot(
                20_006,
                "TextEdit",
                "com.apple.TextEdit",
                "Discount: 50% off launch pricing",
            ),
            "2026-05-31 10:05:00",
        ),
        (
            "safari_deploy",
            app_text_snapshot(
                20_007,
                "Safari",
                "com.apple.Safari",
                "deploy checklist shared from safari tab",
            ),
            "2026-05-31 10:06:00",
        ),
        (
            "terminal_deploy",
            app_text_snapshot(
                20_008,
                "Terminal",
                "com.apple.Terminal",
                "deploy checklist shared from terminal session",
            ),
            "2026-05-31 10:07:00",
        ),
        (
            "terminal_bundle",
            app_text_snapshot(
                20_009,
                "Terminal",
                "com.apple.Terminal",
                "terminal bundle identifier marker qx73",
            ),
            "2026-05-31 10:08:00",
        ),
        (
            "long_noisy_anchor",
            app_text_snapshot(
                20_010,
                "Notes",
                "com.apple.Notes",
                &format!(
                    "{} needle-anchor-781 {}",
                    "needle anchor surrounding filler ".repeat(80),
                    "ordinary trailing text ".repeat(80)
                ),
            ),
            "2026-05-31 10:09:00",
        ),
        (
            "exact_anchor_short",
            app_text_snapshot(
                20_011,
                "Terminal",
                "com.apple.Terminal",
                "needle-anchor-781",
            ),
            "2026-05-31 10:10:00",
        ),
        (
            "ocr_invoice",
            image_snapshot(20_012, &[137, 80, 78, 71, 13, 10, 26, 10, 0, 1, 2, 3]),
            "2026-05-31 10:11:00",
        ),
        (
            "vague_recent",
            app_text_snapshot(
                20_013,
                "Notes",
                "com.apple.Notes",
                "Meeting notes from today with alpha bravo recap",
            ),
            "2026-05-31 10:12:00",
        ),
        (
            "git_status_long_distractor",
            app_text_snapshot(
                20_014,
                "Notes",
                "com.apple.Notes",
                &format!(
                    "{} dashboard notes about status pages and stash cleanup",
                    "git status ".repeat(40)
                ),
            ),
            "2026-05-31 10:13:00",
        ),
        (
            "release_plan_text_distractor",
            app_text_snapshot(
                20_015,
                "Notes",
                "com.apple.Notes",
                "Release Plan discussion without the Finder file URL payload",
            ),
            "2026-05-31 10:14:00",
        ),
        (
            "repo_url_distractor",
            app_rich_snapshot(
                20_016,
                "Safari",
                "com.apple.Safari",
                "Repository issue link copied for release triage duplicate",
                "https://example.com/repo/issues/100?label=archive",
                "file:///Users/test/Desktop/older%20repo%20notes.txt",
            ),
            "2026-05-31 10:15:00",
        ),
        (
            "ocr_invoice_native_distractor",
            app_text_snapshot(
                20_017,
                "Mail",
                "com.apple.mail",
                "Quarterly invoice approval text pasted from email body",
            ),
            "2026-05-31 10:16:00",
        ),
        (
            "terminal_bundle_newer_distractor",
            app_text_snapshot(
                20_018,
                "Terminal",
                "com.apple.Terminal",
                "terminal bundle identifier marker newest distractor",
            ),
            "2026-05-31 10:17:00",
        ),
        (
            "ambiguous_deploy_notes",
            app_text_snapshot(
                20_019,
                "Notes",
                "com.apple.Notes",
                "deploy checklist shared from notes with terminal and safari words",
            ),
            "2026-05-31 10:18:00",
        ),
        (
            "cargo_test_cmd",
            app_text_snapshot(
                20_020,
                "Ghostty",
                "com.mitchellh.ghostty",
                "cargo test --test cli_commands search -- --nocapture",
            ),
            "2026-05-31 10:19:00",
        ),
        (
            "cargo_test_distractor",
            app_text_snapshot(
                20_021,
                "Codex",
                "com.openai.codex",
                "cargo test cargo test cargo test notes about previous failing search run",
            ),
            "2026-05-31 10:20:00",
        ),
        (
            "pnpm_install_cmd",
            app_text_snapshot(
                20_022,
                "Ghostty",
                "com.mitchellh.ghostty",
                "pnpm i --frozen-lockfile && pnpm lint",
            ),
            "2026-05-31 10:21:00",
        ),
        (
            "ssh_remote_cmd",
            app_text_snapshot(
                20_023,
                "Ghostty",
                "com.mitchellh.ghostty",
                "ssh vivobook-ts 'cd /home/tristan/projects/tomojax && uv run pytest'",
            ),
            "2026-05-31 10:22:00",
        ),
        (
            "sql_query",
            app_text_snapshot(
                20_024,
                "TablePlus",
                "com.tinyapp.TablePlus",
                "SELECT snapshot_id, COUNT(*) FROM capture_events GROUP BY snapshot_id HAVING COUNT(*) > 1;",
            ),
            "2026-05-31 10:23:00",
        ),
        (
            "json_payload",
            app_text_snapshot(
                20_025,
                "Codex",
                "com.openai.codex",
                r#"{"event":"search_eval","status":"ok","top1":12,"top3":18}"#,
            ),
            "2026-05-31 10:24:00",
        ),
        (
            "env_path",
            app_text_snapshot(
                20_026,
                "Ghostty",
                "com.mitchellh.ghostty",
                "PATH=/opt/homebrew/bin:/usr/local/bin:/usr/bin CLIPMEM_SEARCH_BENCH_FILLERS=5000",
            ),
            "2026-05-31 10:25:00",
        ),
        (
            "uuid_trace",
            app_text_snapshot(
                20_027,
                "Google Chrome",
                "com.google.Chrome",
                "trace id 019e81d5-aaa7-76d2-99e5-6173fcac88a3 search latency spike",
            ),
            "2026-05-31 10:26:00",
        ),
        (
            "sha_digest",
            app_text_snapshot(
                20_028,
                "Codex",
                "com.openai.codex",
                "sha256 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
            ),
            "2026-05-31 10:27:00",
        ),
        (
            "markdown_link",
            app_text_snapshot(
                20_029,
                "Google Chrome",
                "com.google.Chrome",
                "[Search docs](https://docs.example.com/clipmem/searching-and-filtering#auto-mode)",
            ),
            "2026-05-31 10:28:00",
        ),
        (
            "error_stack",
            app_text_snapshot(
                20_030,
                "T3 Code (Alpha)",
                "com.todesktop.230313mzl4w4u92",
                "thread 'main' panicked at src/db/read/search.rs:187: invalid FTS syntax near ':'",
            ),
            "2026-05-31 10:29:00",
        ),
        (
            "docker_cmd",
            app_text_snapshot(
                20_031,
                "Ghostty",
                "com.mitchellh.ghostty",
                "docker compose run --rm app cargo test search_benchmark",
            ),
            "2026-05-31 10:30:00",
        ),
        (
            "brew_cmd",
            app_text_snapshot(
                20_032,
                "Ghostty",
                "com.mitchellh.ghostty",
                "brew tap-info tristanmanchester/tap && brew audit --formula clipmem",
            ),
            "2026-05-31 10:31:00",
        ),
        (
            "xcodebuild_cmd",
            app_text_snapshot(
                20_033,
                "Ghostty",
                "com.mitchellh.ghostty",
                "xcodebuild -project macos/ClipmemMenuBar/ClipmemMenuBar.xcodeproj -scheme ClipmemMenuBar test",
            ),
            "2026-05-31 10:32:00",
        ),
        (
            "rg_cmd",
            app_text_snapshot(
                20_034,
                "Ghostty",
                "com.mitchellh.ghostty",
                "rg -n \"search_auto|search_literal|search_fts\" src tests docs",
            ),
            "2026-05-31 10:33:00",
        ),
        (
            "invoice_payment_url",
            app_rich_snapshot(
                20_035,
                "Google Chrome",
                "com.google.Chrome",
                "Invoice payment portal copied from browser",
                "https://billing.example.com/invoices/2026-05-clipmem?status=open",
                "file:///Users/test/Downloads/invoice%202026-05.pdf",
            ),
            "2026-05-31 10:34:00",
        ),
        (
            "notion_url",
            app_rich_snapshot(
                20_036,
                "Google Chrome",
                "com.google.Chrome",
                "Notion search benchmark planning page",
                "https://notion.example.com/Search-Benchmark-Plan-abcdef123456",
                "file:///Users/test/Documents/Search%20Benchmark%20Plan.webloc",
            ),
            "2026-05-31 10:35:00",
        ),
        (
            "deep_file_path",
            app_rich_snapshot(
                20_037,
                "Finder",
                "com.apple.finder",
                "Selected local search benchmark fixture",
                "https://files.example.invalid/fixtures",
                "file:///Users/tristan/Projects/clipmem/tests/fixtures/Search%20Quality%20Matrix.csv",
            ),
            "2026-05-31 10:36:00",
        ),
        (
            "regex_snippet",
            app_text_snapshot(
                20_038,
                "Codex",
                "com.openai.codex",
                r#"Regex filter: ^(search|recall)_(auto|literal|fts)_[0-9]+$"#,
            ),
            "2026-05-31 10:37:00",
        ),
        (
            "email_subject",
            app_text_snapshot(
                20_039,
                "Mail",
                "com.apple.mail",
                "Subject: Search benchmark review - invoice portal and OCR results",
            ),
            "2026-05-31 10:38:00",
        ),
        (
            "calendar_text",
            app_text_snapshot(
                20_040,
                "Calendar",
                "com.apple.iCal",
                "Search quality review tomorrow 14:30 with browser archive examples",
            ),
            "2026-05-31 10:39:00",
        ),
        (
            "html_text",
            app_text_snapshot(
                20_041,
                "Google Chrome",
                "com.google.Chrome",
                "<article><h1>Search ranking notes</h1><p>literal mode handles URLs and file paths</p></article>",
            ),
            "2026-05-31 10:40:00",
        ),
        (
            "xml_text",
            app_text_snapshot(
                20_042,
                "T3 Code (Alpha)",
                "com.todesktop.230313mzl4w4u92",
                "<plist><key>Label</key><string>io.openclaw.clipmem.watch</string></plist>",
            ),
            "2026-05-31 10:41:00",
        ),
        (
            "clippy_output",
            app_text_snapshot(
                20_043,
                "Ghostty",
                "com.mitchellh.ghostty",
                "warning: this expression creates a reference which is immediately dereferenced",
            ),
            "2026-05-31 10:42:00",
        ),
        (
            "duplicate_recent",
            app_text_snapshot(
                20_044,
                "Google Chrome",
                "com.google.Chrome",
                "search quality review duplicate recent browser note",
            ),
            "2026-05-31 10:43:00",
        ),
        (
            "screenshot_settings",
            image_snapshot(20_045, &[137, 80, 78, 71, 13, 10, 26, 10, 4, 5, 6, 7]),
            "2026-05-31 10:44:00",
        ),
        (
            "screenshot_error",
            image_snapshot(20_046, &[137, 80, 78, 71, 13, 10, 26, 10, 8, 9, 10, 11]),
            "2026-05-31 10:45:00",
        ),
    ];

    for (key, snapshot, observed_at) in named {
        let stored = db.store_capture(&snapshot)?;
        set_observed_at(path, stored.snapshot_id(), stored.event_id(), observed_at)?;
        ids.insert(key.to_string(), stored.snapshot_id());
    }

    if let Some(snapshot_id) = ids.get("ocr_invoice").copied() {
        seed_ocr_text(
            path,
            snapshot_id,
            "Quarterly invoice approval screenshot alpha",
        )?;
    }
    if let Some(snapshot_id) = ids.get("screenshot_settings").copied() {
        seed_ocr_text(
            path,
            snapshot_id,
            "Settings screen shows OCR enabled and retention 30d",
        )?;
    }
    if let Some(snapshot_id) = ids.get("screenshot_error").copied() {
        seed_ocr_text(
            path,
            snapshot_id,
            "Error dialog: Full disk access required for clipboard watcher",
        )?;
    }

    Ok(ids)
}

fn temp_db_path(test_name: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();

    let dir = std::env::temp_dir()
        .join("clipmem-search-benchmark")
        .join(format!("{test_name}-{}-{timestamp}", process::id()));
    fs::create_dir_all(&dir).expect("temporary benchmark directory should be created");
    dir.join("database.sqlite3")
}

fn cleanup_db(path: &Path) {
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

fn app_text_snapshot(
    change_count: i64,
    app_name: &str,
    app_bundle_id: &str,
    text: &str,
) -> ClipboardSnapshot {
    let item = build_item(
        0,
        vec![build_representation(
            "public.utf8-plain-text".to_string(),
            None,
            text.as_bytes().to_vec(),
        )],
    );

    build_snapshot(
        CaptureContext::new(change_count)
            .with_frontmost_app_name(app_name)
            .with_frontmost_app_bundle_id(app_bundle_id),
        vec![item],
    )
}

fn app_rich_snapshot(
    change_count: i64,
    app_name: &str,
    app_bundle_id: &str,
    text: &str,
    url: &str,
    file_url: &str,
) -> ClipboardSnapshot {
    let item = build_item(
        0,
        vec![
            build_representation(
                "public.utf8-plain-text".to_string(),
                Some(text.to_string()),
                text.as_bytes().to_vec(),
            ),
            build_representation(
                "public.url".to_string(),
                Some(url.to_string()),
                url.as_bytes().to_vec(),
            ),
            build_representation(
                "public.file-url".to_string(),
                Some(file_url.to_string()),
                file_url.as_bytes().to_vec(),
            ),
        ],
    );

    build_snapshot(
        CaptureContext::new(change_count)
            .with_frontmost_app_name(app_name)
            .with_frontmost_app_bundle_id(app_bundle_id),
        vec![item],
    )
}

fn image_snapshot(change_count: i64, bytes: &[u8]) -> ClipboardSnapshot {
    let item = build_item(
        0,
        vec![build_representation(
            "public.png".to_string(),
            None,
            bytes.to_vec(),
        )],
    );

    build_snapshot(
        CaptureContext::new(change_count)
            .with_frontmost_app_name("Preview")
            .with_frontmost_app_bundle_id("com.apple.Preview"),
        vec![item],
    )
}

fn run_cli(args: &[&str]) -> process::Output {
    Command::new(env!("CARGO_BIN_EXE_clipmem"))
        .args(args)
        .output()
        .expect("clipmem binary should execute")
}

fn stdout_text(output: &process::Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout should be UTF-8")
}

fn stderr_text(output: &process::Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr should be UTF-8")
}

fn filler_app_name(index: usize) -> &'static str {
    match index % 5 {
        0 => "Terminal",
        1 => "Safari",
        2 => "Finder",
        3 => "TextEdit",
        _ => "Notes",
    }
}

fn filler_bundle_id(index: usize) -> &'static str {
    match index % 5 {
        0 => "com.example.TerminalFiller",
        1 => "com.example.SafariFiller",
        2 => "com.example.FinderFiller",
        3 => "com.example.TextEditFiller",
        _ => "com.example.NotesFiller",
    }
}

fn set_observed_at(path: &Path, snapshot_id: i64, event_id: i64, observed_at: &str) -> Result<()> {
    let conn = Connection::open(path)?;
    conn.execute(
        "UPDATE capture_events SET observed_at = ?1 WHERE id = ?2",
        (observed_at, event_id),
    )?;
    conn.execute(
        "UPDATE snapshot_stats
         SET first_observed_at = ?1, last_observed_at = ?1, last_event_id = ?2
         WHERE snapshot_id = ?3",
        (observed_at, event_id, snapshot_id),
    )?;
    Ok(())
}

fn seed_ocr_text(path: &Path, snapshot_id: i64, text: &str) -> Result<()> {
    let conn = Connection::open(path)?;
    conn.execute(
        "INSERT INTO snapshot_ocr_cache (snapshot_id, ocr_text, status, updated_at)
         VALUES (?1, ?2, 'ready', '2026-05-31 10:11:30')
         ON CONFLICT(snapshot_id) DO UPDATE SET
             ocr_text = excluded.ocr_text,
             status = excluded.status,
             updated_at = excluded.updated_at",
        (snapshot_id, text),
    )?;
    conn.execute(
        "UPDATE snapshot_search_documents
         SET ocr_text = ?2, has_ocr_text = 1
         WHERE snapshot_id = ?1",
        (snapshot_id, text),
    )?;
    conn.execute(
        "DELETE FROM snapshot_search_documents_fts WHERE rowid = ?1",
        [snapshot_id],
    )?;
    conn.execute(
        "INSERT INTO snapshot_search_documents_fts (
            rowid, native_text, preview_text, ocr_text, url_text,
            file_path_text, historical_app_names, historical_app_bundle_ids
         )
         SELECT snapshot_id, native_text, preview_text, ocr_text, url_text,
                file_path_text, historical_app_names, historical_app_bundle_ids
         FROM snapshot_search_documents WHERE snapshot_id = ?1",
        [snapshot_id],
    )?;
    conn.execute(
        "DELETE FROM snapshot_search_documents_literal_fts WHERE rowid = ?1",
        [snapshot_id],
    )?;
    conn.execute(
        "INSERT INTO snapshot_search_documents_literal_fts (rowid, haystack)
         SELECT snapshot_id, lower(
            native_text || char(31) || preview_text || char(31) || ocr_text ||
            char(31) || url_text || char(31) || file_path_text || char(31) ||
            historical_app_names || char(31) || historical_app_bundle_ids
         ) FROM snapshot_search_documents WHERE snapshot_id = ?1",
        [snapshot_id],
    )?;
    Ok(())
}

fn eval_cases() -> Vec<EvalCase> {
    vec![
        EvalCase {
            name: "recall_plain_git_status",
            command: RetrievalCommand::Recall,
            query: "git status",
            flags: &[],
            expected_key: "git_status",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "recall_shell_fragment_literal",
            command: RetrievalCommand::Recall,
            query: "git commit -m",
            flags: &[],
            expected_key: "shell_commit",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "search_boolean_fts",
            command: RetrievalCommand::Search,
            query: "\"launchctl\" AND bootstrap",
            flags: &["--mode", "fts"],
            expected_key: "launchctl",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "search_url_auto_literal",
            command: RetrievalCommand::Search,
            query: "https://example.com/repo",
            flags: &[],
            expected_key: "url_repo",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "search_file_path_auto_literal",
            command: RetrievalCommand::Search,
            query: "/Users/test/Work/Release Plan.txt",
            flags: &[],
            expected_key: "file_path",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "search_percent_literal",
            command: RetrievalCommand::Search,
            query: "50%",
            flags: &[],
            expected_key: "percent_discount",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "recall_prefer_app_terminal",
            command: RetrievalCommand::Recall,
            query: "deploy checklist",
            flags: &["--prefer-app", "terminal"],
            expected_key: "terminal_deploy",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "recall_weak_query_recent_fallback",
            command: RetrievalCommand::Recall,
            query: "unrelated query phrase",
            flags: &["--min-score", "0.95", "--prefer-recent"],
            expected_key: "screenshot_error",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "search_ocr_fts",
            command: RetrievalCommand::Search,
            query: "quarterly invoice screenshot",
            flags: &[],
            expected_key: "ocr_invoice",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "search_ocr_literal",
            command: RetrievalCommand::Search,
            query: "approval screenshot",
            flags: &["--mode", "literal"],
            expected_key: "ocr_invoice",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "recall_exact_anchor_beats_long_noise",
            command: RetrievalCommand::Recall,
            query: "needle-anchor-781",
            flags: &[],
            expected_key: "exact_anchor_short",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "search_bundle_id_returns_newest_match",
            command: RetrievalCommand::Search,
            query: "com.apple.Terminal",
            flags: &[],
            expected_key: "terminal_bundle_newer_distractor",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "search_kind_url_filter",
            command: RetrievalCommand::Search,
            query: "example.com/repo",
            flags: &["--kind", "url"],
            expected_key: "url_repo",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "search_has_file_url_filter",
            command: RetrievalCommand::Search,
            query: "Release Plan",
            flags: &["--has-file-url"],
            expected_key: "file_path",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "adversarial_git_exact_vs_repetition",
            command: RetrievalCommand::Recall,
            query: "git status --short",
            flags: &[],
            expected_key: "git_status",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "adversarial_file_url_vs_text",
            command: RetrievalCommand::Search,
            query: "Release Plan",
            flags: &[],
            expected_key: "file_path",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "adversarial_older_url_specificity",
            command: RetrievalCommand::Search,
            query: "https://example.com/repo/issues",
            flags: &[],
            expected_key: "url_repo",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "adversarial_ocr_vs_native_text",
            command: RetrievalCommand::Search,
            query: "quarterly invoice approval screenshot",
            flags: &[],
            expected_key: "ocr_invoice",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "adversarial_semantic_watcher_command",
            command: RetrievalCommand::Recall,
            query: "the command to start the watcher service",
            flags: &["--prefer-app", "terminal"],
            expected_key: "launchctl",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "adversarial_semantic_half_off",
            command: RetrievalCommand::Recall,
            query: "half off launch deal",
            flags: &[],
            expected_key: "percent_discount",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "adversarial_prefer_specific_terminal",
            command: RetrievalCommand::Recall,
            query: "deploy checklist terminal",
            flags: &[],
            expected_key: "terminal_deploy",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "realistic_cargo_cli_specific",
            command: RetrievalCommand::Search,
            query: "cargo test --test cli_commands search",
            flags: &[],
            expected_key: "cargo_test_cmd",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_recall_cargo_cli_specific",
            command: RetrievalCommand::Recall,
            query: "cargo test cli commands search",
            flags: &["--prefer-app", "ghostty"],
            expected_key: "cargo_test_cmd",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "adversarial_cargo_repetition",
            command: RetrievalCommand::Recall,
            query: "cargo test search",
            flags: &[],
            expected_key: "cargo_test_cmd",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "realistic_pnpm_frozen_lockfile",
            command: RetrievalCommand::Search,
            query: "pnpm i --frozen-lockfile",
            flags: &[],
            expected_key: "pnpm_install_cmd",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_ssh_remote",
            command: RetrievalCommand::Search,
            query: "ssh vivobook-ts",
            flags: &[],
            expected_key: "ssh_remote_cmd",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_recall_remote_pytest",
            command: RetrievalCommand::Recall,
            query: "remote tomojax pytest",
            flags: &["--prefer-app", "ghostty"],
            expected_key: "ssh_remote_cmd",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_sql_group_by",
            command: RetrievalCommand::Search,
            query: "GROUP BY snapshot_id",
            flags: &[],
            expected_key: "sql_query",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_sql_having_literal",
            command: RetrievalCommand::Search,
            query: "HAVING COUNT(*) > 1",
            flags: &["--mode", "literal"],
            expected_key: "sql_query",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_json_status",
            command: RetrievalCommand::Search,
            query: "search_eval status ok",
            flags: &[],
            expected_key: "json_payload",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_json_top3_literal",
            command: RetrievalCommand::Search,
            query: r#""top3":18"#,
            flags: &["--mode", "literal"],
            expected_key: "json_payload",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_env_fillers_literal",
            command: RetrievalCommand::Search,
            query: "CLIPMEM_SEARCH_BENCH_FILLERS=5000",
            flags: &[],
            expected_key: "env_path",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_env_path_homebrew",
            command: RetrievalCommand::Search,
            query: "/opt/homebrew/bin",
            flags: &[],
            expected_key: "env_path",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_uuid_trace",
            command: RetrievalCommand::Search,
            query: "019e81d5-aaa7-76d2-99e5-6173fcac88a3",
            flags: &[],
            expected_key: "uuid_trace",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_uuid_partial",
            command: RetrievalCommand::Search,
            query: "99e5-6173fcac88a3",
            flags: &[],
            expected_key: "uuid_trace",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_sha_digest",
            command: RetrievalCommand::Search,
            query: "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
            flags: &[],
            expected_key: "sha_digest",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "realistic_sha_prefix",
            command: RetrievalCommand::Search,
            query: "9f86d081884c7d65",
            flags: &[],
            expected_key: "sha_digest",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_markdown_link_url",
            command: RetrievalCommand::Search,
            query: "https://docs.example.com/clipmem/searching-and-filtering",
            flags: &[],
            expected_key: "markdown_link",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_recall_docs_auto_mode",
            command: RetrievalCommand::Recall,
            query: "docs auto mode searching filtering",
            flags: &[],
            expected_key: "markdown_link",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "realistic_error_stack_literal",
            command: RetrievalCommand::Search,
            query: "src/db/read/search.rs:187",
            flags: &[],
            expected_key: "error_stack",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_recall_invalid_fts_syntax",
            command: RetrievalCommand::Recall,
            query: "invalid FTS syntax",
            flags: &[],
            expected_key: "error_stack",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "realistic_docker_compose",
            command: RetrievalCommand::Search,
            query: "docker compose run --rm",
            flags: &[],
            expected_key: "docker_cmd",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_brew_audit",
            command: RetrievalCommand::Search,
            query: "brew audit --formula clipmem",
            flags: &[],
            expected_key: "brew_cmd",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_xcodebuild_scheme",
            command: RetrievalCommand::Search,
            query: "xcodebuild ClipmemMenuBar test",
            flags: &[],
            expected_key: "xcodebuild_cmd",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "realistic_rg_identifier_literal",
            command: RetrievalCommand::Search,
            query: "search_auto|search_literal|search_fts",
            flags: &["--mode", "literal"],
            expected_key: "rg_cmd",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_invoice_payment_url",
            command: RetrievalCommand::Search,
            query: "https://billing.example.com/invoices/2026-05-clipmem",
            flags: &[],
            expected_key: "invoice_payment_url",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_recall_invoice_portal_browser",
            command: RetrievalCommand::Recall,
            query: "invoice payment portal browser",
            flags: &[],
            expected_key: "invoice_payment_url",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "realistic_notion_url",
            command: RetrievalCommand::Search,
            query: "https://notion.example.com/Search-Benchmark-Plan",
            flags: &[],
            expected_key: "notion_url",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_recall_notion_plan",
            command: RetrievalCommand::Recall,
            query: "notion search benchmark planning page",
            flags: &[],
            expected_key: "notion_url",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "realistic_deep_file_path",
            command: RetrievalCommand::Search,
            query: "/Users/tristan/Projects/clipmem/tests/fixtures/Search Quality Matrix.csv",
            flags: &[],
            expected_key: "deep_file_path",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_csv_file_name",
            command: RetrievalCommand::Search,
            query: "Search Quality Matrix.csv",
            flags: &[],
            expected_key: "deep_file_path",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_regex_snippet",
            command: RetrievalCommand::Search,
            query: "search recall auto literal fts",
            flags: &[],
            expected_key: "regex_snippet",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "realistic_regex_prefix_literal",
            command: RetrievalCommand::Search,
            query: "^(search|recall)",
            flags: &["--mode", "literal"],
            expected_key: "regex_snippet",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_email_subject",
            command: RetrievalCommand::Search,
            query: "Search benchmark review invoice portal",
            flags: &[],
            expected_key: "email_subject",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "adversarial_recall_invoice_email_confusion",
            command: RetrievalCommand::Recall,
            query: "invoice portal OCR results",
            flags: &[],
            expected_key: "email_subject",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "realistic_calendar_time",
            command: RetrievalCommand::Search,
            query: "tomorrow 14:30",
            flags: &[],
            expected_key: "calendar_text",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_recall_meeting_tomorrow",
            command: RetrievalCommand::Recall,
            query: "search quality review tomorrow",
            flags: &[],
            expected_key: "calendar_text",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "realistic_html_tag_literal",
            command: RetrievalCommand::Search,
            query: "<h1>Search ranking notes</h1>",
            flags: &["--mode", "literal"],
            expected_key: "html_text",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_recall_html_urls_paths",
            command: RetrievalCommand::Recall,
            query: "literal mode handles URLs and file paths",
            flags: &[],
            expected_key: "html_text",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "realistic_xml_label_literal",
            command: RetrievalCommand::Search,
            query: "io.openclaw.clipmem.watch",
            flags: &[],
            expected_key: "xml_text",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_recall_launchagent_label",
            command: RetrievalCommand::Recall,
            query: "openclaw clipmem watch plist label",
            flags: &[],
            expected_key: "xml_text",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "realistic_clippy_warning",
            command: RetrievalCommand::Search,
            query: "immediately dereferenced",
            flags: &[],
            expected_key: "clippy_output",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "realistic_recall_reference_deref",
            command: RetrievalCommand::Recall,
            query: "reference immediately dereferenced warning",
            flags: &[],
            expected_key: "clippy_output",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "adversarial_duplicate_recent_browser",
            command: RetrievalCommand::Recall,
            query: "search quality review browser",
            flags: &["--prefer-recent"],
            expected_key: "duplicate_recent",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "realistic_ocr_settings",
            command: RetrievalCommand::Search,
            query: "OCR enabled retention 30d",
            flags: &[],
            expected_key: "screenshot_settings",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "realistic_ocr_error",
            command: RetrievalCommand::Search,
            query: "Full disk access required",
            flags: &[],
            expected_key: "screenshot_error",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "adversarial_recall_full_disk_access",
            command: RetrievalCommand::Recall,
            query: "clipboard watcher needs disk permission",
            flags: &[],
            expected_key: "screenshot_error",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_browser_archive_examples",
            command: RetrievalCommand::Search,
            query: "browser archive examples",
            flags: &[],
            expected_key: "calendar_text",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "adversarial_recall_browser_archive_examples",
            command: RetrievalCommand::Recall,
            query: "browser archive examples",
            flags: &[],
            expected_key: "calendar_text",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "realistic_code_app_filter_error",
            command: RetrievalCommand::Search,
            query: "invalid FTS syntax",
            flags: &["--app", "T3 Code"],
            expected_key: "error_stack",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "realistic_terminal_app_filter_cargo",
            command: RetrievalCommand::Search,
            query: "cargo test",
            flags: &["--app", "Ghostty"],
            expected_key: "cargo_test_cmd",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "realistic_chrome_app_filter_notion",
            command: RetrievalCommand::Search,
            query: "notion search benchmark",
            flags: &["--app", "Chrome"],
            expected_key: "notion_url",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "realistic_finder_app_filter_file",
            command: RetrievalCommand::Search,
            query: "Search Quality Matrix",
            flags: &["--app", "Finder"],
            expected_key: "deep_file_path",
            expected_mode: Some("literal"),
        },
        EvalCase {
            name: "realistic_has_url_invoice",
            command: RetrievalCommand::Search,
            query: "invoice",
            flags: &["--has-url"],
            expected_key: "invoice_payment_url",
            expected_mode: Some("fts"),
        },
        EvalCase {
            name: "realistic_has_file_url_invoice_pdf",
            command: RetrievalCommand::Search,
            query: "invoice 2026-05.pdf",
            flags: &["--has-file-url"],
            expected_key: "invoice_payment_url",
            expected_mode: Some("literal"),
        },
    ]
}

fn run_eval_case(path: &Path, ids: &HashMap<String, i64>, case: &EvalCase) -> Result<EvalOutcome> {
    let expected_id = *ids
        .get(case.expected_key)
        .unwrap_or_else(|| panic!("missing seeded id for {}", case.expected_key));
    let mut latencies = Vec::with_capacity(CLI_LATENCY_REPETITIONS);
    let mut final_payload = None;

    for _ in 0..CLI_LATENCY_REPETITIONS {
        let started = Instant::now();
        let payload = run_cli_json(path, case)?;
        latencies.push(started.elapsed());
        final_payload = Some(payload);
    }

    let payload = final_payload.expect("at least one benchmark repetition should run");
    let ranked_ids = ranked_snapshot_ids(&payload, case.command)?;
    let rank = ranked_ids
        .iter()
        .position(|snapshot_id| *snapshot_id == expected_id)
        .map(|index| index + 1);
    let top1_id = ranked_ids.first().copied();
    let mode_used = payload["applied_filters"]["mode_used"]
        .as_str()
        .map(ToOwned::to_owned);
    latencies.sort_unstable();

    Ok(EvalOutcome {
        name: case.name,
        command: case.command,
        expected_id,
        rank,
        top1_id,
        mode_used,
        expected_mode: case.expected_mode,
        median_cli_latency: percentile(&latencies, 0.50),
        p95_cli_latency: percentile(&latencies, 0.95),
    })
}

fn run_cli_json(path: &Path, case: &EvalCase) -> Result<Value> {
    let mut args = vec![
        "--db".to_string(),
        path.to_str().expect("db path should be UTF-8").to_string(),
        case.command.as_str().to_string(),
    ];
    args.extend(case.flags.iter().map(|flag| (*flag).to_string()));
    args.extend(["--format".to_string(), "json".to_string(), "--".to_string()]);
    args.push(case.query.to_string());

    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let output = run_cli(&arg_refs);
    if !output.status.success() {
        bail!(
            "benchmark command failed for {}: {}\nstdout:\n{}",
            case.name,
            stderr_text(&output),
            stdout_text(&output)
        );
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| anyhow::anyhow!("parse JSON for {}: {error}", case.name))
}

fn ranked_snapshot_ids(payload: &Value, command: RetrievalCommand) -> Result<Vec<i64>> {
    match command {
        RetrievalCommand::Recall => {
            let mut ids = Vec::new();
            if let Some(id) = payload["best_candidate"]["snapshot_id"].as_i64() {
                ids.push(id);
            }
            if let Some(alternatives) = payload["alternatives"].as_array() {
                ids.extend(
                    alternatives
                        .iter()
                        .filter_map(|candidate| candidate["snapshot_id"].as_i64()),
                );
            }
            Ok(ids)
        }
        RetrievalCommand::Search => payload["results"]
            .as_array()
            .map(|results| {
                results
                    .iter()
                    .filter_map(|row| row["snapshot_id"].as_i64())
                    .collect::<Vec<_>>()
            })
            .ok_or_else(|| anyhow::anyhow!("search payload did not include results array")),
    }
}

fn measure_database_latency(path: &Path) -> Result<Vec<DbLatencyOutcome>> {
    let db = Database::open_or_init(path)?;
    let filters = RetrievalFilters::default();
    let queries = [
        ("db_auto_plain", SearchMode::Auto, "git status"),
        ("db_auto_shell_literal", SearchMode::Auto, "git commit -m"),
        (
            "db_fts_boolean",
            SearchMode::Fts,
            "\"launchctl\" AND bootstrap",
        ),
        (
            "db_auto_file_path",
            SearchMode::Auto,
            "/Users/test/Work/Release Plan.txt",
        ),
        ("db_literal_percent", SearchMode::Literal, "50%"),
        ("db_auto_ocr", SearchMode::Auto, "quarterly invoice"),
    ];

    let mut outcomes = Vec::new();
    for (name, mode, query) in queries {
        let mut latencies = Vec::with_capacity(DB_LATENCY_REPETITIONS);
        let mut result_count = 0;
        for _ in 0..DB_LATENCY_REPETITIONS {
            let started = Instant::now();
            let results = match mode {
                SearchMode::Auto => db.search_auto(query, 10, &filters)?,
                SearchMode::Fts => db.search_fts(query, 10, &filters)?,
                SearchMode::Literal => db.search_literal(query, 10, &filters)?,
            };
            latencies.push(started.elapsed());
            result_count = black_box(results.hits().len());
        }
        latencies.sort_unstable();
        outcomes.push(DbLatencyOutcome {
            name,
            mode,
            median_latency: percentile(&latencies, 0.50),
            p95_latency: percentile(&latencies, 0.95),
            result_count,
        });
    }

    Ok(outcomes)
}

fn percentile(sorted: &[Duration], percentile: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let index = ((sorted.len() - 1) as f64 * percentile).ceil() as usize;
    sorted[index.min(sorted.len() - 1)]
}
