use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use clipmem::archive::Database;
use clipmem::capture::{
    build_item, build_representation, build_snapshot, CaptureContext, ClipboardSnapshot,
};
use rusqlite::Connection;
use serde_json::Value;

fn temp_db_path(test_name: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();

    std::env::temp_dir()
        .join("clipmem-cli-tests")
        .join(format!("{test_name}-{}-{timestamp}.sqlite3", process::id()))
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

fn text_snapshot(change_count: i64, text: &str) -> ClipboardSnapshot {
    app_text_snapshot(change_count, "Terminal", "com.apple.Terminal", text)
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

fn rich_snapshot(change_count: i64, text: &str, url: &str, file_url: &str) -> ClipboardSnapshot {
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
            .with_frontmost_app_name("Terminal")
            .with_frontmost_app_bundle_id("com.apple.Terminal"),
        vec![item],
    )
}

fn html_snapshot(change_count: i64, html: &str) -> ClipboardSnapshot {
    let item = build_item(
        0,
        vec![build_representation(
            "public.html".to_string(),
            Some(html.to_string()),
            html.as_bytes().to_vec(),
        )],
    );

    build_snapshot(
        CaptureContext::new(change_count)
            .with_frontmost_app_name("Safari")
            .with_frontmost_app_bundle_id("com.apple.Safari"),
        vec![item],
    )
}

fn rtf_snapshot(change_count: i64, rtf: &str) -> ClipboardSnapshot {
    let item = build_item(
        0,
        vec![build_representation(
            "public.rtf".to_string(),
            Some(rtf.to_string()),
            rtf.as_bytes().to_vec(),
        )],
    );

    build_snapshot(
        CaptureContext::new(change_count)
            .with_frontmost_app_name("TextEdit")
            .with_frontmost_app_bundle_id("com.apple.TextEdit"),
        vec![item],
    )
}

fn seed_database(path: &Path, snapshots: &[ClipboardSnapshot]) -> Result<Vec<i64>> {
    let mut db = Database::open_or_init(path)?;
    let mut ids = Vec::new();

    for snapshot in snapshots {
        let stored = db.store_capture(snapshot)?;
        ids.push(stored.snapshot_id());
    }

    Ok(ids)
}

fn seed_events(path: &Path, snapshots: &[ClipboardSnapshot]) -> Result<Vec<(i64, i64)>> {
    let mut db = Database::open_or_init(path)?;
    let mut ids = Vec::new();

    for snapshot in snapshots {
        let stored = db.store_capture(snapshot)?;
        ids.push((stored.snapshot_id(), stored.event_id()));
    }

    Ok(ids)
}

fn set_event_observed_at(path: &Path, event_id: i64, observed_at: &str) -> Result<()> {
    let conn = Connection::open(path)?;
    conn.execute(
        "UPDATE capture_events SET observed_at = ?1 WHERE id = ?2",
        (observed_at, event_id),
    )?;
    Ok(())
}

fn run_cli(args: &[&str]) -> process::Output {
    Command::new(env!("CARGO_BIN_EXE_clipmem"))
        .args(args)
        .output()
        .expect("clipmem binary should execute")
}

fn run_cli_with_env(args: &[&str], envs: &[(&str, &str)]) -> process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_clipmem"));
    command.args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    command
        .output()
        .expect("clipmem binary should execute with env")
}

fn stdout_text(output: &process::Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout should be UTF-8")
}

fn stderr_text(output: &process::Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr should be UTF-8")
}

fn temp_test_dir(test_name: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();

    std::env::temp_dir()
        .join("clipmem-agent-tests")
        .join(format!("{test_name}-{}-{timestamp}", process::id()))
}

#[cfg(unix)]
fn write_executable(path: &Path, content: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::write(path, content)?;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[test]
fn search_command_prints_literal_results_in_text_mode() -> Result<()> {
    let path = temp_db_path("search-text");
    seed_database(
        &path,
        &[
            text_snapshot(1, "Discount: 50 percent off"),
            text_snapshot(2, "Discount: 50%"),
        ],
    )?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "search",
        "--mode",
        "literal",
        "50%",
    ]);
    let stdout = stdout_text(&output);

    assert!(output.status.success());
    assert!(stdout.contains("preview: Discount: 50%"));
    assert!(!stdout.contains("Discount: 50 percent off"));

    cleanup_db(&path);
    Ok(())
}

