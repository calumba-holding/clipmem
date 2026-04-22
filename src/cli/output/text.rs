use super::*;

pub(in crate::cli) fn render_capture_once_text(output: &CaptureOnceOutput) -> String {
    let mut out = String::new();
    match output {
        CaptureOnceOutput::Stored(output) => {
            let store_state = if output.store.inserted_new_snapshot() {
                "new content"
            } else {
                "already known content"
            };
            let _ = writeln!(
                out,
                "stored snapshot {} via event {} ({store_state})",
                output.store.snapshot_id(),
                output.store.event_id()
            );
            let _ = writeln!(out, "kind: {}", output.snapshot.snapshot_kind());
            let _ = writeln!(out, "bytes: {}", output.snapshot.total_bytes());
            if let Some(app) = output.snapshot.frontmost_app_name() {
                let _ = writeln!(out, "frontmost app: {app}");
            }
            if !output.snapshot.preview_text().is_empty() {
                let _ = writeln!(out, "preview: {}", output.snapshot.preview_text());
            }
        }
        CaptureOnceOutput::Skipped(output) => {
            let _ = writeln!(out, "skipped capture reason={}", output.reason.as_str());
            let _ = writeln!(out, "kind: {}", output.kind);
            let _ = writeln!(out, "bytes: {}", output.total_bytes);
            if let Some(app) = &output.frontmost_app_name {
                let _ = writeln!(out, "frontmost app: {app}");
            }
        }
    }

    out
}

pub(in crate::cli) fn render_hits_text(hits: &[SearchHit]) -> String {
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
            format_utc_timestamp(hit.last_observed_at()),
            hit.capture_count(),
            app,
            hit.total_bytes(),
            hit.item_count()
        );

        if !hit.preview_text().is_empty() {
            let _ = writeln!(out, "  preview: {}", hit.preview_text());
        }
        if let Some(why_matched) = hit
            .why_matched()
            .filter(|value| *value != hit.preview_text())
        {
            let _ = writeln!(out, "  match:   {why_matched}");
        }
        if !hit.urls().is_empty() {
            let _ = writeln!(out, "  urls:    {}", hit.urls().join(", "));
        }
        if !hit.file_paths().is_empty() {
            let _ = writeln!(out, "  files:   {}", hit.file_paths().join(", "));
        }
        if let Some(score) = hit.score() {
            let _ = writeln!(out, "  score:   {score:.3}");
        }
        push_blank_line(&mut out);
    }
    out
}

pub(in crate::cli) fn render_timeline_text(events: &[TimelineEvent]) -> String {
    let mut out = String::new();
    for event in events {
        let app = event
            .frontmost_app_name()
            .or(event.frontmost_app_bundle_id())
            .unwrap_or("unknown app");

        let _ = writeln!(
            out,
            "event {} · snapshot {} · {} · change_count={} · {} · {} bytes",
            event.event_id(),
            event.snapshot_id(),
            format_utc_timestamp(event.observed_at()),
            event.change_count(),
            app,
            event.total_bytes()
        );

        if !event.best_text().is_empty() {
            let _ = writeln!(out, "  best:    {}", event.best_text());
        }
        if !event.preview_text().is_empty() && event.preview_text() != event.best_text() {
            let _ = writeln!(out, "  preview: {}", event.preview_text());
        }
        if !event.urls().is_empty() {
            let _ = writeln!(out, "  urls:    {}", event.urls().join(", "));
        }
        if !event.file_paths().is_empty() {
            let _ = writeln!(out, "  files:   {}", event.file_paths().join(", "));
        }
        push_blank_line(&mut out);
    }
    out
}

pub(in crate::cli) fn render_search_results_text(results: &SearchResults) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "search mode: {}", results.mode_used().as_str());
    if !results.hits().is_empty() {
        out.push('\n');
    }
    out.push_str(&render_hits_text(results.hits()));
    out
}

