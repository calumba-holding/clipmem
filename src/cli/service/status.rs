use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use anyhow::Result;

use crate::db::Database;

use super::manage::{build_context, conflict_message, ensure_supported, select_provider};
use super::model::*;
use super::render::render_retention_value;

pub(crate) fn status_report(db_path: &Path) -> Result<ServiceStatusReport> {
    ensure_supported()?;
    let context = build_context(db_path)?;
    let direct_row = launchctl_row_probe(DIRECT_LABEL);
    let homebrew_row = launchctl_row_probe(HOMEBREW_LABEL);
    let direct_configured_binary =
        configured_binary_path_from_plist_probe(&context.direct_plist_path);
    let homebrew_configured_binary =
        configured_binary_path_from_plist_probe(&context.homebrew_plist_path);
    let direct_installed = context.direct_plist_path.is_file() || direct_row.value.is_some();
    let homebrew_installed = context.homebrew_plist_path.is_file() || homebrew_row.value.is_some();
    let direct_status = provider_status_probe(ProviderStatusInput {
        provider: ServiceProvider::Launchagent,
        label: DIRECT_LABEL,
        installed: direct_installed,
        row: direct_row.value,
        plist_path: Some(context.direct_plist_path.clone()),
        configured_binary_path: direct_configured_binary.value,
        stdout_log_path: Some(context.direct_stdout_path.clone()),
        stderr_log_path: Some(context.direct_stderr_path.clone()),
    });
    let homebrew_status = provider_status_probe(ProviderStatusInput {
        provider: ServiceProvider::Homebrew,
        label: HOMEBREW_LABEL,
        installed: homebrew_installed,
        row: homebrew_row.value,
        plist_path: Some(context.homebrew_plist_path.clone()),
        configured_binary_path: homebrew_configured_binary.value,
        stdout_log_path: context
            .homebrew_prefix
            .as_ref()
            .map(|prefix| prefix.join("var/log/clipmem.log")),
        stderr_log_path: context
            .homebrew_prefix
            .as_ref()
            .map(|prefix| prefix.join("var/log/clipmem.error.log")),
    });
    let mut probe_warnings = Vec::new();
    probe_warnings.extend(direct_row.warnings);
    probe_warnings.extend(homebrew_row.warnings);
    probe_warnings.extend(direct_configured_binary.warnings);
    probe_warnings.extend(homebrew_configured_binary.warnings);
    probe_warnings.extend(direct_status.warnings);
    probe_warnings.extend(homebrew_status.warnings);
    let direct_status = direct_status.value;
    let homebrew_status = homebrew_status.value;
    let conflict = homebrew_status.installed && direct_status.installed;
    let selection = select_provider(&context);

    let db_exists = context.db_path.is_file();
    let db_size_bytes = if db_exists {
        fs::metadata(&context.db_path)
            .map(|metadata| metadata.len())
            .ok()
    } else {
        None
    };
    let (
        recent_capture_at,
        recent_capture_within_last_hour,
        paused,
        api_key_filter_enabled,
        retention_seconds,
        retention,
        ignored_bundle_id_count,
        db_error,
    ) = if db_exists {
        match Database::open_existing(&context.db_path).and_then(|db| {
            let policy = db.capture_policy()?;
            Ok((
                db.latest_capture_observed_at()?,
                Some(db.has_capture_within_hours(SERVICE_FRESHNESS_HOURS)?),
                Some(policy.settings().paused()),
                Some(policy.settings().api_key_filter_enabled()),
                policy.settings().retention_seconds(),
                Some(render_retention_value(policy.settings())),
                Some(policy.ignored_bundle_id_count()),
            ))
        }) {
            Ok(fields) => (
                fields.0, fields.1, fields.2, fields.3, fields.4, fields.5, fields.6, None,
            ),
            Err(error) => (
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(error.to_string()),
            ),
        }
    } else {
        (None, None, None, None, None, None, None, None)
    };
    let stale = matches!(recent_capture_within_last_hour, Some(false))
        && !homebrew_status.running
        && !direct_status.running;

    let mut notes = selection.notes;
    notes.extend(probe_warnings);
    if conflict {
        notes.push(conflict_message());
    }
    if !db_exists {
        notes.push(format!(
            "Database does not exist yet at {}. Run `clipmem setup` to initialize capture.",
            context.db_path.display()
        ));
    }
    if stale {
        notes.push(
            "No recent captures were found and no background watcher is running.".to_string(),
        );
    }
    let watcher_binary_mismatch_note =
        watcher_binary_mismatch_note(&context.binary_path, [&homebrew_status, &direct_status]);
    let watcher_binary_mismatch = watcher_binary_mismatch_note.is_some();
    if let Some(note) = &watcher_binary_mismatch_note {
        notes.push(note.clone());
    }

    Ok(ServiceStatusReport {
        binary_path: context.binary_path.display().to_string(),
        db_path: context.db_path.display().to_string(),
        preferred_provider: selection.provider.as_str().to_string(),
        preferred_provider_reason: selection.reason,
        conflict,
        homebrew: homebrew_status,
        launchagent: direct_status,
        db_exists,
        db_size_bytes,
        recent_capture_at,
        recent_capture_within_last_hour,
        paused,
        api_key_filter_enabled,
        retention_seconds,
        retention,
        ignored_bundle_id_count,
        stale,
        db_error,
        watcher_binary_mismatch,
        watcher_binary_mismatch_note,
        notes,
    })
}

