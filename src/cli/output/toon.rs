use super::*;

pub(in crate::cli) fn render_list_toon(envelope: &ListEnvelope) -> String {
    let mut out = String::with_capacity(estimated_list_toon_capacity(envelope));
    render_toon_entry(
        &mut out,
        "schema_version",
        &Value::from(envelope.schema_version as u64),
        0,
    );
    render_toon_entry(
        &mut out,
        "command",
        &Value::String(envelope.command.to_string()),
        0,
    );
    render_toon_entry(
        &mut out,
        "generated_at",
        &Value::String(envelope.generated_at.clone()),
        0,
    );
    render_toon_entry(&mut out, "applied_filters", &envelope.applied_filters, 0);
    render_toon_entry(&mut out, "truncated", &Value::Bool(envelope.truncated), 0);
    render_toon_entry(
        &mut out,
        "next_cursor",
        &envelope
            .next_cursor
            .as_ref()
            .map_or(Value::Null, |value| Value::String(value.clone())),
        0,
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
        out.push_str("  ");
        push_toon_scalars_tab_separated(&mut out, &values);
        out.push('\n');
    }

    out
}

pub(in crate::cli) fn render_recall_toon(envelope: &RecallEnvelope) -> String {
    let mut out = String::new();
    render_toon_entry(
        &mut out,
        "schema_version",
        &Value::from(envelope.schema_version as u64),
        0,
    );
    render_toon_entry(&mut out, "command", &Value::String("recall".to_string()), 0);
    render_toon_entry(
        &mut out,
        "generated_at",
        &Value::String(envelope.generated_at.clone()),
        0,
    );
    render_toon_entry(
        &mut out,
        "query",
        &envelope.query.clone().map_or(Value::Null, Value::String),
        0,
    );
    render_toon_entry(
        &mut out,
        "best_match_confidence",
        &serde_json::to_value(&envelope.best_match_confidence).unwrap_or(Value::Null),
        0,
    );
    render_toon_entry(
        &mut out,
        "best_match_score",
        &envelope.best_match_score.map_or(Value::Null, Value::from),
        0,
    );
    render_toon_entry(
        &mut out,
        "why_selected",
        &Value::String(envelope.why_selected.clone()),
        0,
    );
    if let Some(quoted_text) = &envelope.quoted_text {
        render_toon_entry(
            &mut out,
            "quoted_text",
            &Value::String(quoted_text.clone()),
            0,
        );
    }
    render_toon_entry(&mut out, "applied_filters", &envelope.applied_filters, 0);

    render_recall_rows_toon(
        &mut out,
        "best_candidate",
        std::slice::from_ref(&envelope.best_candidate),
    );
    render_recall_rows_toon(&mut out, "alternatives", &envelope.alternatives);

    out
}

pub(in crate::cli) fn render_recall_rows_toon(
    out: &mut String,
    key: &str,
    rows: &[RecallOutputRow],
) {
    let _ = writeln!(
        out,
        "{key}[#{}\t]{{{}}}:",
        rows.len(),
        ToonRecallRowProjection::FIELDS.join("\t")
    );
    for row in rows {
        let values = ToonRecallRowProjection::from_row(row).values();
        out.push_str("  ");
        push_toon_scalars_tab_separated(out, &values);
        out.push('\n');
    }
}

pub(in crate::cli) fn render_toon_entry(out: &mut String, key: &str, value: &Value, indent: usize) {
    let padding = " ".repeat(indent);
    match value {
        Value::Object(object) => {
            let _ = writeln!(out, "{padding}{key}:");
            render_toon_object_entries(out, object, indent + 2);
        }
        Value::Array(array) => render_toon_array(out, Some(key), array, indent),
        _ => {
            let _ = write!(out, "{padding}{key}: ");
            push_toon_scalar(out, value);
            out.push('\n');
        }
    }
}

pub(in crate::cli) fn render_toon_object_entries(
    out: &mut String,
    object: &serde_json::Map<String, Value>,
    indent: usize,
) {
    for (key, value) in object {
        render_toon_entry(out, key, value, indent);
    }
}

pub(in crate::cli) fn render_toon_array(
    out: &mut String,
    key: Option<&str>,
    values: &[Value],
    indent: usize,
) {
    let padding = " ".repeat(indent);
    let key_prefix = key
        .map(|name| format!("{padding}{name}"))
        .unwrap_or(padding);

    if values.iter().all(Value::is_null)
        || values
            .iter()
            .all(|value| !matches!(value, Value::Array(_) | Value::Object(_)))
    {
        if values.is_empty() {
            let _ = writeln!(out, "{key_prefix}[#0\t]:");
            return;
        }

        let _ = write!(out, "{key_prefix}[#{}\t]: ", values.len());
        push_toon_scalars_tab_separated(out, values);
        out.push('\n');
        return;
    }

    let _ = writeln!(out, "{key_prefix}[#{}]:", values.len());
    for value in values {
        render_toon_list_item(out, value, indent + 2);
    }
}

pub(in crate::cli) fn render_toon_list_item(out: &mut String, value: &Value, indent: usize) {
    let padding = " ".repeat(indent);
    match value {
        Value::Object(object) => render_toon_object_list_item(out, object, indent),
        Value::Array(array) => {
            let _ = writeln!(out, "{padding}-");
            render_toon_array(out, None, array, indent + 2);
        }
        _ => {
            let _ = write!(out, "{padding}- ");
            push_toon_scalar(out, value);
            out.push('\n');
        }
    }
}

