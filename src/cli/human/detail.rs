use std::fmt::Write;

use crate::cli::human::{
    format_bytes, format_count, format_timestamp_short, header, render_filter_summary, separator,
    truncate_cell, truncate_multiline, HumanTheme, WIDTH,
};
use crate::cli::output::GetEnvelope;
use crate::cli::service::ServiceProviderStatus;
use crate::model::SnapshotDetails;

pub(in crate::cli) fn render_get_human(envelope: &GetEnvelope) -> String {
    let theme = HumanTheme::detect();
    let snapshot = &envelope.snapshot;
    let mut out = header(&theme, "clipmem Snapshot");
    let _ = writeln!(
        out,
        "Generated {}  Filters: {}",
        format_timestamp_short(&envelope.generated_at),
        render_filter_summary(&envelope.applied_filters)
    );
    out.push('\n');

    push_kpi(&mut out, "Snapshot", theme.id(snapshot.snapshot_id()));
    push_kpi(
        &mut out,
        "Kind",
        snapshot.snapshot_kind().as_str().to_string(),
    );
    push_kpi(&mut out, "SHA-256", truncate_cell(snapshot.sha256(), 24));
    push_kpi(
        &mut out,
        "First seen",
        format_timestamp_short(snapshot.first_observed_at()),
    );
    push_kpi(
        &mut out,
        "Last seen",
        format_timestamp_short(snapshot.last_observed_at()),
    );
    push_kpi(&mut out, "Captures", format_count(snapshot.capture_count()));
    push_kpi(&mut out, "Items", format_count(snapshot.item_count()));
    push_kpi(
        &mut out,
        "Size",
        format_bytes(snapshot.total_bytes() as u64),
    );
    if let Some(app) = snapshot
        .last_frontmost_app_name()
        .or(snapshot.last_frontmost_app_bundle_id())
    {
        push_kpi(&mut out, "Last app", app.to_string());
    }
    out.push('\n');

    push_optional_text(&mut out, &theme, "Best Text", snapshot.best_text(), 360);
    if let Some(best_text_uti) = snapshot.best_text_uti() {
        push_kpi(&mut out, "Best text UTI", best_text_uti.to_string());
    }
    push_optional_text(
        &mut out,
        &theme,
        "Text Summary",
        snapshot.text_summary(),
        280,
    );
    if !snapshot.urls().is_empty() {
        push_optional_text(&mut out, &theme, "URLs", &snapshot.urls().join(", "), 280);
    }
    if !snapshot.file_paths().is_empty() {
        push_optional_text(
            &mut out,
            &theme,
            "Files",
            &snapshot.file_paths().join(", "),
            280,
        );
    }
    if let Some(ocr_text) = snapshot.ocr_text() {
        push_optional_text(&mut out, &theme, "OCR", ocr_text, 280);
    }
    if let Some(ocr_status) = snapshot.ocr_status() {
        push_kpi(&mut out, "OCR status", ocr_status.to_string());
    }

    push_snapshot_items(&mut out, &theme, snapshot);
    push_recent_events(&mut out, snapshot);
    out
}

pub(in crate::cli) fn push_snapshot_items(
    out: &mut String,
    theme: &HumanTheme,
    snapshot: &SnapshotDetails,
) {
    out.push('\n');
    let _ = writeln!(out, "{}", theme.section("Items"));
    if snapshot.items().is_empty() {
        out.push_str("No items.\n");
        return;
    }
    out.push_str(&separator(WIDTH, false));
    out.push('\n');
    let _ = writeln!(
        out,
        "{:>4}  {:<22}  {:<7}  {:>8}  {:<12}  Preview",
        "Item", "UTI", "Kind", "Size", "SHA"
    );
    out.push_str(&separator(WIDTH, false));
    out.push('\n');
    for item in snapshot.items() {
        if item.representations().is_empty() {
            let _ = writeln!(
                out,
                "{:>4}  {:<22}  {:<7}  {:>8}  {:<12}  {}",
                item.item_index(),
                item.primary_uti().unwrap_or("unknown"),
                item.primary_kind().as_str(),
                format_bytes(item.total_bytes() as u64),
                "",
                truncate_cell(item.preview_text(), 28)
            );
            continue;
        }
        for (idx, rep) in item.representations().iter().enumerate() {
            let preview = rep.text_value().unwrap_or_else(|| item.preview_text());
            let _ = writeln!(
                out,
                "{:>4}  {:<22}  {:<7}  {:>8}  {:<12}  {}",
                if idx == 0 {
                    item.item_index().to_string()
                } else {
                    String::new()
                },
                truncate_cell(rep.uti(), 22),
                rep.kind().as_str(),
                format_bytes(rep.byte_len() as u64),
                truncate_cell(rep.raw_sha256(), 12),
                truncate_cell(preview, 28)
            );
        }
    }
}

pub(in crate::cli) fn push_recent_events(out: &mut String, snapshot: &SnapshotDetails) {
    out.push('\n');
    out.push_str("Recent Events\n");
    if snapshot.recent_events().is_empty() {
        out.push_str("No recent events.\n");
        return;
    }
    out.push_str(&separator(WIDTH, false));
    out.push('\n');
    let _ = writeln!(
        out,
        "{:>7}  {:<14}  {:>12}  Source",
        "Event", "Observed", "Change"
    );
    out.push_str(&separator(WIDTH, false));
    out.push('\n');
    for event in snapshot.recent_events() {
        let source = event
            .frontmost_app_name()
            .or(event.frontmost_app_bundle_id())
            .unwrap_or("unknown");
        let _ = writeln!(
            out,
            "{:>7}  {:<14}  {:>12}  {}",
            event.event_id(),
            format_timestamp_short(event.observed_at()),
            event.change_count(),
            truncate_cell(source, 46)
        );
    }
}

pub(in crate::cli) fn push_ignored_bundle_table(out: &mut String, bundle_ids: &[String]) {
    if bundle_ids.is_empty() {
        return;
    }
    out.push('\n');
    out.push_str("Ignored Bundle IDs\n");
    out.push_str(&separator(60, false));
    out.push('\n');
    for (idx, bundle_id) in bundle_ids.iter().enumerate() {
        let _ = writeln!(out, "{:>2}. {}", idx + 1, bundle_id);
    }
}

pub(in crate::cli) fn push_provider_row(out: &mut String, provider: &ServiceProviderStatus) {
    let _ = writeln!(
        out,
        "{:<13}  {:<16}  {:>9}  {:>7}  {:>7}  {:<8}  {}",
        provider.provider.as_str(),
        format!("{:?}", provider.state).to_ascii_lowercase(),
        provider.installed,
        provider.loaded,
        provider.running,
        truncate_cell(&provider.label, 8),
        provider
            .pid
            .map(|pid| pid.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
}

pub(in crate::cli) fn push_optional_text(
    out: &mut String,
    theme: &HumanTheme,
    label: &str,
    text: &str,
    limit: usize,
) {
    if text.trim().is_empty() {
        return;
    }
    let _ = writeln!(out, "{}", theme.section(label));
    out.push_str(&truncate_multiline(text, limit));
    out.push('\n');
}

pub(in crate::cli) fn push_kpi(out: &mut String, label: &str, value: String) {
    let _ = writeln!(out, "{:<18} {}", format!("{label}:"), value);
}