#[test]
fn search_json_envelope_includes_agent_friendly_rows() -> Result<()> {
    let path = temp_db_path("search-json");
    let ids = seed_database(
        &path,
        &[rich_snapshot(
            1,
            "git status",
            "https://example.com/repo",
            "file:///Users/test/report.txt",
        )],
    )?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "search",
        "--mode",
        "literal",
        "--format",
        "json",
        "git",
    ]);
    let payload: Value =
        serde_json::from_slice(&output.stdout).expect("search JSON output should parse");

    assert!(output.status.success());
    assert_eq!(payload["schema_version"].as_u64(), Some(1));
    assert_eq!(payload["command"].as_str(), Some("search"));
    assert_eq!(payload["applied_filters"]["requested_mode"].as_str(), Some("literal"));
    assert_eq!(payload["applied_filters"]["mode_used"].as_str(), Some("literal"));
    assert_eq!(payload["truncated"].as_bool(), Some(false));
    assert_eq!(payload["results"][0]["snapshot_id"].as_i64(), Some(ids[0]));
    assert_eq!(payload["results"][0]["event_id"].as_i64(), Some(1));
    assert!(payload["results"][0]["best_text"]
        .as_str()
        .unwrap_or_default()
        .contains("git status"));
    assert_eq!(
        payload["results"][0]["best_text_uti"].as_str(),
        Some("public.utf8-plain-text")
    );
    let fragments = payload["results"][0]["text_fragments"]
        .as_array()
        .expect("text_fragments should be an array");
    assert!(fragments.iter().any(|fragment| {
        fragment["text"].as_str() == Some("git status")
            && fragment["uti"].as_str() == Some("public.utf8-plain-text")
    }));
    assert_eq!(
        payload["results"][0]["why_matched"].as_str(),
        Some("git status  https://example.com/repo  file:///Users/test/report.txt")
    );
    assert_eq!(
        payload["results"][0]["urls"][0].as_str(),
        Some("https://example.com/repo")
    );
    assert_eq!(
        payload["results"][0]["file_paths"][0].as_str(),
        Some("/Users/test/report.txt")
    );
    assert!(payload["results"][0]["text_summary"]
        .as_str()
        .unwrap_or_default()
        .contains("git status"));

    cleanup_db(&path);
    Ok(())
}

#[test]
fn recent_json_alias_uses_new_envelope_shape() -> Result<()> {
    let path = temp_db_path("recent-json");
    let ids = seed_database(&path, &[text_snapshot(1, "git status")])?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "recent",
        "--json",
    ]);
    let payload: Value =
        serde_json::from_slice(&output.stdout).expect("recent JSON output should parse");

    assert!(output.status.success());
    assert_eq!(payload["command"].as_str(), Some("recent"));
    assert_eq!(payload["results"].as_array().map(Vec::len), Some(1));
    assert_eq!(payload["results"][0]["snapshot_id"].as_i64(), Some(ids[0]));
    assert_eq!(payload["results"][0]["best_text"].as_str(), Some("git status"));
    assert_eq!(
        payload["results"][0]["best_text_uti"].as_str(),
        Some("public.utf8-plain-text")
    );
    assert!(payload["results"][0]["why_matched"].is_null());

    cleanup_db(&path);
    Ok(())
}

#[test]
fn timeline_json_envelope_returns_event_rows_in_descending_order() -> Result<()> {
    let path = temp_db_path("timeline-json");
    let events = seed_events(
        &path,
        &[
            text_snapshot(1, "git status"),
            text_snapshot(2, "git status"),
            rich_snapshot(
                3,
                "cargo test",
                "https://example.com/repo",
                "file:///Users/test/report.txt",
            ),
        ],
    )?;
    set_event_observed_at(&path, events[0].1, "2026-04-16 09:00:00")?;
    set_event_observed_at(&path, events[1].1, "2026-04-16 10:00:00")?;
    set_event_observed_at(&path, events[2].1, "2026-04-16 11:00:00")?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "timeline",
        "--format",
        "json",
        "--limit",
        "3",
    ]);
    let payload: Value =
        serde_json::from_slice(&output.stdout).expect("timeline JSON output should parse");

    assert!(output.status.success());
    assert_eq!(payload["command"].as_str(), Some("timeline"));
    assert_eq!(payload["applied_filters"]["sort"].as_str(), Some("desc"));
    assert_eq!(payload["results"].as_array().map(Vec::len), Some(3));
    assert_eq!(payload["results"][0]["event_id"].as_i64(), Some(events[2].1));
    assert_eq!(payload["results"][1]["event_id"].as_i64(), Some(events[1].1));
    assert_eq!(payload["results"][2]["event_id"].as_i64(), Some(events[0].1));
    assert_eq!(payload["results"][0]["change_count"].as_i64(), Some(3));
    assert_eq!(payload["results"][0]["best_text"].as_str(), Some("cargo test"));
    assert_eq!(
        payload["results"][0]["best_text_uti"].as_str(),
        Some("public.utf8-plain-text")
    );
    assert_eq!(
        payload["results"][0]["urls"][0].as_str(),
        Some("https://example.com/repo")
    );
    assert_eq!(
        payload["results"][0]["file_paths"][0].as_str(),
        Some("/Users/test/report.txt")
    );

    cleanup_db(&path);
    Ok(())
}