#[derive(Debug)]
struct ServiceProbe<T> {
    value: T,
    warnings: Vec<String>,
}

impl<T> ServiceProbe<T> {
    fn ok(value: T) -> Self {
        Self {
            value,
            warnings: Vec::new(),
        }
    }

    fn warning(value: T, warning: impl Into<String>) -> Self {
        Self {
            value,
            warnings: vec![warning.into()],
        }
    }
}

fn launchctl_row_probe(label: &str) -> ServiceProbe<Option<LaunchctlRow>> {
    let output = ProcessCommand::new("launchctl").arg("list").output();
    let output = match output {
        Ok(output) => output,
        Err(error) => {
            return ServiceProbe::warning(
                None,
                format!("Could not run `launchctl list` for {label}: {error}"),
            );
        }
    };
    if !output.status.success() {
        return ServiceProbe::warning(
            None,
            format!(
                "`launchctl list` exited with {} while probing {label}",
                output.status
            ),
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut warnings = Vec::new();
    for line in stdout.lines() {
        let mut parts = line.split_whitespace();
        let Some(pid) = parts.next() else {
            continue;
        };
        let _last_exit = parts.next();
        let Some(found_label) = parts.next() else {
            continue;
        };
        if found_label != label {
            continue;
        }
        let parsed_pid = if pid == "-" {
            None
        } else {
            match pid.parse::<i64>() {
                Ok(pid) => Some(pid),
                Err(error) => {
                    warnings.push(format!(
                        "`launchctl list` reported invalid pid `{pid}` for {label}: {error}"
                    ));
                    None
                }
            }
        };
        return ServiceProbe {
            value: Some(LaunchctlRow { pid: parsed_pid }),
            warnings,
        };
    }

    ServiceProbe::ok(None)
}

fn provider_status_probe(input: ProviderStatusInput) -> ServiceProbe<ServiceProviderStatus> {
    let ProviderStatusInput {
        provider,
        label,
        installed,
        row,
        plist_path,
        configured_binary_path,
        stdout_log_path,
        stderr_log_path,
    } = input;

    let (loaded, running, pid) = match row {
        Some(row) if row.pid.is_some() => (true, true, row.pid),
        Some(_) => (true, false, None),
        None => (false, false, None),
    };
    let running_command = pid.map(process_command_probe);
    let mut warnings = Vec::new();
    let running_command = running_command.and_then(|probe| {
        warnings.extend(probe.warnings);
        probe.value
    });
    let running_binary_path = running_command.as_deref().and_then(command_binary_path);
    let state = if running {
        ServiceState::Running
    } else if loaded {
        ServiceState::Loaded
    } else if installed {
        ServiceState::Installed
    } else {
        ServiceState::NotInstalled
    };

    ServiceProbe {
        value: ServiceProviderStatus {
            provider,
            label: label.to_string(),
            state,
            installed,
            loaded,
            running,
            pid,
            plist_path: plist_path.map(|path| path.display().to_string()),
            configured_binary_path,
            running_command,
            running_binary_path,
            stdout_log_path: stdout_log_path.map(|path| path.display().to_string()),
            stderr_log_path: stderr_log_path.map(|path| path.display().to_string()),
        },
        warnings,
    }
}

#[cfg(test)]
pub(crate) fn configured_binary_path_from_plist(plist_path: &Path) -> Option<String> {
    configured_binary_path_from_plist_probe(plist_path).value
}

fn configured_binary_path_from_plist_probe(plist_path: &Path) -> ServiceProbe<Option<String>> {
    if !plist_path.is_file() {
        return ServiceProbe::ok(None);
    }

    let output = ProcessCommand::new("plutil")
        .args([
            "-extract",
            "ProgramArguments.0",
            "raw",
            "-o",
            "-",
            &plist_path.display().to_string(),
        ])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !value.is_empty() {
                return ServiceProbe::ok(Some(value));
            }
            let mut fallback = configured_binary_path_from_plist_xml_probe(plist_path);
            fallback.warnings.insert(
                0,
                format!(
                    "`plutil` returned an empty ProgramArguments.0 for {}",
                    plist_path.display()
                ),
            );
            fallback
        }
        Ok(output) => {
            let mut fallback = configured_binary_path_from_plist_xml_probe(plist_path);
            fallback.warnings.insert(
                0,
                format!(
                    "`plutil` exited with {} while reading {}",
                    output.status,
                    plist_path.display()
                ),
            );
            fallback
        }
        Err(error) => {
            let mut fallback = configured_binary_path_from_plist_xml_probe(plist_path);
            fallback.warnings.insert(
                0,
                format!(
                    "Could not run `plutil` for {}: {error}",
                    plist_path.display()
                ),
            );
            fallback
        }
    }
}