pub(in crate::cli) fn render_stats_text(stats: &StatsReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Overview");
    let _ = writeln!(out, "  snapshots: {}", stats.snapshot_count);
    let _ = writeln!(out, "  capture events: {}", stats.capture_event_count);
    let _ = writeln!(out, "  unique apps: {}", stats.unique_app_count);
    let _ = writeln!(out, "  total bytes: {}", stats.total_bytes);
    let _ = writeln!(
        out,
        "  average bytes per snapshot: {:.1}",
        stats.average_bytes_per_snapshot
    );
    let _ = writeln!(
        out,
        "  average captures per snapshot: {:.2}",
        stats.average_captures_per_snapshot
    );
    let _ = writeln!(out, "  dedupe ratio: {:.1}%", stats.dedupe_ratio * 100.0);
    let _ = writeln!(
        out,
        "  observed: {} to {}",
        stats
            .first_observed_at
            .as_deref()
            .map_or("none".to_string(), format_utc_timestamp),
        stats
            .last_observed_at
            .as_deref()
            .map_or("none".to_string(), format_utc_timestamp)
    );
    let _ = writeln!(
        out,
        "  archive span: {}",
        stats.archive_span_seconds.map_or_else(
            || "0s".to_string(),
            |seconds| { format_duration_seconds(seconds.max(0) as u64) }
        )
    );
    push_blank_line(&mut out);

    let _ = writeln!(out, "Content mix");
    if stats.kind_breakdown.is_empty() {
        let _ = writeln!(out, "  none");
    } else {
        for entry in &stats.kind_breakdown {
            let _ = writeln!(
                out,
                "  {}: {} snapshots, {} bytes",
                entry.kind, entry.snapshot_count, entry.total_bytes
            );
        }
    }
    push_blank_line(&mut out);

    let _ = writeln!(out, "Top apps");
    if stats.top_apps.is_empty() {
        let _ = writeln!(out, "  none");
    } else {
        for app in &stats.top_apps {
            let _ = writeln!(out, "  {}: {} events", app.app, app.capture_event_count);
        }
    }
    push_blank_line(&mut out);

    let _ = writeln!(out, "Activity patterns");
    let busiest_hour = peak_bucket(&stats.busiest_hours).map_or_else(
        || "none".to_string(),
        |entry| {
            format!(
                "{}:00 UTC ({} events)",
                entry.bucket, entry.capture_event_count
            )
        },
    );
    let busiest_weekday = peak_bucket(&stats.busiest_weekdays).map_or_else(
        || "none".to_string(),
        |entry| format!("{} ({} events)", entry.bucket, entry.capture_event_count),
    );
    let _ = writeln!(out, "  Busiest hour: {busiest_hour}");
    let _ = writeln!(out, "  Busiest weekday: {busiest_weekday}");
    push_blank_line(&mut out);

    let _ = writeln!(out, "Leaderboards");
    if let Some(snapshot) = &stats.most_recopied_snapshot {
        let _ = writeln!(out, "  Most re-copied snapshot:");
        push_snapshot_leaderboard_entry(&mut out, snapshot, 4);
    } else {
        let _ = writeln!(out, "  Most re-copied snapshot: none");
    }
    let _ = writeln!(out, "  Largest snapshots:");
    push_snapshot_leaderboard(&mut out, &stats.largest_snapshots, 4);
    let _ = writeln!(out, "  Most captured snapshots:");
    push_snapshot_leaderboard(&mut out, &stats.most_captured_snapshots, 4);

    out
}

pub(in crate::cli) fn render_snapshot_text(snapshot: &SnapshotDetails) -> String {
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
        format_utc_timestamp(snapshot.first_observed_at()),
        format_utc_timestamp(snapshot.last_observed_at())
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
    if !snapshot.best_text().is_empty() {
        let _ = writeln!(out, "best: {}", snapshot.best_text());
    }
    if !snapshot.preview_text().is_empty() {
        let _ = writeln!(out, "preview: {}", snapshot.preview_text());
    }
    if !snapshot.urls().is_empty() {
        let _ = writeln!(out, "urls: {}", snapshot.urls().join(", "));
    }
    if !snapshot.file_paths().is_empty() {
        let _ = writeln!(out, "files: {}", snapshot.file_paths().join(", "));
    }
    if let Some(ocr_text) = snapshot.ocr_text() {
        let _ = writeln!(out, "ocr: {}", ocr_text);
    }
    if let Some(ocr_status) = snapshot.ocr_status() {
        let _ = writeln!(out, "ocr status: {}", ocr_status);
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
                format_utc_timestamp(event.observed_at()),
                event.change_count(),
                source
            );
        }
    }

    out
}

pub(in crate::cli) fn render_doctor_text(report: &DoctorReport) -> String {
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
