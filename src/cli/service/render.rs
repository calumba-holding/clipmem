use super::*;

pub(in crate::cli) fn render_setup_text(report: &SetupReport) -> String {
    let mut out = String::new();
    out.push_str("clipmem setup completed\n");
    let _ = write!(
        out,
        "provider: {}\nlabel: {}\nbinary: {}\ndatabase: {}\nseed_capture: {}\n",
        report.action.provider.as_str(),
        report.action.label,
        report.action.binary_path.display(),
        report.action.db_path.display(),
        render_seed_capture_outcome(report.seed_capture)
    );
    out.push_str("\nNext steps:\n");
    out.push_str("  clipmem service status\n");
    out.push_str("  clipmem doctor\n");
    for note in &report.action.notes {
        let _ = writeln!(out, "  note: {note}");
    }
    out
}

pub(in crate::cli) fn render_service_action_text(report: &ServiceActionReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "clipmem service {} completed", report.action);
    let _ = write!(
        out,
        "provider: {}\nlabel: {}\nbinary: {}\ndatabase: {}\n",
        report.provider.as_str(),
        report.label,
        report.binary_path.display(),
        report.db_path.display(),
    );
    for note in &report.notes {
        let _ = writeln!(out, "note: {note}");
    }
    out
}

pub(in crate::cli) fn render_service_status_text(report: &ServiceStatusReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "binary: {}", report.binary_path);
    let _ = writeln!(out, "database: {}", report.db_path);
    let _ = writeln!(
        out,
        "preferred provider: {} ({})",
        report.preferred_provider, report.preferred_provider_reason
    );
    out.push('\n');
    render_provider_status(&mut out, &report.homebrew);
    out.push('\n');
    render_provider_status(&mut out, &report.launchagent);
    out.push('\n');
    let _ = writeln!(out, "database exists: {}", report.db_exists);
    if let Some(db_size_bytes) = report.db_size_bytes {
        let _ = writeln!(out, "database size: {db_size_bytes} bytes");
    }
    if let Some(recent_capture_at) = &report.recent_capture_at {
        let _ = writeln!(out, "latest capture: {recent_capture_at}");
    } else {
        out.push_str("latest capture: none\n");
    }
    if let Some(fresh) = report.recent_capture_within_last_hour {
        let _ = writeln!(out, "capture within last hour: {fresh}");
    } else {
        out.push_str("capture within last hour: unknown\n");
    }
    if let Some(paused) = report.paused {
        let _ = writeln!(out, "paused: {paused}");
    } else {
        out.push_str("paused: unknown\n");
    }
    if let Some(enabled) = report.api_key_filter_enabled {
        let _ = writeln!(out, "api key filter: {enabled}");
    } else {
        out.push_str("api key filter: unknown\n");
    }
    if let Some(retention) = &report.retention {
        let _ = writeln!(out, "retention: {retention}");
    } else {
        out.push_str("retention: unknown\n");
    }
    if let Some(count) = report.ignored_bundle_id_count {
        let _ = writeln!(out, "ignored bundle ids: {count}");
    } else {
        out.push_str("ignored bundle ids: unknown\n");
    }
    let _ = writeln!(out, "stale: {}", report.stale);
    let _ = writeln!(
        out,
        "watcher binary mismatch: {}",
        report.watcher_binary_mismatch
    );
    if let Some(note) = &report.watcher_binary_mismatch_note {
        let _ = writeln!(out, "watcher binary mismatch note: {note}");
    }
    if let Some(db_error) = &report.db_error {
        let _ = writeln!(out, "database error: {db_error}");
    }
    if !report.notes.is_empty() {
        out.push_str("\nNotes:\n");
        for note in &report.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub(in crate::cli) fn render_provider_status(out: &mut String, status: &ServiceProviderStatus) {
    let _ = writeln!(out, "{} service:", status.provider.as_str());
    let _ = writeln!(out, "  label: {}", status.label);
    let _ = writeln!(out, "  state: {:?}", status.state);
    let _ = writeln!(out, "  installed: {}", status.installed);
    let _ = writeln!(out, "  loaded: {}", status.loaded);
    let _ = writeln!(out, "  running: {}", status.running);
    if let Some(pid) = status.pid {
        let _ = writeln!(out, "  pid: {pid}");
    }
    if let Some(path) = &status.plist_path {
        let _ = writeln!(out, "  plist: {path}");
    }
    if let Some(path) = &status.configured_binary_path {
        let _ = writeln!(out, "  configured binary: {path}");
    }
    if let Some(path) = &status.running_binary_path {
        let _ = writeln!(out, "  running binary: {path}");
    }
    if let Some(command) = &status.running_command {
        let _ = writeln!(out, "  running command: {command}");
    }
    if let Some(path) = &status.stdout_log_path {
        let _ = writeln!(out, "  stdout log: {path}");
    }
    if let Some(path) = &status.stderr_log_path {
        let _ = writeln!(out, "  stderr log: {path}");
    }
}

pub(in crate::cli) fn render_retention_value(settings: &CaptureSettings) -> String {
    settings
        .retention_seconds()
        .map_or_else(|| "forever".to_string(), format_duration_compact)
}

pub(in crate::cli) const fn render_seed_capture_outcome(
    outcome: SeedCaptureOutcome,
) -> &'static str {
    match outcome {
        SeedCaptureOutcome::Stored => "stored",
        SeedCaptureOutcome::Skipped(CaptureSkipReason::ApiKeyFilter) => "skipped_api_key_filter",
        SeedCaptureOutcome::Skipped(CaptureSkipReason::RestoredSnapshot) => {
            "skipped_restored_snapshot"
        }
        SeedCaptureOutcome::NotAttempted => "not_attempted",
    }
}

pub(in crate::cli) fn format_duration_compact(seconds: u64) -> String {
    let day = 24 * 60 * 60;
    let hour = 60 * 60;
    let minute = 60;

    if seconds.is_multiple_of(day) {
        format!("{}d", seconds / day)
    } else if seconds.is_multiple_of(hour) {
        format!("{}h", seconds / hour)
    } else if seconds.is_multiple_of(minute) {
        format!("{}m", seconds / minute)
    } else {
        format!("{seconds}s")
    }
}