#[test]
fn timeline_paginates_with_cursor_and_sort_asc() -> Result<()> {
    let path = temp_db_path("timeline-pagination");
    let events = seed_events(
        &path,
        &[
            text_snapshot(1, "alpha"),
            text_snapshot(2, "beta"),
            text_snapshot(3, "gamma"),
        ],
    )?;
    set_event_observed_at(&path, events[0].1, "2026-04-16 09:00:00")?;
    set_event_observed_at(&path, events[1].1, "2026-04-16 10:00:00")?;
    set_event_observed_at(&path, events[2].1, "2026-04-16 11:00:00")?;

    let first_output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "timeline",
        "--sort",
        "asc",
        "--limit",
        "2",
        "--format",
        "json",
    ]);
    let first_payload: Value =
        serde_json::from_slice(&first_output.stdout).expect("first timeline page should parse");
    let next_cursor = first_payload["next_cursor"]
        .as_str()
        .expect("first timeline page should include a cursor")
        .to_string();

    assert!(first_output.status.success());
    assert_eq!(first_payload["truncated"].as_bool(), Some(true));
    assert_eq!(first_payload["results"][0]["event_id"].as_i64(), Some(events[0].1));
    assert_eq!(first_payload["results"][1]["event_id"].as_i64(), Some(events[1].1));

    let second_output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "timeline",
        "--sort",
        "asc",
        "--limit",
        "2",
        "--cursor",
        &next_cursor,
        "--format",
        "json",
    ]);
    let second_payload: Value =
        serde_json::from_slice(&second_output.stdout).expect("second timeline page should parse");

    assert!(second_output.status.success());
    assert_eq!(second_payload["truncated"].as_bool(), Some(false));
    assert!(second_payload["next_cursor"].is_null());
    assert_eq!(second_payload["results"].as_array().map(Vec::len), Some(1));
    assert_eq!(second_payload["results"][0]["event_id"].as_i64(), Some(events[2].1));

    cleanup_db(&path);
    Ok(())
}

#[test]
fn search_filters_are_combined_and_echoed_in_structured_output() -> Result<()> {
    let path = temp_db_path("search-shared-filters");
    seed_database(
        &path,
        &[
            rich_snapshot(
                1,
                "git clone https://example.com/repo",
                "https://example.com/repo",
                "file:///Users/test/report.txt",
            ),
            app_text_snapshot(2, "Preview", "com.apple.Preview", "git clone local mirror"),
        ],
    )?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "search",
        "--mode",
        "literal",
        "--app",
        "terminal",
        "--kind",
        "url",
        "--has-url",
        "--min-bytes",
        "20",
        "--format",
        "json",
        "example.com",
    ]);
    let payload: Value = serde_json::from_slice(&output.stdout).expect("search JSON should parse");

    assert!(output.status.success());
    assert_eq!(payload["results"].as_array().map(Vec::len), Some(1));
    assert_eq!(payload["applied_filters"]["app"].as_str(), Some("terminal"));
    assert_eq!(payload["applied_filters"]["kind"].as_str(), Some("url"));
    assert_eq!(payload["applied_filters"]["has_url"].as_bool(), Some(true));
    assert_eq!(payload["applied_filters"]["min_bytes"].as_u64(), Some(20));

    cleanup_db(&path);
    Ok(())
}

#[test]
fn recall_respects_shared_filters() -> Result<()> {
    let path = temp_db_path("recall-shared-filters");
    seed_database(
        &path,
        &[
            app_text_snapshot(1, "Terminal", "com.apple.Terminal", "deploy checklist"),
            app_text_snapshot(2, "Preview", "com.apple.Preview", "invoice draft"),
        ],
    )?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "recall",
        "--prefer-recent",
        "--app",
        "preview",
        "--format",
        "json",
    ]);
    let payload: Value = serde_json::from_slice(&output.stdout).expect("recall JSON should parse");

    assert!(output.status.success());
    assert_eq!(
        payload["best_candidate"]["app_name"].as_str(),
        Some("Preview")
    );
    assert_eq!(payload["applied_filters"]["app"].as_str(), Some("preview"));

    cleanup_db(&path);
    Ok(())
}