#[cfg(test)]
pub(crate) fn configured_binary_path_from_plist_xml(plist_path: &Path) -> Option<String> {
    configured_binary_path_from_plist_xml_probe(plist_path).value
}

fn configured_binary_path_from_plist_xml_probe(plist_path: &Path) -> ServiceProbe<Option<String>> {
    let plist = match fs::read_to_string(plist_path) {
        Ok(plist) => plist,
        Err(error) => {
            return ServiceProbe::warning(
                None,
                format!(
                    "Could not read plist XML fallback {}: {error}",
                    plist_path.display()
                ),
            );
        }
    };
    let Some(program_args) = plist.find("<key>ProgramArguments</key>") else {
        return ServiceProbe::warning(
            None,
            format!("Plist {} has no ProgramArguments key", plist_path.display()),
        );
    };
    let after_key = &plist[program_args..];
    let Some(array_start) = after_key.find("<array") else {
        return ServiceProbe::warning(
            None,
            format!(
                "Plist {} has ProgramArguments without an array",
                plist_path.display()
            ),
        );
    };
    let Some(array_tag_end) = after_key[array_start..].find('>') else {
        return ServiceProbe::warning(
            None,
            format!(
                "Plist {} has malformed ProgramArguments array",
                plist_path.display()
            ),
        );
    };
    let after_array_tag = array_tag_end + array_start + 1;
    let array = &after_key[after_array_tag..];
    let Some(array_end) = array.find("</array>") else {
        return ServiceProbe::warning(
            None,
            format!(
                "Plist {} has unterminated ProgramArguments array",
                plist_path.display()
            ),
        );
    };
    let array = &array[..array_end];
    let Some(string_start) = array.find("<string>").map(|index| index + "<string>".len()) else {
        return ServiceProbe::warning(
            None,
            format!(
                "Plist {} has no ProgramArguments string value",
                plist_path.display()
            ),
        );
    };
    let Some(string_end) = array[string_start..]
        .find("</string>")
        .map(|index| index + string_start)
    else {
        return ServiceProbe::warning(
            None,
            format!(
                "Plist {} has unterminated ProgramArguments string value",
                plist_path.display()
            ),
        );
    };
    let value = decode_plist_xml_text(array[string_start..string_end].trim());
    if value.is_empty() {
        ServiceProbe::warning(
            None,
            format!(
                "Plist {} has an empty ProgramArguments string value",
                plist_path.display()
            ),
        )
    } else {
        ServiceProbe::ok(Some(value))
    }
}

fn process_command_probe(pid: i64) -> ServiceProbe<Option<String>> {
    let output = ProcessCommand::new("ps")
        .args(["-ww", "-p", &pid.to_string(), "-o", "command="])
        .output();
    let output = match output {
        Ok(output) => output,
        Err(error) => {
            return ServiceProbe::warning(
                None,
                format!("Could not run `ps` for watcher pid {pid}: {error}"),
            );
        }
    };
    if !output.status.success() {
        return ServiceProbe::warning(
            None,
            format!("`ps` exited with {} for watcher pid {pid}", output.status),
        );
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        ServiceProbe::warning(
            None,
            format!("`ps` returned an empty command for pid {pid}"),
        )
    } else {
        ServiceProbe::ok(Some(value))
    }
}

pub(in crate::cli) struct ProviderStatusInput {
    provider: ServiceProvider,
    label: &'static str,
    installed: bool,
    row: Option<LaunchctlRow>,
    plist_path: Option<PathBuf>,
    configured_binary_path: Option<String>,
    stdout_log_path: Option<PathBuf>,
    stderr_log_path: Option<PathBuf>,
}

pub(in crate::cli) fn decode_plist_xml_text(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

pub(crate) fn command_binary_path(command: &str) -> Option<String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix('"') {
        let end = rest.find('"')?;
        let value = &rest[..end];
        return (!value.is_empty()).then_some(value.to_string());
    }
    if let Some(rest) = trimmed.strip_prefix('\'') {
        let end = rest.find('\'')?;
        let value = &rest[..end];
        return (!value.is_empty()).then_some(value.to_string());
    }
    trimmed.split_whitespace().next().map(ToString::to_string)
}

pub(crate) fn watcher_binary_mismatch_note<'a>(
    current_binary_path: &Path,
    providers: impl IntoIterator<Item = &'a ServiceProviderStatus>,
) -> Option<String> {
    for provider in providers {
        if !(provider.installed || provider.loaded || provider.running) {
            continue;
        }
        let candidates: Vec<&str> = provider.running_binary_path.as_deref().map_or_else(
            || {
                provider
                    .configured_binary_path
                    .iter()
                    .map(String::as_str)
                    .collect()
            },
            |running| vec![running],
        );
        for candidate in candidates {
            if !paths_equivalent(current_binary_path, Path::new(candidate)) {
                return Some(format!(
                    "{} watcher uses {}, but this command is {}. Restart the watcher with the same clipmem binary to avoid mixed-build behavior.",
                    provider.provider.as_str(),
                    candidate,
                    current_binary_path.display()
                ));
            }
        }
    }
    None
}

pub(in crate::cli) fn paths_equivalent(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}
