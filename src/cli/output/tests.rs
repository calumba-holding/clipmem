use serde_json::{json, Value};
use std::fmt::Write;
use std::time::{Duration, Instant};

use crate::db::{SearchMode, SearchResults};
use crate::model::{
    build_item, build_representation, build_snapshot, CaptureContext, CaptureEvent,
    CaptureStoreResult, DoctorReport, SearchHit, SearchHitParts, SnapshotDetails, SnapshotKind,
    TimelineEvent,
};

use super::super::commands::{CaptureOnceOutput, CaptureOnceStoredOutput};
use super::json::write_json_pretty;
use super::markdown::{render_get_markdown, render_list_markdown, render_recall_markdown};
use super::model::{
    GetEnvelope, ListEnvelope, ListRow, RecallEnvelope, RecallMatchConfidence, RecallOutputRow,
    SnapshotListRow, TimelineListRow, ToonSnapshotRowProjection, ToonTimelineRowProjection,
    OUTPUT_SCHEMA_VERSION,
};
use super::text::{
    render_capture_once_text, render_doctor_text, render_hits_text, render_search_results_text,
    render_snapshot_text, render_timeline_text,
};
use super::toon::{render_list_toon, render_recall_toon};

#[test]
pub(in crate::cli) fn render_capture_once_text_reports_summary_lines() {
    let text = render_capture_once_text(&CaptureOnceOutput::Stored(CaptureOnceStoredOutput {
        store: CaptureStoreResult::new(9, 12, true),
        snapshot: build_snapshot(
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
    }));

    assert!(text.contains("stored snapshot 9 via event 12"));
    assert!(text.contains("kind: plain_text"));
    assert!(text.contains("preview: git status"));
}

#[test]
pub(in crate::cli) fn render_capture_once_text_skips_preview_for_filtered_content() {
    let text = render_capture_once_text(&CaptureOnceOutput::Skipped(
        super::super::commands::CaptureOnceSkippedOutput {
            status: "skipped",
            reason: crate::db::CaptureSkipReason::ApiKeyFilter,
            kind: "plain_text".to_string(),
            total_bytes: 48,
            frontmost_app_name: Some("Terminal".to_string()),
            frontmost_app_bundle_id: Some("com.apple.Terminal".to_string()),
        },
    ));

    assert!(text.contains("skipped capture reason=api_key_filter"));
    assert!(text.contains("kind: plain_text"));
    assert!(!text.contains("preview:"));
}

#[test]
pub(in crate::cli) fn render_hits_text_includes_match_score_urls_and_files() {
    let text = render_hits_text(&[SearchHit::from_parts(
        SearchHitParts::plain_text(42, 77, "git clone".to_string())
            .with_sha256("abc123".to_string())
            .with_match(
                Some("⟦git⟧ clone".to_string()),
                vec!["best_text".to_string(), "search_text".to_string()],
            )
            .with_capture_summary(
                3,
                "2026-04-16 10:00:00".to_string(),
                "2026-04-16 11:00:00".to_string(),
            )
            .with_last_frontmost_app(
                Some("Terminal".to_string()),
                Some("com.apple.Terminal".to_string()),
            )
            .with_locations(
                vec!["https://example.com".to_string()],
                vec!["/Users/test/repo".to_string()],
            )
            .with_size(9, 1)
            .with_score(Some(0.125)),
    )]);

    assert!(text.contains("[42] plain_text"));
    assert!(text.contains("match:   ⟦git⟧ clone"));
    assert!(text.contains("urls:    https://example.com"));
    assert!(text.contains("files:   /Users/test/repo"));
    assert!(text.contains("score:   0.125"));
}

#[test]
pub(in crate::cli) fn render_snapshot_text_lists_items_and_events() {
    let text = render_snapshot_text(&SnapshotDetails::new(
        7,
        "abc123".to_string(),
        SnapshotKind::PlainText,
        "git push".to_string(),
        "git push".to_string(),
        1,
        8,
        "2026-04-16 10:00:00".to_string(),
        2,
        "2026-04-16 10:00:00".to_string(),
        "2026-04-16 11:00:00".to_string(),
        Some("Terminal".to_string()),
        Some("com.apple.Terminal".to_string()),
        None,
        None,
        vec![CaptureEvent::new(
            21,
            "2026-04-16 11:00:00".to_string(),
            3,
            Some("Terminal".to_string()),
            Some("com.apple.Terminal".to_string()),
        )],
        vec![build_item(
            0,
            vec![build_representation(
                "public.utf8-plain-text".to_string(),
                None,
                b"git push".to_vec(),
            )],
        )],
    ));

    assert!(text.contains("snapshot 7"));
    assert!(text.contains("item 0 · kind=plain_text"));
    assert!(text.contains("event 21"));
}

#[test]
pub(in crate::cli) fn render_doctor_text_lists_compile_options() {
    let text = render_doctor_text(&DoctorReport::new(
        "/tmp/clipmem.sqlite3".to_string(),
        "3.46.0".to_string(),
        "wal".to_string(),
        true,
        true,
        vec!["ENABLE_FTS5".to_string()],
    ));

    assert!(text.contains("database: /tmp/clipmem.sqlite3"));
    assert!(text.contains("compile options:"));
    assert!(text.contains("ENABLE_FTS5"));
}

#[test]
pub(in crate::cli) fn render_search_results_text_reports_effective_mode() {
    let text = render_search_results_text(&SearchResults::new(
        SearchMode::Literal,
        vec![SearchHit::from_parts(
            SearchHitParts::plain_text(7, 21, "git push".to_string())
                .with_sha256("abc123".to_string())
                .with_match(
                    Some("git push".to_string()),
                    vec!["best_text".to_string(), "search_text".to_string()],
                )
                .with_capture_summary(
                    2,
                    "2026-04-16 10:00:00".to_string(),
                    "2026-04-16 11:00:00".to_string(),
                )
                .with_last_frontmost_app(
                    Some("Terminal".to_string()),
                    Some("com.apple.Terminal".to_string()),
                )
                .with_size(8, 1),
        )],
        false,
    ));

    assert!(text.contains("search mode: literal"));
}

#[test]
pub(in crate::cli) fn render_timeline_text_reports_event_history() {
    let text = render_timeline_text(&[TimelineEvent::new(
        21,
        7,
        "2026-04-16 11:00:00".to_string(),
        3,
        "abc123".to_string(),
        SnapshotKind::PlainText,
        "git push".to_string(),
        "git push".to_string(),
        Some("Terminal".to_string()),
        Some("com.apple.Terminal".to_string()),
        vec!["https://example.com".to_string()],
        vec!["/Users/test/repo".to_string()],
        8,
        1,
    )]);

    assert!(text.contains("event 21"));
    assert!(text.contains("change_count=3"));
    assert!(text.contains("best:    git push"));
}

#[test]
pub(in crate::cli) fn markdown_and_toon_render_envelopes() {
    let row = ListRow::Snapshot(SnapshotListRow {
        snapshot_id: 1,
        event_id: 2,
        sha256: "abc".to_string(),
        kind: "plain_text".to_string(),
        observed_at: "2026-04-17T10:00:00Z".to_string(),
        first_seen_at: "2026-04-17T09:00:00Z".to_string(),
        last_seen_at: "2026-04-17T10:00:00Z".to_string(),
        app_name: Some("Terminal".to_string()),
        app_bundle_id: Some("com.apple.Terminal".to_string()),
        best_text: "git status".to_string(),
        best_text_uti: Some("public.utf8-plain-text".to_string()),
        text_fragments: Vec::new(),
        urls: vec!["https://example.com".to_string()],
        file_paths: Vec::new(),
        html_text: None,
        rtf_text: None,
        ocr_text: None,
        ocr_status: None,
        text_summary: "git status".to_string(),
        preview_text: "git status".to_string(),
        item_count: 1,
        total_bytes: 10,
        capture_count: 1,
        score: Some(0.3),
        why_matched: Some("git status".to_string()),
        matched_fields: vec!["best_text".to_string(), "search_text".to_string()],
    });
    let envelope = ListEnvelope {
        schema_version: OUTPUT_SCHEMA_VERSION,
        command: "search",
        generated_at: "2026-04-17T10:00:00Z".to_string(),
        applied_filters: json!({
            "query": "git",
            "requested_mode": "auto",
            "mode_used": "literal",
            "apps": ["Terminal", "Safari"],
            "window": {
                "hours": 24,
            },
            "flags": [
                {
                    "name": "prefer_recent",
                    "enabled": true,
                }
            ],
            "limit": 10,
            "cursor": null,
        }),
        truncated: false,
        next_cursor: None,
        results: vec![row],
    };
    let markdown = render_list_markdown(&envelope);
    let toon = render_list_toon(&envelope);

    assert!(markdown.contains("| snapshot_id | kind | observed_at |"));
    assert!(toon.contains(
            "results[#1\t]{snapshot_id\tevent_id\tobserved_at\tfirst_seen_at\tlast_seen_at\tkind\tapp_name\tapp_bundle_id\tdisplay_text\tcapture_count\titem_count\ttotal_bytes\tscore\twhy_matched}:"
        ));
    assert!(toon.contains("git status"));
    assert!(!toon.contains("sha256"));
    assert!(!toon.contains("text_fragments"));
    assert!(toon.contains("apps[#2\t]: Terminal\tSafari"));
    assert!(toon.contains("window:"));
    assert!(toon.contains("hours: 24"));
    assert!(toon.contains("flags[#1]:"));
    assert!(toon.contains("name: prefer_recent"));
}

#[test]
pub(in crate::cli) fn get_markdown_renders_summary_items_and_events() {
    let markdown = render_get_markdown(&GetEnvelope {
        schema_version: OUTPUT_SCHEMA_VERSION,
        command: "get",
        generated_at: "2026-04-17T10:00:00Z".to_string(),
        applied_filters: json!({}),
        snapshot: SnapshotDetails::new(
            7,
            "abc123".to_string(),
            SnapshotKind::PlainText,
            "git push".to_string(),
            "git push".to_string(),
            1,
            8,
            "2026-04-16 10:00:00".to_string(),
            2,
            "2026-04-16 10:00:00".to_string(),
            "2026-04-16 11:00:00".to_string(),
            Some("Terminal".to_string()),
            Some("com.apple.Terminal".to_string()),
            None,
            None,
            vec![CaptureEvent::new(
                21,
                "2026-04-16 11:00:00".to_string(),
                3,
                Some("Terminal".to_string()),
                Some("com.apple.Terminal".to_string()),
            )],
            vec![build_item(
                0,
                vec![build_representation(
                    "public.utf8-plain-text".to_string(),
                    None,
                    b"git push".to_vec(),
                )],
            )],
        ),
    });

    assert!(markdown.contains("## Items"));
    assert!(markdown.contains("## Recent Events"));
    assert!(markdown.contains("Snapshot: 7"));
}

#[test]
pub(in crate::cli) fn recall_markdown_and_toon_render_best_match_and_alternatives() {
    let envelope = RecallEnvelope {
        schema_version: OUTPUT_SCHEMA_VERSION,
        command: "recall",
        generated_at: "2026-04-17T10:00:00Z".to_string(),
        applied_filters: json!({
            "limit": 5,
            "prefer_recent": true,
        }),
        query: Some("git".to_string()),
        best_candidate: RecallOutputRow {
            snapshot_id: 1,
            event_id: 2,
            sha256: "abc".to_string(),
            kind: "plain_text".to_string(),
            observed_at: "2026-04-17T10:00:00Z".to_string(),
            first_seen_at: "2026-04-17T09:00:00Z".to_string(),
            last_seen_at: "2026-04-17T10:00:00Z".to_string(),
            app_name: Some("Terminal".to_string()),
            app_bundle_id: Some("com.apple.Terminal".to_string()),
            best_text: "".to_string(),
            best_text_uti: Some("public.utf8-plain-text".to_string()),
            text_fragments: Vec::new(),
            urls: vec!["https://example.com".to_string()],
            file_paths: vec!["/tmp/file.txt".to_string()],
            html_text: None,
            rtf_text: None,
            ocr_text: None,
            ocr_status: None,
            text_summary: "git status".to_string(),
            preview_text: "git status".to_string(),
            item_count: 1,
            total_bytes: 10,
            capture_count: 1,
            score: Some(0.1),
            why_matched: Some("git status".to_string()),
            matched_fields: vec!["best_text".to_string(), "search_text".to_string()],
            snippet: "quoted snippet".to_string(),
        },
        alternatives: vec![RecallOutputRow {
            snapshot_id: 3,
            event_id: 4,
            sha256: "def".to_string(),
            kind: "plain_text".to_string(),
            observed_at: "2026-04-17T09:00:00Z".to_string(),
            first_seen_at: "2026-04-17T08:00:00Z".to_string(),
            last_seen_at: "2026-04-17T09:00:00Z".to_string(),
            app_name: Some("Editor".to_string()),
            app_bundle_id: Some("com.example.Editor".to_string()),
            best_text: "git commit".to_string(),
            best_text_uti: Some("public.utf8-plain-text".to_string()),
            text_fragments: Vec::new(),
            urls: Vec::new(),
            file_paths: Vec::new(),
            html_text: None,
            rtf_text: None,
            ocr_text: None,
            ocr_status: None,
            text_summary: "git commit".to_string(),
            preview_text: "git commit".to_string(),
            item_count: 1,
            total_bytes: 10,
            capture_count: 1,
            score: None,
            why_matched: None,
            matched_fields: Vec::new(),
            snippet: String::new(),
        }],
        best_match_confidence: RecallMatchConfidence::High,
        best_match_score: Some(0.91),
        why_selected: "Selected the strongest search match".to_string(),
        quoted_text: Some("git status".to_string()),
    };

    let markdown = render_recall_markdown(&envelope);
    let toon = render_recall_toon(&envelope);

    assert!(markdown.contains("# Best Match"));
    assert!(markdown.contains("## Alternatives"));
    assert!(markdown.contains("> git status"));
    assert!(toon.contains(
            "best_candidate[#1\t]{snapshot_id\tevent_id\tobserved_at\tfirst_seen_at\tlast_seen_at\tkind\tapp_name\tapp_bundle_id\tdisplay_text\tcapture_count\titem_count\ttotal_bytes\tscore\twhy_matched}:"
        ));
    assert!(toon.contains("alternatives[#1\t]{snapshot_id\tevent_id\tobserved_at\tfirst_seen_at\tlast_seen_at\tkind\tapp_name\tapp_bundle_id\tdisplay_text\tcapture_count\titem_count\ttotal_bytes\tscore\twhy_matched}:"));
    assert!(toon.contains("quoted snippet"));
    assert!(toon.contains("git commit"));
    assert!(!toon.contains("\tsnippet}:"));
    assert!(!toon.contains("matched_fields"));
    assert!(!toon.contains("sha256"));
}

#[test]
pub(in crate::cli) fn timeline_toon_uses_scalar_projection_display_text_fallback() {
    let envelope = ListEnvelope {
        schema_version: OUTPUT_SCHEMA_VERSION,
        command: "timeline",
        generated_at: "2026-04-17T10:00:00Z".to_string(),
        applied_filters: json!({ "hours": 24 }),
        truncated: false,
        next_cursor: None,
        results: vec![ListRow::Timeline(TimelineListRow {
            event_id: 9,
            snapshot_id: 3,
            observed_at: "2026-04-17T10:00:00Z".to_string(),
            change_count: 2,
            kind: "plain_text".to_string(),
            app_name: Some("Terminal".to_string()),
            app_bundle_id: Some("com.apple.Terminal".to_string()),
            best_text: String::new(),
            best_text_uti: None,
            text_fragments: Vec::new(),
            urls: Vec::new(),
            file_paths: Vec::new(),
            html_text: None,
            rtf_text: None,
            ocr_text: None,
            ocr_status: None,
            text_summary: "git summary".to_string(),
            preview_text: "git preview".to_string(),
            total_bytes: 42,
            sha256: "abc123".to_string(),
            item_count: 1,
        })],
    };

    let toon = render_list_toon(&envelope);

    assert!(toon.contains(
            "results[#1\t]{event_id\tsnapshot_id\tobserved_at\tchange_count\tkind\tapp_name\tapp_bundle_id\tdisplay_text\titem_count\ttotal_bytes}:"
        ));
    assert!(toon.contains("git preview"));
    assert!(!toon.contains("best_text_uti"));
    assert!(!toon.contains("sha256"));
}

#[test]
#[ignore = "profiling harness for CLI JSON serialization"]
pub(in crate::cli) fn profile_json_output_serialization() {
    let envelope = large_list_envelope(10_000);

    let before = median_duration(7, || {
        let json = serde_json::to_string_pretty(&envelope).expect("serialize envelope to string");
        assert!(json.len() > 1_000_000);
    });
    let after = median_duration(7, || {
        let mut out = Vec::new();
        write_json_pretty(&mut out, &envelope).expect("stream envelope to writer");
        assert!(out.len() > 1_000_000);
    });

    eprintln!("json_output_string_before={before:?} json_output_writer_after={after:?}");
}

#[test]
#[ignore = "profiling harness for CLI TOON rendering"]
pub(in crate::cli) fn profile_toon_output_rendering() {
    let envelope = large_list_envelope(10_000);

    let before = median_duration(7, || {
        let toon = render_list_toon_join_encoded_for_profile(&envelope);
        assert!(toon.len() > 1_000_000);
    });
    let after = median_duration(7, || {
        let toon = render_list_toon(&envelope);
        assert!(toon.len() > 1_000_000);
    });

    eprintln!("toon_output_join_before={before:?} toon_output_stream_after={after:?}");
}

fn median_duration(runs: usize, mut f: impl FnMut()) -> Duration {
    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        let started = Instant::now();
        f();
        samples.push(started.elapsed());
    }
    samples.sort();
    samples[samples.len() / 2]
}