pub(in crate::cli) fn render_toon_object_list_item(
    out: &mut String,
    object: &serde_json::Map<String, Value>,
    indent: usize,
) {
    let padding = " ".repeat(indent);
    let mut entries = object.iter();
    let Some((first_key, first_value)) = entries.next() else {
        let _ = writeln!(out, "{padding}-");
        return;
    };

    match first_value {
        Value::Object(nested) => {
            let _ = writeln!(out, "{padding}- {first_key}:");
            render_toon_object_entries(out, nested, indent + 4);
        }
        Value::Array(array) => {
            let _ = writeln!(out, "{padding}- {first_key}:");
            render_toon_array(out, None, array, indent + 4);
        }
        _ => {
            let _ = write!(out, "{padding}- {first_key}: ");
            push_toon_scalar(out, first_value);
            out.push('\n');
        }
    }

    for (key, value) in entries {
        render_toon_entry(out, key, value, indent + 2);
    }
}

pub(in crate::cli) fn push_toon_scalar(out: &mut String, value: &Value) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(flag) => {
            let _ = write!(out, "{flag}");
        }
        Value::Number(number) => {
            let _ = write!(out, "{number}");
        }
        Value::String(text) => push_toon_string(out, text),
        Value::Array(_) | Value::Object(_) => {
            unreachable!("push_toon_scalar only accepts primitive TOON values")
        }
    }
}

pub(in crate::cli) fn push_toon_string(out: &mut String, text: &str) {
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
        out.push('"');
        for character in text.chars() {
            match character {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                other => out.push(other),
            }
        }
        out.push('"');
    } else {
        out.push_str(text);
    }
}

pub(in crate::cli) fn push_toon_scalars_tab_separated(out: &mut String, values: &[Value]) {
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push('\t');
        }
        push_toon_scalar(out, value);
    }
}

fn estimated_list_toon_capacity(envelope: &ListEnvelope) -> usize {
    384 + envelope.results.len().saturating_mul(192)
}

pub(in crate::cli) fn render_filter_pairs(filters: &Value) -> String {
    let Some(object) = filters.as_object() else {
        return "none".to_string();
    };

    object
        .iter()
        .map(|(key, value)| {
            let rendered = match value {
                Value::Null => "null".to_string(),
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            format!("{key}={rendered}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(in crate::cli) const fn render_confidence_label(
    confidence: &RecallMatchConfidence,
) -> &'static str {
    match confidence {
        RecallMatchConfidence::High => "high",
        RecallMatchConfidence::Medium => "medium",
        RecallMatchConfidence::Low => "low",
    }
}

pub(in crate::cli) fn best_text_from_hit(hit: &SearchHit) -> String {
    if hit.preview_text().trim().is_empty() {
        hit.why_matched()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(hit.preview_text())
            .to_string()
    } else {
        hit.preview_text().to_string()
    }
}

pub(in crate::cli) fn escape_markdown_cell(value: &str) -> String {
    value.replace('|', "\\|")
}

pub(in crate::cli) fn truncate_for_markdown(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        value.to_string()
    } else {
        value
            .chars()
            .take(limit.saturating_sub(1))
            .collect::<String>()
            + "…"
    }
}

pub(in crate::cli) fn push_blank_line(out: &mut String) {
    out.push('\n');
}

pub(in crate::cli) fn peak_bucket(
    buckets: &[StatsTimeBucketEntry],
) -> Option<&StatsTimeBucketEntry> {
    buckets
        .iter()
        .max_by_key(|entry| {
            (
                entry.capture_event_count,
                std::cmp::Reverse(entry.bucket.as_str()),
            )
        })
        .filter(|entry| entry.capture_event_count > 0)
}

pub(in crate::cli) fn push_snapshot_leaderboard(
    out: &mut String,
    snapshots: &[StatsSnapshotLeaderboardEntry],
    indent: usize,
) {
    if snapshots.is_empty() {
        let _ = writeln!(out, "{}none", " ".repeat(indent));
        return;
    }

    for snapshot in snapshots {
        push_snapshot_leaderboard_entry(out, snapshot, indent);
    }
}

pub(in crate::cli) fn push_snapshot_leaderboard_entry(
    out: &mut String,
    snapshot: &StatsSnapshotLeaderboardEntry,
    indent: usize,
) {
    let prefix = " ".repeat(indent);
    let app = snapshot.app_name.as_deref().unwrap_or("Unknown");
    let preview = if snapshot.preview_text.trim().is_empty() {
        "(no preview)".to_string()
    } else {
        truncate_for_markdown(&snapshot.preview_text.replace('\n', " "), 120)
    };
    let _ = writeln!(
        out,
        "{prefix}[{}] {} captures, {}, {} bytes, last seen {}, {}",
        snapshot.snapshot_id,
        snapshot.capture_count,
        snapshot.kind,
        snapshot.total_bytes,
        format_utc_timestamp(&snapshot.last_observed_at),
        app
    );
    let _ = writeln!(out, "{prefix}  preview: {preview}");
}

pub(in crate::cli) fn format_duration_seconds(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let remainder = seconds % 60;

    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {remainder}s")
    } else {
        format!("{remainder}s")
    }
}

pub(in crate::cli) fn format_utc_timestamp(timestamp: &str) -> String {
    if timestamp.ends_with('Z')
        || timestamp.ends_with(" UTC")
        || timestamp.contains('+')
        || timestamp.rfind('-').is_some_and(|idx| idx > 9)
    {
        timestamp.to_string()
    } else {
        format!("{timestamp} UTC")
    }
}