#[test]
fn get_and_export_use_shared_filters_as_guards() -> Result<()> {
    let path = temp_db_path("get-export-guards");
    let ids = seed_database(
        &path,
        &[rich_snapshot(
            1,
            "git status",
            "https://example.com/repo",
            "file:///Users/test/report.txt",
        )],
    )?;

    let get_output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "get",
        &ids[0].to_string(),
        "--app",
        "terminal",
        "--has-url",
        "--format",
        "json",
    ]);
    let get_payload: Value = serde_json::from_slice(&get_output.stdout).expect("get JSON should parse");
    assert!(get_output.status.success());
    assert_eq!(get_payload["applied_filters"]["app"].as_str(), Some("terminal"));
    assert_eq!(get_payload["applied_filters"]["has_url"].as_bool(), Some(true));

    let rejected_get = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "get",
        &ids[0].to_string(),
        "--app",
        "preview",
    ]);
    assert!(!rejected_get.status.success());
    assert!(stderr_text(&rejected_get).contains("does not satisfy the active filters"));

    let output_path = std::env::temp_dir()
        .join(format!("clipmem-filter-export-{}-{}.txt", process::id(), 1));
    let rejected_export = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "export",
        &ids[0].to_string(),
        "--item",
        "0",
        "--uti",
        "public.utf8-plain-text",
        "--out",
        output_path.to_str().expect("export path should be UTF-8"),
        "--app",
        "preview",
    ]);
    assert!(!rejected_export.status.success());
    assert!(stderr_text(&rejected_export).contains("does not satisfy the active filters"));
    let _ = fs::remove_file(&output_path);

    cleanup_db(&path);
    Ok(())
}

#[test]
fn get_json_wraps_snapshot_in_versioned_envelope() -> Result<()> {
    let path = temp_db_path("get-json");
    let ids = seed_database(
        &path,
        &[text_snapshot(1, "git clone https://example.com/repo")],
    )?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "get",
        &ids[0].to_string(),
        "--format",
        "json",
    ]);
    let payload: Value = serde_json::from_slice(&output.stdout).expect("get JSON should parse");

    assert!(output.status.success());
    assert_eq!(payload["schema_version"].as_u64(), Some(1));
    assert_eq!(payload["command"].as_str(), Some("get"));
    assert_eq!(payload["snapshot"]["snapshot_id"].as_i64(), Some(ids[0]));
    assert_eq!(
        payload["snapshot"]["best_text"].as_str(),
        Some("git clone https://example.com/repo")
    );
    assert_eq!(
        payload["snapshot"]["best_text_uti"].as_str(),
        Some("public.utf8-plain-text")
    );
    assert_eq!(
        payload["snapshot"]["text_fragments"][0]["text"].as_str(),
        Some("git clone https://example.com/repo")
    );
    assert!(payload["snapshot"]["urls"]
        .as_array()
        .expect("urls should be an array")
        .is_empty());
    assert!(payload["snapshot"]["file_paths"]
        .as_array()
        .expect("file_paths should be an array")
        .is_empty());
    assert!(payload["snapshot"]["text_summary"]
        .as_str()
        .unwrap_or_default()
        .contains("git clone https://example.com/repo"));
    assert_eq!(
        payload["snapshot"]["preview_text"].as_str(),
        Some("git clone https://example.com/repo")
    );

    cleanup_db(&path);
    Ok(())
}

#[test]
fn jsonl_output_emits_meta_and_result_records() -> Result<()> {
    let path = temp_db_path("search-jsonl");
    seed_database(&path, &[text_snapshot(1, "git status")])?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "search",
        "--mode",
        "literal",
        "--format",
        "jsonl",
        "git",
    ]);
    let stdout = stdout_text(&output);
    let lines = stdout.lines().collect::<Vec<_>>();
    let meta: Value = serde_json::from_str(lines[0]).expect("meta JSONL line should parse");
    let row: Value = serde_json::from_str(lines[1]).expect("result JSONL line should parse");

    assert!(output.status.success());
    assert_eq!(lines.len(), 2);
    assert_eq!(meta["type"].as_str(), Some("meta"));
    assert_eq!(row["type"].as_str(), Some("result"));
    assert_eq!(row["best_text"].as_str(), Some("git status"));

    cleanup_db(&path);
    Ok(())
}

#[test]
fn markdown_output_is_compact_and_scannable() -> Result<()> {
    let path = temp_db_path("search-md");
    seed_database(&path, &[text_snapshot(1, "git status")])?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "search",
        "--format",
        "md",
        "git",
    ]);
    let stdout = stdout_text(&output);

    assert!(output.status.success());
    assert!(stdout.contains("# clipmem search"));
    assert!(stdout.contains("| snapshot_id | kind | observed_at |"));

    cleanup_db(&path);
    Ok(())
}

#[test]
fn toon_output_is_available_for_flattened_list_commands() -> Result<()> {
    let path = temp_db_path("recent-toon");
    seed_database(&path, &[text_snapshot(1, "git status")])?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "recent",
        "--format",
        "toon",
    ]);
    let stdout = stdout_text(&output);

    assert!(output.status.success());
    assert!(stdout.contains("results[#1\t]{snapshot_id"));
    assert!(stdout.contains("git status"));

    cleanup_db(&path);
    Ok(())
}

#[test]
fn get_rejects_toon_output() -> Result<()> {
    let path = temp_db_path("get-toon");
    let ids = seed_database(&path, &[text_snapshot(1, "git status")])?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "get",
        &ids[0].to_string(),
        "--format",
        "toon",
    ]);
    let stderr = stderr_text(&output);

    assert!(!output.status.success());
    assert!(stderr.contains("TOON is only available for flattened list output"));

    cleanup_db(&path);
    Ok(())
}