fn large_list_envelope(row_count: usize) -> ListEnvelope {
    let results = (0..row_count)
        .map(|index| {
            ListRow::Snapshot(SnapshotListRow {
                snapshot_id: index as i64 + 1,
                event_id: index as i64 + 10_000,
                sha256: format!("{:064x}", index + 1),
                kind: "plain_text".to_string(),
                observed_at: "2026-04-17T10:00:00Z".to_string(),
                first_seen_at: "2026-04-17T09:00:00Z".to_string(),
                last_seen_at: "2026-04-17T10:00:00Z".to_string(),
                app_name: Some("Terminal".to_string()),
                app_bundle_id: Some("com.apple.Terminal".to_string()),
                best_text: format!("git status output row {index} with enough text to resemble a real clipboard row"),
                best_text_uti: Some("public.utf8-plain-text".to_string()),
                text_fragments: Vec::new(),
                urls: vec![format!("https://example.com/{index}")],
                file_paths: vec![format!("/Users/test/repo/{index}/Cargo.toml")],
                html_text: None,
                rtf_text: None,
                ocr_text: None,
                ocr_status: None,
                text_summary: format!("git status summary {index}"),
                preview_text: format!("git status preview {index}"),
                item_count: 1,
                total_bytes: 128,
                capture_count: 1,
                score: Some(0.42),
                why_matched: Some("Exact text match".to_string()),
                matched_fields: vec!["best_text".to_string(), "search_text".to_string()],
            })
        })
        .collect();

    ListEnvelope {
        schema_version: OUTPUT_SCHEMA_VERSION,
        command: "search",
        generated_at: "2026-04-17T10:00:00Z".to_string(),
        applied_filters: json!({
            "query": "git",
            "requested_mode": "auto",
            "mode_used": "literal",
            "limit": row_count,
        }),
        truncated: false,
        next_cursor: None,
        results,
    }
}

