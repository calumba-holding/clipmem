use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Result};
use serde_json::json;

use crate::db::{Database, RetrievalFilters};
use crate::model::FlattenedTextProjection;

use crate::cli::schema::RetrievalFilterArgs;

#[cfg(test)]
use super::cursor::{
    encode_recent_cursor, encode_search_cursor, parse_recent_cursor, parse_search_cursor,
};
#[cfg(test)]
use super::recall::query_search_results;
#[cfg(test)]
use crate::app::WatchState;
#[cfg(test)]
use crate::cli::commands::openclaw_manage::{packaged_openclaw_files, packaged_openclaw_skill};
#[cfg(test)]
use crate::cli::commands::openclaw_validate::{
    referenced_markdown_files, validate_openclaw_skill_content,
};
#[cfg(test)]
use crate::cli::commands::runtime::run_watch_iteration_with_capture;
#[cfg(test)]
use crate::cli::schema::{SearchArgs, WatchArgs};
#[cfg(test)]
use crate::db::{SearchMode, SearchResults};

pub(in crate::cli) fn normalize_retrieval_filters(
    args: &RetrievalFilterArgs,
) -> Result<RetrievalFilters> {
    args.normalized()
        .map_err(|error| anyhow!(error.to_string()))
}

pub(in crate::cli) fn load_snapshot_projections<I>(
    db: &Database,
    snapshot_ids: I,
) -> Result<HashMap<i64, FlattenedTextProjection>>
where
    I: IntoIterator<Item = i64>,
{
    let mut unique_ids = Vec::new();
    let mut seen = HashSet::new();
    for snapshot_id in snapshot_ids {
        if seen.insert(snapshot_id) {
            unique_ids.push(snapshot_id);
        }
    }

    let mut projections = HashMap::with_capacity(unique_ids.len());
    for snapshot_id in unique_ids {
        if let Some(projection) = db.snapshot_projection(snapshot_id)? {
            projections.insert(snapshot_id, projection);
        }
    }

    Ok(projections)
}

