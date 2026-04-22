use super::*;

pub(in crate::cli) fn render_list_markdown(envelope: &ListEnvelope) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# clipmem {}", envelope.command);
    let _ = writeln!(out);
    let _ = writeln!(out, "- Generated at: {}", envelope.generated_at);
    let _ = writeln!(
        out,
        "- Filters: {}",
        render_filter_pairs(&envelope.applied_filters)
    );
    let _ = writeln!(out, "- Truncated: {}", envelope.truncated);
    let _ = writeln!(
        out,
        "- Next cursor: {}",
        envelope.next_cursor.as_deref().unwrap_or("null")
    );
    let _ = writeln!(out);

    if envelope.results.is_empty() {
        let _ = writeln!(out, "No results.");
        return out;
    }

    if envelope.command == "timeline" {
        let _ = writeln!(
            out,
            "| event_id | snapshot_id | observed_at | app | best_text |"
        );
        let _ = writeln!(out, "| --- | --- | --- | --- | --- |");
    } else {
        let _ = writeln!(
            out,
            "| snapshot_id | kind | observed_at | app | best_text | score |"
        );
        let _ = writeln!(out, "| --- | --- | --- | --- | --- | --- |");
    }
    for row in &envelope.results {
        let app = row
            .app_name()
            .or(row.app_bundle_id())
            .unwrap_or("unknown app");
        let best_text = truncate_for_markdown(row.best_text(), 100);
        match row {
            ListRow::Snapshot(row) => {
                let score = row
                    .score
                    .map_or_else(|| "null".to_string(), |value| format!("{value:.3}"));
                let _ = writeln!(
                    out,
                    "| {} | {} | {} | {} | {} | {} |",
                    row.snapshot_id,
                    escape_markdown_cell(&row.kind),
                    escape_markdown_cell(&row.observed_at),
                    escape_markdown_cell(app),
                    escape_markdown_cell(&best_text),
                    score
                );
            }
            ListRow::Timeline(row) => {
                let _ = writeln!(
                    out,
                    "| {} | {} | {} | {} | {} |",
                    row.event_id,
                    row.snapshot_id,
                    escape_markdown_cell(&row.observed_at),
                    escape_markdown_cell(app),
                    escape_markdown_cell(&best_text)
                );
            }
        }
    }

    out
}

pub(in crate::cli) fn render_get_markdown(envelope: &GetEnvelope) -> String {
    let snapshot = &envelope.snapshot;
    let mut out = String::new();
    let _ = writeln!(out, "# clipmem get");
    let _ = writeln!(out);
    let _ = writeln!(out, "- Generated at: {}", envelope.generated_at);
    let _ = writeln!(
        out,
        "- Filters: {}",
        render_filter_pairs(&envelope.applied_filters)
    );
    let _ = writeln!(out, "- Snapshot: {}", snapshot.snapshot_id());
    let _ = writeln!(out, "- Kind: {}", snapshot.snapshot_kind());
    let _ = writeln!(out, "- First seen: {}", snapshot.first_observed_at());
    let _ = writeln!(out, "- Last seen: {}", snapshot.last_observed_at());
    let _ = writeln!(out, "- Captures: {}", snapshot.capture_count());
    let _ = writeln!(out, "- Bytes: {}", snapshot.total_bytes());
    let _ = writeln!(out, "- Items: {}", snapshot.item_count());
    if !snapshot.best_text().is_empty() {
        let _ = writeln!(
            out,
            "- Best text: {}",
            escape_markdown_cell(&truncate_for_markdown(snapshot.best_text(), 240))
        );
    }
    if let Some(best_text_uti) = snapshot.best_text_uti() {
        let _ = writeln!(
            out,
            "- Best text UTI: {}",
            escape_markdown_cell(best_text_uti)
        );
    }
    if !snapshot.text_summary().is_empty() {
        let _ = writeln!(
            out,
            "- Text summary: {}",
            escape_markdown_cell(&truncate_for_markdown(snapshot.text_summary(), 240))
        );
    }
    if !snapshot.urls().is_empty() {
        let _ = writeln!(
            out,
            "- URLs: {}",
            escape_markdown_cell(&truncate_for_markdown(&snapshot.urls().join(", "), 240))
        );
    }
    if !snapshot.file_paths().is_empty() {
        let _ = writeln!(
            out,
            "- File paths: {}",
            escape_markdown_cell(&truncate_for_markdown(
                &snapshot.file_paths().join(", "),
                240
            ))
        );
    }
    if !snapshot.preview_text().is_empty() {
        let _ = writeln!(
            out,
            "- Preview: {}",
            escape_markdown_cell(&truncate_for_markdown(snapshot.preview_text(), 160))
        );
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "## Items");
    if snapshot.items().is_empty() {
        let _ = writeln!(out, "No items.");
    } else {
        for item in snapshot.items() {
            let _ = writeln!(
                out,
                "- item={} kind={} bytes={} preview={}",
                item.item_index(),
                item.primary_kind(),
                item.total_bytes(),
                escape_markdown_cell(&truncate_for_markdown(item.preview_text(), 120))
            );
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "## Recent Events");
    if snapshot.recent_events().is_empty() {
        let _ = writeln!(out, "No recent events.");
    } else {
        for event in snapshot.recent_events() {
            let source = event
                .frontmost_app_name()
                .or(event.frontmost_app_bundle_id())
                .unwrap_or("unknown app");
            let _ = writeln!(
                out,
                "- event={} observed_at={} change_count={} source={}",
                event.event_id(),
                event.observed_at(),
                event.change_count(),
                escape_markdown_cell(source)
            );
        }
    }

    out
}

pub(in crate::cli) fn render_recall_markdown(envelope: &RecallEnvelope) -> String {
    let best = &envelope.best_candidate;
    let mut out = String::new();
    let _ = writeln!(out, "# Best Match");
    let _ = writeln!(out);

    if let Some(quoted_text) = &envelope.quoted_text {
        for line in quoted_text.lines() {
            let _ = writeln!(out, "> {line}");
        }
    } else if !best.best_text.is_empty() {
        let _ = writeln!(out, "{}", best.best_text);
    } else {
        let _ = writeln!(out, "No direct text was recovered for this clipboard item.");
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "**Why This Match**");
    let _ = writeln!(out, "{}", envelope.why_selected);
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Provenance: snapshot {} · observed {} · {} · confidence {}{}",
        best.snapshot_id,
        best.observed_at,
        best.app_name
            .as_deref()
            .or(best.app_bundle_id.as_deref())
            .unwrap_or("unknown app"),
        render_confidence_label(&envelope.best_match_confidence),
        envelope
            .best_match_score
            .map(|score| format!(" · score {:.3}", score))
            .unwrap_or_default()
    );
    if !best.urls.is_empty() {
        let _ = writeln!(out, "URLs: {}", best.urls.join(", "));
    }
    if !best.file_paths.is_empty() {
        let _ = writeln!(out, "Files: {}", best.file_paths.join(", "));
    }

    if !envelope.alternatives.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "## Alternatives");
        for alternative in envelope.alternatives.iter().take(5) {
            let app = alternative
                .app_name
                .as_deref()
                .or(alternative.app_bundle_id.as_deref())
                .unwrap_or("unknown app");
            let _ = writeln!(
                out,
                "- snapshot {} · {} · {} · {}",
                alternative.snapshot_id,
                alternative.observed_at,
                app,
                escape_markdown_cell(&alternative.snippet)
            );
        }
    }

    out
}