#[test]
fn agents_openclaw_install_print_and_uninstall_skill_work() -> Result<()> {
    let test_dir = temp_test_dir("openclaw-install");
    let bin_dir = test_dir.join("bin");
    let workspace_dir = test_dir.join("workspace");
    let install_dir = workspace_dir.join("skills").join("clipboard_memory");
    fs::create_dir_all(&bin_dir)?;
    fs::create_dir_all(&workspace_dir)?;

    let openclaw_path = bin_dir.join("openclaw");
    write_executable(
        &openclaw_path,
        &format!(
            "#!/bin/sh\nif [ \"$1\" = \"config\" ] && [ \"$2\" = \"get\" ] && [ \"$3\" = \"agents.defaults.workspace\" ]; then\n  printf '%s\\n' '{}'\n  exit 0\nfi\nif [ \"$1\" = \"sandbox\" ] && [ \"$2\" = \"explain\" ]; then\n  printf 'sandbox disabled\\n'\n  exit 0\nfi\nexit 1\n",
            workspace_dir.display()
        ),
    )?;

    let clipmem_link = bin_dir.join("clipmem");
    #[cfg(unix)]
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_clipmem"), &clipmem_link)?;

    let path_value = bin_dir.display().to_string();

    let install = run_cli_with_env(
        &["agents", "openclaw", "install-skill"],
        &[("PATH", &path_value), ("HOME", test_dir.to_str().unwrap())],
    );
    assert!(install.status.success());
    assert!(install_dir.join("SKILL.md").is_file());

    let printed = run_cli_with_env(
        &["agents", "openclaw", "print-skill"],
        &[("PATH", &path_value), ("HOME", test_dir.to_str().unwrap())],
    );
    assert!(printed.status.success());
    assert_eq!(
        stdout_text(&printed),
        fs::read_to_string(install_dir.join("SKILL.md"))?
    );

    let uninstall = run_cli_with_env(
        &["agents", "openclaw", "uninstall-skill"],
        &[("PATH", &path_value), ("HOME", test_dir.to_str().unwrap())],
    );
    assert!(uninstall.status.success());
    assert!(!install_dir.exists());

    let _ = fs::remove_dir_all(&test_dir);
    Ok(())
}

#[test]
fn agents_openclaw_doctor_reports_missing_clipmem_with_next_steps() -> Result<()> {
    let test_dir = temp_test_dir("openclaw-doctor-missing-clipmem");
    let bin_dir = test_dir.join("bin");
    let workspace_dir = test_dir.join("workspace");
    let skill_dir = workspace_dir.join("skills").join("clipboard_memory");
    fs::create_dir_all(&bin_dir)?;
    fs::create_dir_all(&skill_dir)?;
    fs::write(
        skill_dir.join("SKILL.md"),
        include_str!("../extras/openclaw/clipboard_memory/SKILL.md"),
    )?;

    let openclaw_path = bin_dir.join("openclaw");
    write_executable(
        &openclaw_path,
        &format!(
            "#!/bin/sh\nif [ \"$1\" = \"config\" ] && [ \"$2\" = \"get\" ] && [ \"$3\" = \"agents.defaults.workspace\" ]; then\n  printf '%s\\n' '{}'\n  exit 0\nfi\nif [ \"$1\" = \"sandbox\" ] && [ \"$2\" = \"explain\" ]; then\n  printf 'sandbox disabled\\n'\n  exit 0\nfi\nexit 1\n",
            workspace_dir.display()
        ),
    )?;

    let output = run_cli_with_env(
        &["agents", "openclaw", "doctor"],
        &[("PATH", bin_dir.to_str().unwrap()), ("HOME", test_dir.to_str().unwrap())],
    );
    assert!(!output.status.success());
    let stdout = stdout_text(&output);
    assert!(stdout.contains("[FAIL] Host clipmem on PATH"));
    assert!(stdout.contains("brew install tristanmanchester/tap/clipmem"));

    let _ = fs::remove_dir_all(&test_dir);
    Ok(())
}

