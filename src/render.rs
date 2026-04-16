use std::fmt::Write;

use crate::model::{
    CaptureStoreResult, ClipboardSnapshot, DoctorReport, SearchHit, SnapshotDetails,
};

pub(crate) fn render_capture_once_text(
    store: &CaptureStoreResult,
    snapshot: &ClipboardSnapshot,
) -> String {
    let mut out = String::new();
    let store_state = if store.inserted_new_snapshot() {
        "new content"
    } else {
        "already known content"
    };
    let _ = writeln!(
        out,
        "stored snapshot {} via event {} ({store_state})",
        store.snapshot_id(),
        store.event_id()
    );
    let _ = writeln!(out, "kind: {}", snapshot.snapshot_kind());
    let _ = writeln!(out, "bytes: {}", snapshot.total_bytes());
    if let Some(app) = snapshot.frontmost_app_name() {
        let _ = writeln!(out, "frontmost app: {app}");
    }
    if !snapshot.preview_text().is_empty() {
        let _ = writeln!(out, "preview: {}", snapshot.preview_text());
    }
    out
}

pub(crate) fn render_hits_text(hits: &[SearchHit]) -> String {
    let mut out = String::new();
    for hit in hits {
        let app = hit
            .last_frontmost_app_name()
            .or(hit.last_frontmost_app_bundle_id())
            .unwrap_or("unknown app");

        let _ = writeln!(
            out,
            "[{}] {} · last seen {} · captures {} · {} · {} bytes · {} items",
            hit.snapshot_id(),
            hit.snapshot_kind(),
            hit.last_observed_at(),
            hit.capture_count(),
            app,
            hit.total_bytes(),
            hit.item_count()
        );

        if !hit.preview_text().is_empty() {
            let _ = writeln!(out, "  preview: {}", hit.preview_text());
        }
        if !hit.snippet().is_empty() && hit.snippet() != hit.preview_text() {
            let _ = writeln!(out, "  match:   {}", hit.snippet());
        }
        if let Some(score) = hit.score() {
            let _ = writeln!(out, "  score:   {score:.3}");
        }
        push_blank_line(&mut out);
    }
    out
}

pub(crate) fn render_snapshot_text(snapshot: &SnapshotDetails) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "snapshot {} · sha256={} · kind={} · captures={}",
        snapshot.snapshot_id(),
        snapshot.sha256(),
        snapshot.snapshot_kind(),
        snapshot.capture_count()
    );
    let _ = writeln!(
        out,
        "first seen: {} · last seen: {}",
        snapshot.first_observed_at(),
        snapshot.last_observed_at()
    );
    let _ = writeln!(
        out,
        "bytes: {} · items: {}",
        snapshot.total_bytes(),
        snapshot.item_count()
    );
    if let Some(app) = snapshot
        .last_frontmost_app_name()
        .or(snapshot.last_frontmost_app_bundle_id())
    {
        let _ = writeln!(out, "last frontmost app: {app}");
    }
    if !snapshot.preview_text().is_empty() {
        let _ = writeln!(out, "preview: {}", snapshot.preview_text());
    }
    push_blank_line(&mut out);

    for item in snapshot.items() {
        let _ = writeln!(
            out,
            "item {} · kind={} · bytes={}{}",
            item.item_index(),
            item.primary_kind(),
            item.total_bytes(),
            item.primary_uti()
                .map(|uti| format!(" · primary type={uti}"))
                .unwrap_or_default()
        );
        if !item.preview_text().is_empty() {
            let _ = writeln!(out, "  preview: {}", item.preview_text());
        }
        for rep in item.representations() {
            let _ = writeln!(
                out,
                "  - {} · kind={} · {} bytes · sha256={}",
                rep.uti(),
                rep.kind(),
                rep.byte_len(),
                rep.raw_sha256()
            );
            if let Some(text) = rep.text_value() {
                let _ = writeln!(out, "    text: {text}");
            }
        }
        push_blank_line(&mut out);
    }

    if !snapshot.recent_events().is_empty() {
        let _ = writeln!(out, "recent events:");
        for event in snapshot.recent_events() {
            let source = event
                .frontmost_app_name()
                .or(event.frontmost_app_bundle_id())
                .unwrap_or("unknown app");
            let _ = writeln!(
                out,
                "  event {} · {} · change_count={} · {}",
                event.event_id(),
                event.observed_at(),
                event.change_count(),
                source
            );
        }
    }

    out
}

pub(crate) fn render_doctor_text(report: &DoctorReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "database: {}", report.db_path());
    let _ = writeln!(out, "sqlite version: {}", report.sqlite_version());
    let _ = writeln!(out, "journal mode: {}", report.journal_mode());
    let _ = writeln!(
        out,
        "fts5 compile option present: {}",
        report.fts5_compile_option_present()
    );
    let _ = writeln!(
        out,
        "fts5 temp table creation works: {}",
        report.fts5_create_virtual_table_ok()
    );

    if !report.compile_options().is_empty() {
        out.push('\n');
        let _ = writeln!(out, "compile options:");
        for opt in report.compile_options() {
            let _ = writeln!(out, "  {opt}");
        }
    }

    out
}

fn push_blank_line(out: &mut String) {
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use crate::model::{
        build_item, build_representation, build_snapshot, CaptureContext, CaptureEvent,
        CaptureStoreResult, DoctorReport, SearchHit, SnapshotDetails, SnapshotKind,
    };

    use super::{
        render_capture_once_text, render_doctor_text, render_hits_text, render_snapshot_text,
    };

    #[test]
    fn render_capture_once_text_reports_summary_lines() {
        let text = render_capture_once_text(
            &CaptureStoreResult::new(9, 12, true),
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
        let text = render_hits_text(&[SearchHit::new(
            42,
            "deadbeef".to_string(),
            SnapshotKind::PlainText,
            "git status".to_string(),
            "git ⟦status⟧".to_string(),
            2,
            "2026-04-16 18:00:00".to_string(),
            Some("Terminal".to_string()),
            Some("com.apple.Terminal".to_string()),
            10,
            1,
            Some(0.125),
        )]);

        assert!(text.contains("[42] plain_text"));
        assert!(text.contains("preview: git status"));
        assert!(text.contains("match:   git ⟦status⟧"));
        assert!(text.contains("score:   0.125"));
    }

    #[test]
    fn render_snapshot_text_lists_items_and_events() {
        let text = render_snapshot_text(&SnapshotDetails::new(
            7,
            "abc123".to_string(),
            SnapshotKind::PlainText,
            "git clone".to_string(),
            "git clone".to_string(),
            1,
            9,
            "2026-04-16 18:00:00".to_string(),
            1,
            "2026-04-16 18:00:00".to_string(),
            "2026-04-16 18:00:00".to_string(),
            Some("Terminal".to_string()),
            Some("com.apple.Terminal".to_string()),
            vec![CaptureEvent::new(
                9,
                "2026-04-16 18:00:00".to_string(),
                4,
                Some("Terminal".to_string()),
                Some("com.apple.Terminal".to_string()),
            )],
            vec![build_item(
                0,
                vec![build_representation(
                    "public.utf8-plain-text".to_string(),
                    None,
                    b"git clone".to_vec(),
                )],
            )],
        ));

        assert!(text.contains("snapshot 7 · sha256=abc123 · kind=plain_text · captures=1"));
        assert!(text.contains("item 0 · kind=plain_text"));
        assert!(text.contains("text: git clone"));
        assert!(text.contains("event 9 · 2026-04-16 18:00:00 · change_count=4 · Terminal"));
    }

    #[test]
    fn render_doctor_text_lists_compile_options_when_present() {
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
}