pub(in crate::cli) fn merge_applied_filters(
    filters: &RetrievalFilters,
    command_specific: serde_json::Value,
) -> serde_json::Value {
    let mut value = serde_json::to_value(filters).unwrap_or_else(|_| json!({}));
    if let (Some(base), Some(extra)) = (value.as_object_mut(), command_specific.as_object()) {
        for (key, value) in extra {
            base.insert(key.clone(), value.clone());
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::fs;
    use std::path::PathBuf;
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        encode_recent_cursor, encode_search_cursor, packaged_openclaw_files,
        packaged_openclaw_skill, parse_recent_cursor, parse_search_cursor, query_search_results,
        referenced_markdown_files, run_watch_iteration_with_capture,
        validate_openclaw_skill_content, SearchArgs, SearchMode, SearchResults, WatchArgs,
        WatchState,
    };
    use crate::cli::formats::OutputArgs;
    use crate::cli::schema::RetrievalFilterArgs;
    use crate::db::{Database, RetrievalFilters};
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

    fn unfiltered() -> RetrievalFilters {
        RetrievalFilters::default()
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
                    b"git status".to_vec(),
                )],
            )],
        );
        let second = build_snapshot(
            CaptureContext::new(2)
                .with_frontmost_app_name("Terminal")
                .with_frontmost_app_bundle_id("com.apple.Terminal"),
            vec![build_item(
                0,
                vec![build_representation(
                    "public.utf8-plain-text".to_string(),
                    None,
                    b"cargo test".to_vec(),
                )],
            )],
        );

        db.store_capture(&first).expect("seed should succeed");
        db.store_capture(&second).expect("seed should succeed");

        let results: SearchResults = query_search_results(
            &db,
            &SearchArgs {
                query: "git".to_string(),
                mode: SearchMode::Literal,
                limit: 10,
                cursor: None,
                filters: RetrievalFilterArgs {
                    since: None,
                    until: None,
                    hours: None,
                    app: None,
                    bundle_id: None,
                    kind: None,
                    has_text: false,
                    has_url: false,
                    has_file_url: false,
                    has_image: false,
                    has_pdf: false,
                    min_bytes: None,
                    max_bytes: None,
                },
                output: OutputArgs {
                    format: None,
                    json: false,
                    human: false,
                },
            },
            &unfiltered(),
            None,
        )
        .expect("query should succeed");

        assert_eq!(results.mode_used(), SearchMode::Literal);
        assert_eq!(results.hits().len(), 1);
        assert_eq!(results.hits()[0].preview_text(), "git status");

        cleanup_db(&path);
    }

    #[test]
    fn search_cursor_round_trips_and_rejects_mismatches() {
        let hit = crate::model::SearchHit::from_parts(
            crate::model::SearchHitParts::plain_text(9, 12, "git status".to_string())
                .with_sha256("abc".to_string())
                .with_match(
                    Some("git status".to_string()),
                    vec!["best_text".to_string(), "search_text".to_string()],
                )
                .with_capture_summary(
                    1,
                    "2026-04-16T09:00:00Z".to_string(),
                    "2026-04-16T10:00:00Z".to_string(),
                )
                .with_last_frontmost_app(
                    Some("Terminal".to_string()),
                    Some("com.apple.Terminal".to_string()),
                )
                .with_size(10, 1)
                .with_score(Some(0.5)),
        );
        let filters = unfiltered();
        let encoded =
            encode_search_cursor("git", SearchMode::Auto, &filters, SearchMode::Fts, &hit).unwrap();
        let cursor = parse_search_cursor(&encoded, "git", SearchMode::Auto, &filters).unwrap();

        assert_eq!(cursor.mode_used(), SearchMode::Fts);
        assert!(parse_search_cursor(&encoded, "cargo", SearchMode::Auto, &filters).is_err());
    }

    #[test]
    fn recent_cursor_round_trips_and_rejects_mismatches() {
        let hit = crate::model::SearchHit::from_parts(
            crate::model::SearchHitParts::plain_text(9, 12, "git status".to_string())
                .with_sha256("abc".to_string())
                .with_capture_summary(
                    1,
                    "2026-04-16T09:00:00Z".to_string(),
                    "2026-04-16T10:00:00Z".to_string(),
                )
                .with_last_frontmost_app(
                    Some("Terminal".to_string()),
                    Some("com.apple.Terminal".to_string()),
                )
                .with_size(10, 1),
        );
        let filters = RetrievalFilters::default().with_hours(Some(24));
        let encoded = encode_recent_cursor(&filters, &hit).unwrap();
        let cursor = parse_recent_cursor(&encoded, &filters).unwrap();

        assert_eq!(cursor.snapshot_id(), 9);
        assert!(
            parse_recent_cursor(&encoded, &RetrievalFilters::default().with_hours(Some(12)))
                .is_err()
        );
    }

    #[test]
    fn watch_iteration_skips_initial_clipboard_state_once() {
        let path = temp_db_path("watch-skip-initial");
        let mut db = Database::open_or_init(&path).expect("test database should open");
        let args = WatchArgs {
            interval_ms: 350,
            quiet: true,
            skip_initial: true,
        };
        let mut state = WatchState::new();
        let capture_calls = Cell::new(0);

        let result = run_watch_iteration_with_capture(
            &mut db,
            &args,
            &mut state,
            || Ok(7),
            || {
                capture_calls.set(capture_calls.get() + 1);
                Ok(build_snapshot(
                    CaptureContext::new(7)
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
                ))
            },
        );

        assert!(result.is_ok());
        assert_eq!(capture_calls.get(), 0);
        assert!(db.recent(10, &unfiltered()).unwrap().hits().is_empty());

        cleanup_db(&path);
    }

    #[test]
    fn watch_iteration_captures_when_clipboard_changes() {
        let path = temp_db_path("watch-capture-change");
        let mut db = Database::open_or_init(&path).expect("test database should open");
        let args = WatchArgs {
            interval_ms: 350,
            quiet: true,
            skip_initial: false,
        };
        let mut state = WatchState::new();
        let capture_calls = Cell::new(0);

        let result = run_watch_iteration_with_capture(
            &mut db,
            &args,
            &mut state,
            || Ok(2),
            || {
                capture_calls.set(capture_calls.get() + 1);
                Ok(build_snapshot(
                    CaptureContext::new(2)
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
                ))
            },
        );

        assert!(result.is_ok());
        assert_eq!(capture_calls.get(), 1);
        assert_eq!(db.recent(10, &unfiltered()).unwrap().hits().len(), 1);

        cleanup_db(&path);
    }

    #[test]
    fn watch_iteration_skips_restored_snapshots_and_marks_them_handled() {
        let path = temp_db_path("watch-restore-suppression");
        let mut db = Database::open_or_init(&path).expect("test database should open");
        let stored = db
            .store_capture(&build_snapshot(
                CaptureContext::new(1)
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
            ))
            .expect("seed snapshot should store");
        let sha256 = db
            .find_snapshot(stored.snapshot_id(), 1)
            .expect("snapshot lookup should succeed")
            .expect("snapshot should exist")
            .sha256()
            .to_string();
        db.register_pending_restore(&sha256)
            .expect("pending restore should register");
        let args = WatchArgs {
            interval_ms: 350,
            quiet: true,
            skip_initial: false,
        };
        let mut state = WatchState::new();
        let capture_calls = Cell::new(0);

        let first = run_watch_iteration_with_capture(
            &mut db,
            &args,
            &mut state,
            || Ok(2),
            || {
                capture_calls.set(capture_calls.get() + 1);
                Ok(build_snapshot(
                    CaptureContext::new(2)
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
                ))
            },
        );

        assert!(first.is_ok());
        assert_eq!(capture_calls.get(), 1);
        assert_eq!(
            db.recent(10, &unfiltered()).unwrap().hits()[0].capture_count(),
            1
        );

        let second = run_watch_iteration_with_capture(
            &mut db,
            &args,
            &mut state,
            || Ok(2),
            || panic!("restored change count should have been marked handled"),
        );

        assert!(second.is_ok());
        cleanup_db(&path);
    }

    #[test]
    fn watch_iteration_skips_paused_changes_and_marks_them_handled() {
        let path = temp_db_path("watch-paused");
        let mut db = Database::open_or_init(&path).expect("test database should open");
        db.set_paused(true).expect("pause setting should persist");
        let args = WatchArgs {
            interval_ms: 350,
            quiet: true,
            skip_initial: false,
        };
        let mut state = WatchState::new();
        let capture_calls = Cell::new(0);

        let first = run_watch_iteration_with_capture(
            &mut db,
            &args,
            &mut state,
            || Ok(5),
            || {
                capture_calls.set(capture_calls.get() + 1);
                Ok(build_snapshot(
                    CaptureContext::new(5)
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
                ))
            },
        );

        assert!(first.is_ok());
        assert_eq!(capture_calls.get(), 1);
        assert!(db.recent(10, &unfiltered()).unwrap().hits().is_empty());

        let second = run_watch_iteration_with_capture(
            &mut db,
            &args,
            &mut state,
            || Ok(5),
            || panic!("paused change count should have been marked handled"),
        );

        assert!(second.is_ok());
        cleanup_db(&path);
    }

    #[test]
    fn watch_iteration_skips_ignored_bundle_ids_and_does_not_backfill_later() {
        let path = temp_db_path("watch-ignore-bundle");
        let mut db = Database::open_or_init(&path).expect("test database should open");
        db.add_ignored_bundle_id("com.apple.terminal")
            .expect("ignore list insert should succeed");
        let args = WatchArgs {
            interval_ms: 350,
            quiet: true,
            skip_initial: false,
        };
        let mut state = WatchState::new();
        let capture_calls = Cell::new(0);

        let first = run_watch_iteration_with_capture(
            &mut db,
            &args,
            &mut state,
            || Ok(9),
            || {
                capture_calls.set(capture_calls.get() + 1);
                Ok(build_snapshot(
                    CaptureContext::new(9)
                        .with_frontmost_app_name("Terminal")
                        .with_frontmost_app_bundle_id("com.apple.Terminal"),
                    vec![build_item(
                        0,
                        vec![build_representation(
                            "public.utf8-plain-text".to_string(),
                            None,
                            b"ignored clipboard".to_vec(),
                        )],
                    )],
                ))
            },
        );

        assert!(first.is_ok());
        assert_eq!(capture_calls.get(), 1);
        assert!(db.recent(10, &unfiltered()).unwrap().hits().is_empty());

        let second = run_watch_iteration_with_capture(
            &mut db,
            &args,
            &mut state,
            || Ok(9),
            || panic!("ignored change count should have been marked handled"),
        );

        assert!(second.is_ok());
        assert!(db
            .search_auto("ignored", 10, &unfiltered())
            .unwrap()
            .hits()
            .is_empty());
        cleanup_db(&path);
    }

    #[test]
    fn watch_iteration_skips_api_key_like_content_and_does_not_backfill_later() {
        let path = temp_db_path("watch-api-key-filter");
        let mut db = Database::open_or_init(&path).expect("test database should open");
        db.set_api_key_filter_enabled(true)
            .expect("api key filter setting should persist");
        let args = WatchArgs {
            interval_ms: 350,
            quiet: true,
            skip_initial: false,
        };
        let mut state = WatchState::new();
        let capture_calls = Cell::new(0);

        let first = run_watch_iteration_with_capture(
            &mut db,
            &args,
            &mut state,
            || Ok(11),
            || {
                capture_calls.set(capture_calls.get() + 1);
                Ok(build_snapshot(
                    CaptureContext::new(11)
                        .with_frontmost_app_name("Terminal")
                        .with_frontmost_app_bundle_id("com.apple.Terminal"),
                    vec![build_item(
                        0,
                        vec![build_representation(
                            "public.utf8-plain-text".to_string(),
                            None,
                            b"Authorization: Bearer 8JfA-2mQpV_4tLz9XnR6cH0wKdS7yBu3".to_vec(),
                        )],
                    )],
                ))
            },
        );

        assert!(first.is_ok());
        assert_eq!(capture_calls.get(), 1);
        assert!(db.recent(10, &unfiltered()).unwrap().hits().is_empty());

        let second = run_watch_iteration_with_capture(
            &mut db,
            &args,
            &mut state,
            || Ok(11),
            || panic!("filtered change count should have been marked handled"),
        );

        assert!(second.is_ok());
        assert!(db
            .search_auto("Authorization", 10, &unfiltered())
            .unwrap()
            .hits()
            .is_empty());
        cleanup_db(&path);
    }

    #[test]
    fn watch_iteration_applies_retention_after_successful_store() {
        let path = temp_db_path("watch-retention");
        let mut db = Database::open_or_init(&path).expect("test database should open");
        let old = db
            .store_capture(&build_snapshot(
                CaptureContext::new(1)
                    .with_frontmost_app_name("Notes")
                    .with_frontmost_app_bundle_id("com.apple.Notes"),
                vec![build_item(
                    0,
                    vec![build_representation(
                        "public.utf8-plain-text".to_string(),
                        None,
                        b"expired clipboard".to_vec(),
                    )],
                )],
            ))
            .expect("seed snapshot should store");
        db.conn
            .execute(
                "UPDATE capture_events SET observed_at = '2000-01-01 00:00:00' WHERE id = ?1",
                [old.event_id()],
            )
            .expect("event timestamp should update");
        db.set_retention_seconds(Some(24 * 60 * 60))
            .expect("retention setting should persist");

        let args = WatchArgs {
            interval_ms: 350,
            quiet: true,
            skip_initial: false,
        };
        let mut state = WatchState::new();

        let result = run_watch_iteration_with_capture(
            &mut db,
            &args,
            &mut state,
            || Ok(2),
            || {
                Ok(build_snapshot(
                    CaptureContext::new(2)
                        .with_frontmost_app_name("Terminal")
                        .with_frontmost_app_bundle_id("com.apple.Terminal"),
                    vec![build_item(
                        0,
                        vec![build_representation(
                            "public.utf8-plain-text".to_string(),
                            None,
                            b"fresh clipboard".to_vec(),
                        )],
                    )],
                ))
            },
        );

        assert!(result.is_ok());
        assert!(db
            .search_auto("expired", 10, &unfiltered())
            .unwrap()
            .hits()
            .is_empty());
        let fresh_hits = db.search_auto("fresh", 10, &unfiltered()).unwrap();
        assert_eq!(fresh_hits.hits().len(), 1);
        assert_eq!(db.recent(10, &unfiltered()).unwrap().hits().len(), 1);

        cleanup_db(&path);
    }

    #[test]
    fn packaged_openclaw_skill_includes_required_metadata() {
        let content = packaged_openclaw_skill();

        validate_openclaw_skill_content(&content).expect("packaged skill should validate");
        assert!(content.contains("\"openclaw\""));
        assert!(content.contains("\"requires\":{\"bins\":[\"clipmem\"]}"));
        assert!(content.contains("license:"));
        assert!(content.contains("clipmem recall"));
        assert!(content.contains("clipmem timeline"));
        assert!(content.contains("clipmem export"));
        assert!(content.contains("references/commands.md"));
        assert!(content.contains("references/json-schema.md"));
        assert!(content.contains("references/examples.md"));
        assert!(content.contains("references/setup-check.md"));
        assert!(content.contains("references/troubleshooting.md"));
        assert!(content.contains("scripts/check-setup.sh"));
        assert!(content.contains("what was that command I copied?"));
        assert!(content.contains("toon"));
        assert!(content.contains("--cursor"));
        assert!(content.contains("schema_version"));
    }

    #[test]
    fn packaged_openclaw_package_embeds_reference_files() {
        let files = packaged_openclaw_files();
        assert_eq!(files.len(), 7);
        assert!(files.iter().any(|file| file.relative_path == "SKILL.md"));
        assert!(files
            .iter()
            .any(|file| file.relative_path == "references/commands.md"));
        assert!(files
            .iter()
            .any(|file| file.relative_path == "references/troubleshooting.md"));
        assert!(files
            .iter()
            .any(|file| file.relative_path == "references/json-schema.md"));
        assert!(files
            .iter()
            .any(|file| file.relative_path == "references/examples.md"));
        assert!(files
            .iter()
            .any(|file| file.relative_path == "references/setup-check.md"));
        assert!(files
            .iter()
            .any(|file| file.relative_path == "scripts/check-setup.sh"));
    }

    #[test]
    fn packaged_openclaw_skill_references_relative_markdown_files() {
        let references = referenced_markdown_files(&packaged_openclaw_skill());
        assert!(references
            .iter()
            .any(|path| path == &PathBuf::from("references/commands.md")));
        assert!(references
            .iter()
            .any(|path| path == &PathBuf::from("references/troubleshooting.md")));
        assert!(references
            .iter()
            .any(|path| path == &PathBuf::from("references/json-schema.md")));
        assert!(references
            .iter()
            .any(|path| path == &PathBuf::from("references/examples.md")));
        assert!(references
            .iter()
            .any(|path| path == &PathBuf::from("references/setup-check.md")));
    }
}