#[test]
fn recall_json_prefers_a_strong_query_match() -> Result<()> {
    let path = temp_db_path("recall-strong-query");
    let ids = seed_database(
        &path,
        &[
            text_snapshot(1, "git status"),
            text_snapshot(2, "cargo test"),
            text_snapshot(3, "git commit"),
        ],
    )?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "recall",
        "git status",
        "--format",
        "json",
    ]);
    let payload: Value =
        serde_json::from_slice(&output.stdout).expect("recall JSON output should parse");

    assert!(output.status.success());
    assert_eq!(payload["command"].as_str(), Some("recall"));
    assert_eq!(payload["query"].as_str(), Some("git status"));
    assert_eq!(payload["best_candidate"]["snapshot_id"].as_i64(), Some(ids[0]));
    assert_eq!(
        payload["best_candidate"]["best_text"].as_str(),
        Some("git status")
    );
    assert_eq!(
        payload["best_candidate"]["best_text_uti"].as_str(),
        Some("public.utf8-plain-text")
    );
    assert_eq!(
        payload["best_candidate"]["text_fragments"][0]["text"].as_str(),
        Some("git status")
    );
    assert_eq!(
        payload["best_match_confidence"].as_str(),
        Some("high")
    );
    assert!(payload["why_selected"]
        .as_str()
        .unwrap_or_default()
        .contains("strongest search match"));

    cleanup_db(&path);
    Ok(())
}

#[test]
fn recall_json_falls_back_to_recent_when_search_is_weak() -> Result<()> {
    let path = temp_db_path("recall-weak-search");
    let ids = seed_database(
        &path,
        &[
            text_snapshot(1, "git status"),
            app_text_snapshot(2, "Preview", "com.apple.Preview", "Meeting notes from today"),
        ],
    )?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "recall",
        "git",
        "--mode",
        "literal",
        "--format",
        "json",
        "--min-score",
        "0.95",
        "--prefer-recent",
    ]);
    let payload: Value =
        serde_json::from_slice(&output.stdout).expect("recall fallback JSON should parse");

    assert!(output.status.success());
    assert_eq!(payload["best_candidate"]["snapshot_id"].as_i64(), Some(ids[1]));
    assert!(payload["why_selected"]
        .as_str()
        .unwrap_or_default()
        .contains("Fell back to recent clipboard items"));

    cleanup_db(&path);
    Ok(())
}

#[test]
fn recall_without_query_returns_recent_candidates() -> Result<()> {
    let path = temp_db_path("recall-no-query");
    let ids = seed_database(
        &path,
        &[
            text_snapshot(1, "older text"),
            text_snapshot(2, "newest text"),
        ],
    )?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "recall",
        "--format",
        "json",
    ]);
    let payload: Value =
        serde_json::from_slice(&output.stdout).expect("recall no-query JSON should parse");

    assert!(output.status.success());
    assert!(payload["query"].is_null());
    assert_eq!(payload["best_candidate"]["snapshot_id"].as_i64(), Some(ids[1]));
    assert_eq!(
        payload["best_candidate"]["best_text"].as_str(),
        Some("newest text")
    );
    assert!(payload["why_selected"]
        .as_str()
        .unwrap_or_default()
        .contains("most likely useful recent clipboard item"));

    cleanup_db(&path);
    Ok(())
}

#[test]
fn get_json_exposes_html_and_rtf_plain_text_projections() -> Result<()> {
    let path = temp_db_path("get-rich-text-projections");
    let ids = seed_database(
        &path,
        &[
            html_snapshot(1, "<p>Hello <strong>world</strong></p>"),
            rtf_snapshot(2, r"{\rtf1\ansi hello\par world}"),
        ],
    )?;

    let html_output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "get",
        &ids[0].to_string(),
        "--format",
        "json",
    ]);
    let html_payload: Value =
        serde_json::from_slice(&html_output.stdout).expect("html get JSON should parse");
    assert!(html_output.status.success());
    assert_eq!(html_payload["snapshot"]["html_text"].as_str(), Some("Hello world"));
    assert_eq!(
        html_payload["snapshot"]["best_text"].as_str(),
        Some("Hello world")
    );

    let rtf_output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "get",
        &ids[1].to_string(),
        "--format",
        "json",
    ]);
    let rtf_payload: Value =
        serde_json::from_slice(&rtf_output.stdout).expect("rtf get JSON should parse");
    assert!(rtf_output.status.success());
    assert_eq!(rtf_payload["snapshot"]["rtf_text"].as_str(), Some("hello world"));
    assert_eq!(
        rtf_payload["snapshot"]["best_text"].as_str(),
        Some("hello world")
    );

    cleanup_db(&path);
    Ok(())
}

#[test]
fn recall_prefer_app_boosts_matching_candidates() -> Result<()> {
    let path = temp_db_path("recall-prefer-app");
    let ids = seed_database(
        &path,
        &[
            app_text_snapshot(1, "Terminal", "com.apple.Terminal", "deploy checklist"),
            app_text_snapshot(2, "Preview", "com.apple.Preview", "invoice draft"),
        ],
    )?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "recall",
        "--format",
        "json",
        "--prefer-app",
        "terminal",
    ]);
    let payload: Value =
        serde_json::from_slice(&output.stdout).expect("recall prefer-app JSON should parse");

    assert!(output.status.success());
    assert_eq!(payload["best_candidate"]["snapshot_id"].as_i64(), Some(ids[0]));
    assert!(payload["why_selected"]
        .as_str()
        .unwrap_or_default()
        .contains("preferred app"));

    cleanup_db(&path);
    Ok(())
}