fn render_list_toon_join_encoded_for_profile(envelope: &ListEnvelope) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "schema_version: {}", envelope.schema_version);
    let _ = writeln!(out, "command: {}", envelope.command);
    let _ = writeln!(out, "generated_at: {}", envelope.generated_at);
    let _ = writeln!(out, "applied_filters: {}", envelope.applied_filters);
    let _ = writeln!(out, "truncated: {}", envelope.truncated);
    let _ = writeln!(
        out,
        "next_cursor: {}",
        envelope.next_cursor.as_deref().unwrap_or("null")
    );

    let fields = if envelope.command == "timeline" {
        ToonTimelineRowProjection::FIELDS.as_slice()
    } else {
        ToonSnapshotRowProjection::FIELDS.as_slice()
    };
    let _ = writeln!(
        out,
        "results[#{}\t]{{{}}}:",
        envelope.results.len(),
        fields.join("\t")
    );

    for row in &envelope.results {
        let values = match row {
            ListRow::Snapshot(row) => ToonSnapshotRowProjection::from_row(row).values(),
            ListRow::Timeline(row) => ToonTimelineRowProjection::from_row(row).values(),
        };
        let encoded = values
            .iter()
            .map(encode_toon_scalar_for_profile)
            .collect::<Vec<_>>()
            .join("\t");
        let _ = writeln!(out, "  {encoded}");
    }

    out
}

fn encode_toon_scalar_for_profile(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => encode_toon_string_for_profile(text),
        Value::Array(_) | Value::Object(_) => {
            unreachable!("profile scalar encoder only accepts primitive values")
        }
    }
}

fn encode_toon_string_for_profile(text: &str) -> String {
    if text.is_empty()
        || text.contains('\t')
        || text.contains('\n')
        || text.contains('\r')
        || text.contains(':')
        || text.contains('"')
        || text.contains('\\')
        || text.starts_with(' ')
        || text.ends_with(' ')
    {
        let escaped = text
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t");
        format!("\"{escaped}\"")
    } else {
        text.to_string()
    }
}