#[test]
fn recall_markdown_quotes_and_expands_best_text() -> Result<()> {
    let path = temp_db_path("recall-md-quote");
    let long_text = "git status --short && git log --oneline && cargo test --package clipmem";
    seed_database(&path, &[text_snapshot(1, long_text)])?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "recall",
        "git status",
        "--full",
        "--quote",
    ]);
    let stdout = stdout_text(&output);

    assert!(output.status.success());
    assert!(stdout.contains("# Best Match"));
    assert!(stdout.contains("> git status"));
    assert!(stdout.contains("Why This Match"));
    assert!(stdout.contains("Alternatives") || !stdout.contains("## Alternatives"));

    cleanup_db(&path);
    Ok(())
}

#[test]
fn recall_toon_output_is_flattened() -> Result<()> {
    let path = temp_db_path("recall-toon");
    seed_database(&path, &[text_snapshot(1, "git status")])?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "recall",
        "git status",
        "--format",
        "toon",
    ]);
    let stdout = stdout_text(&output);

    assert!(output.status.success());
    assert!(stdout.contains("best_candidate[#1"));
    assert!(stdout.contains("alternatives[#0"));

    cleanup_db(&path);
    Ok(())
}

#[test]
fn list_commands_include_truncation_metadata_and_resume_with_cursor() -> Result<()> {
    let path = temp_db_path("search-pagination");
    seed_database(
        &path,
        &[
            text_snapshot(1, "git one"),
            text_snapshot(2, "git two"),
            text_snapshot(3, "git three"),
        ],
    )?;

    let first_output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "search",
        "--mode",
        "literal",
        "--limit",
        "2",
        "--format",
        "json",
        "git",
    ]);
    let first_payload: Value =
        serde_json::from_slice(&first_output.stdout).expect("first page should parse");
    let next_cursor = first_payload["next_cursor"]
        .as_str()
        .expect("first page should include a cursor")
        .to_string();
    let first_ids = first_payload["results"]
        .as_array()
        .expect("results should be an array")
        .iter()
        .map(|row| row["snapshot_id"].as_i64().unwrap())
        .collect::<Vec<_>>();

    assert!(first_output.status.success());
    assert_eq!(first_payload["truncated"].as_bool(), Some(true));
    assert_eq!(first_payload["results"].as_array().map(Vec::len), Some(2));

    let second_output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "search",
        "--mode",
        "literal",
        "--limit",
        "2",
        "--cursor",
        &next_cursor,
        "--format",
        "json",
        "git",
    ]);
    let second_payload: Value =
        serde_json::from_slice(&second_output.stdout).expect("second page should parse");
    let second_ids = second_payload["results"]
        .as_array()
        .expect("results should be an array")
        .iter()
        .map(|row| row["snapshot_id"].as_i64().unwrap())
        .collect::<Vec<_>>();

    assert!(second_output.status.success());
    assert_eq!(second_payload["truncated"].as_bool(), Some(false));
    assert!(second_payload["next_cursor"].is_null());
    assert_eq!(second_ids.len(), 1);
    assert!(!first_ids.contains(&second_ids[0]));

    cleanup_db(&path);
    Ok(())
}

#[test]
fn search_rejects_cursor_filter_mismatches() -> Result<()> {
    let path = temp_db_path("search-cursor-mismatch");
    seed_database(
        &path,
        &[text_snapshot(1, "git status"), text_snapshot(2, "git commit")],
    )?;

    let first_output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "search",
        "--mode",
        "literal",
        "--limit",
        "1",
        "--format",
        "json",
        "git",
    ]);
    let first_payload: Value =
        serde_json::from_slice(&first_output.stdout).expect("first page should parse");
    let next_cursor = first_payload["next_cursor"]
        .as_str()
        .expect("first page should include a cursor")
        .to_string();

    let mismatched = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "search",
        "--mode",
        "literal",
        "--limit",
        "1",
        "--cursor",
        &next_cursor,
        "--format",
        "json",
        "cargo",
    ]);
    let stderr = stderr_text(&mismatched);

    assert!(!mismatched.status.success());
    assert!(stderr.contains("cursor does not match the active search query or mode"));

    cleanup_db(&path);
    Ok(())
}

#[test]
fn timeline_rejects_cursor_filter_mismatches() -> Result<()> {
    let path = temp_db_path("timeline-cursor-mismatch");
    let events = seed_events(
        &path,
        &[text_snapshot(1, "alpha"), text_snapshot(2, "beta")],
    )?;
    set_event_observed_at(&path, events[0].1, "2026-04-16 09:00:00")?;
    set_event_observed_at(&path, events[1].1, "2026-04-16 10:00:00")?;

    let first_output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "timeline",
        "--sort",
        "asc",
        "--limit",
        "1",
        "--format",
        "json",
    ]);
    let first_payload: Value =
        serde_json::from_slice(&first_output.stdout).expect("timeline page should parse");
    let next_cursor = first_payload["next_cursor"]
        .as_str()
        .expect("timeline page should include a cursor")
        .to_string();

    let mismatched = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "timeline",
        "--sort",
        "desc",
        "--limit",
        "1",
        "--cursor",
        &next_cursor,
        "--format",
        "json",
    ]);
    let stderr = stderr_text(&mismatched);

    assert!(!mismatched.status.success());
    assert!(stderr.contains("cursor does not match the active timeline filters or sort"));

    cleanup_db(&path);
    Ok(())
}

#[test]
fn search_help_mentions_new_format_and_cursor_flags() {
    let output = run_cli(&["help", "search"]);
    let help_text = format!("{}{}", stdout_text(&output), stderr_text(&output));

    assert!(help_text.contains("--format <FORMAT>"));
    assert!(help_text.contains("--json"));
    assert!(help_text.contains("--cursor <CURSOR>"));
}

#[test]
fn recent_and_timeline_help_make_the_distinction_clear() {
    let recent = run_cli(&["help", "recent"]);
    let recent_help = format!("{}{}", stdout_text(&recent), stderr_text(&recent));
    let timeline = run_cli(&["help", "timeline"]);
    let timeline_help = format!("{}{}", stdout_text(&timeline), stderr_text(&timeline));

    assert!(recent_help.contains("recent unique clipboard states"));
    assert!(recent_help.contains("deduplicated by snapshot"));
    assert!(timeline_help.contains("chronological clipboard capture events"));
    assert!(timeline_help.contains("one row per observation"));
    assert!(timeline_help.contains("--since <SINCE>"));
    assert!(timeline_help.contains("--sort <SORT>"));
}

#[test]
fn json_alias_rejects_non_json_format() {
    let output = run_cli(&["search", "git", "--json", "--format", "md"]);
    let stderr = stderr_text(&output);

    assert!(!output.status.success());
    assert!(stderr.contains("`--json` is only compatible with `--format json`"));
}

#[test]
fn doctor_command_reports_database_capabilities() {
    let path = temp_db_path("doctor-text");
    let db = Database::open_or_init(&path).expect("test database should open");
    drop(db);
    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "doctor",
    ]);
    let stdout = stdout_text(&output);

    assert!(output.status.success());
    assert!(stdout.contains("database:"));
    assert!(stdout.contains("sqlite version:"));
    assert!(stdout.contains("fts5 temp table creation works:"));

    cleanup_db(&path);
}

#[test]
fn recent_command_rejects_zero_limit() {
    let path = temp_db_path("recent-zero-limit");
    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "recent",
        "--limit",
        "0",
    ]);
    let stderr = stderr_text(&output);

    assert!(!output.status.success());
    assert!(stderr.contains('0'));

    cleanup_db(&path);
}

#[test]
fn export_command_writes_raw_representation_bytes() -> Result<()> {
    let path = temp_db_path("export-bytes");
    let output_path = std::env::temp_dir()
        .join(format!("clipmem-export-{}-{}.bin", process::id(), 1));
    let snapshot = build_snapshot(
        CaptureContext::new(1)
            .with_frontmost_app_name("Preview")
            .with_frontmost_app_bundle_id("com.apple.Preview"),
        vec![build_item(
            0,
            vec![build_representation(
                "public.png".to_string(),
                None,
                vec![0x89, b'P', b'N', b'G'],
            )],
        )],
    );
    let ids = seed_database(&path, &[snapshot])?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "export",
        &ids[0].to_string(),
        "--item",
        "0",
        "--uti",
        "public.png",
        "--out",
        output_path.to_str().expect("output path should be UTF-8"),
    ]);
    let stdout = stdout_text(&output);
    let bytes = fs::read(&output_path)?;

    assert!(output.status.success());
    assert_eq!(bytes, vec![0x89, b'P', b'N', b'G']);
    assert!(stdout.contains("snapshot="));
    assert!(stdout.contains("uti=public.png"));

    let _ = fs::remove_file(&output_path);
    cleanup_db(&path);
    Ok(())
}

#[test]
fn export_command_fails_for_unknown_representation() -> Result<()> {
    let path = temp_db_path("export-missing");
    let snapshot = text_snapshot(1, "git status");
    let ids = seed_database(&path, &[snapshot])?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "export",
        &ids[0].to_string(),
        "--item",
        "0",
        "--uti",
        "public.png",
        "--out",
        "/tmp/clipmem-missing.bin",
    ]);
    let stderr = stderr_text(&output);

    assert!(!output.status.success());
    assert!(stderr.contains("representation not found"));

    cleanup_db(&path);
    Ok(())
}
