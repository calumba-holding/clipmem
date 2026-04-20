use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;

use crate::cli::db_path::default_db_path;
use crate::db::{CaptureSettings, CaptureSkipReason, CaptureStoreOutcome, Database};
use crate::platform::capture_snapshot;

const DIRECT_LABEL: &str = "io.openclaw.clipmem.watch";
const HOMEBREW_LABEL: &str = "homebrew.mxcl.clipmem";
const DEFAULT_INTERVAL_MS: u64 = 350;
const SERVICE_FRESHNESS_HOURS: u32 = 1;
const DIRECT_PLIST_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/extras/launchd/io.openclaw.clipmem.watch.plist.template"
));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ServiceProvider {
    Homebrew,
    Launchagent,
}

impl ServiceProvider {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Homebrew => "homebrew",
            Self::Launchagent => "launchagent",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ServiceState {
    NotInstalled,
    Installed,
    Loaded,
    Running,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct ServiceProviderStatus {
    pub(super) provider: ServiceProvider,
    pub(super) label: String,
    pub(super) state: ServiceState,
    pub(super) installed: bool,
    pub(super) loaded: bool,
    pub(super) running: bool,
    pub(super) pid: Option<i64>,
    pub(super) plist_path: Option<String>,
    pub(super) stdout_log_path: Option<String>,
    pub(super) stderr_log_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct ServiceStatusReport {
    pub(super) binary_path: String,
    pub(super) db_path: String,
    pub(super) preferred_provider: String,
    pub(super) preferred_provider_reason: String,
    pub(super) conflict: bool,
    pub(super) homebrew: ServiceProviderStatus,
    pub(super) launchagent: ServiceProviderStatus,
    pub(super) db_exists: bool,
    pub(super) db_size_bytes: Option<u64>,
    pub(super) recent_capture_at: Option<String>,
    pub(super) recent_capture_within_last_hour: Option<bool>,
    pub(super) paused: Option<bool>,
    pub(super) api_key_filter_enabled: Option<bool>,
    pub(super) retention_seconds: Option<u64>,
    pub(super) retention: Option<String>,
    pub(super) ignored_bundle_id_count: Option<usize>,
    pub(super) stale: bool,
    pub(super) db_error: Option<String>,
    pub(super) notes: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct ServiceActionReport {
    pub(super) action: &'static str,
    pub(super) provider: ServiceProvider,
    pub(super) binary_path: PathBuf,
    pub(super) db_path: PathBuf,
    pub(super) label: &'static str,
    pub(super) notes: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct SetupReport {
    pub(super) seed_capture: SeedCaptureOutcome,
    pub(super) action: ServiceActionReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SeedCaptureOutcome {
    Stored,
    Skipped(CaptureSkipReason),
    NotAttempted,
}

#[derive(Debug, Clone)]
struct ServiceContext {
    binary_path: PathBuf,
    db_path: PathBuf,
    default_db_path: PathBuf,
    direct_plist_path: PathBuf,
    homebrew_plist_path: PathBuf,
    direct_stdout_path: PathBuf,
    direct_stderr_path: PathBuf,
    brew_path: Option<PathBuf>,
    homebrew_prefix: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct ProviderSelection {
    provider: ServiceProvider,
    reason: String,
    notes: Vec<String>,
}

#[derive(Debug, Clone)]
struct LaunchctlRow {
    pid: Option<i64>,
}

pub(super) fn setup(db_path: &Path) -> Result<SetupReport> {
    ensure_supported()?;
    let context = build_context(db_path)?;
    let status = status_report(db_path)?;
    let selection = select_provider(&context)?;
    ensure_no_conflict(&status)?;
    let seed_capture = seed_capture(db_path)?;
    let action = start_with_provider(&context, &selection)?;
    Ok(SetupReport {
        seed_capture,
        action,
    })
}

pub(super) fn start(db_path: &Path) -> Result<ServiceActionReport> {
    ensure_supported()?;
    let context = build_context(db_path)?;
    let status = status_report(db_path)?;
    let selection = select_provider(&context)?;
    ensure_no_conflict(&status)?;
    start_with_provider(&context, &selection)
}

pub(super) fn stop(db_path: &Path) -> Result<ServiceActionReport> {
    ensure_supported()?;
    let context = build_context(db_path)?;
    let status = status_report(db_path)?;
    if status.conflict {
        bail!(conflict_message());
    }

    if status.homebrew.installed || status.homebrew.loaded || status.homebrew.running {
        stop_homebrew_provider(&context)
    } else {
        stop_direct_provider(&context)
    }
}

pub(super) fn uninstall(db_path: &Path) -> Result<ServiceActionReport> {
    ensure_supported()?;
    let context = build_context(db_path)?;
    let status = status_report(db_path)?;
    if status.conflict {
        bail!(conflict_message());
    }

    if status.homebrew.installed || status.homebrew.loaded || status.homebrew.running {
        uninstall_homebrew_provider(&context)
    } else {
        uninstall_direct_provider(&context)
    }
}

pub(super) fn status_report(db_path: &Path) -> Result<ServiceStatusReport> {
    ensure_supported()?;
    let context = build_context(db_path)?;
    let direct_row = launchctl_row(DIRECT_LABEL)?;
    let homebrew_row = launchctl_row(HOMEBREW_LABEL)?;
    let direct_installed = context.direct_plist_path.is_file() || direct_row.is_some();
    let homebrew_installed = context.homebrew_plist_path.is_file() || homebrew_row.is_some();
    let direct_status = provider_status(
        ServiceProvider::Launchagent,
        DIRECT_LABEL,
        direct_installed,
        direct_row,
        Some(context.direct_plist_path.clone()),
        Some(context.direct_stdout_path.clone()),
        Some(context.direct_stderr_path.clone()),
    );
    let homebrew_status = provider_status(
        ServiceProvider::Homebrew,
        HOMEBREW_LABEL,
        homebrew_installed,
        homebrew_row,
        Some(context.homebrew_plist_path.clone()),
        context
            .homebrew_prefix
            .as_ref()
            .map(|prefix| prefix.join("var/log/clipmem.log")),
        context
            .homebrew_prefix
            .as_ref()
            .map(|prefix| prefix.join("var/log/clipmem.error.log")),
    );
    let conflict = homebrew_status.installed && direct_status.installed;
    let selection = select_provider(&context)?;

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
        match Database::open_existing(&context.db_path) {
            Ok(db) => {
                let policy = db.capture_policy()?;
                (
                    db.latest_capture_observed_at()?,
                    Some(db.has_capture_within_hours(SERVICE_FRESHNESS_HOURS)?),
                    Some(policy.settings().paused()),
                    Some(policy.settings().api_key_filter_enabled()),
                    policy.settings().retention_seconds(),
                    Some(render_retention_value(policy.settings())),
                    Some(policy.ignored_bundle_id_count()),
                    None,
                )
            }
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
        notes,
    })
}

pub(super) fn render_setup_text(report: &SetupReport) -> String {
    let mut out = String::new();
    out.push_str("clipmem setup completed\n");
    out.push_str(&format!(
        "provider: {}\nlabel: {}\nbinary: {}\ndatabase: {}\nseed_capture: {}\n",
        report.action.provider.as_str(),
        report.action.label,
        report.action.binary_path.display(),
        report.action.db_path.display(),
        render_seed_capture_outcome(report.seed_capture)
    ));
    out.push_str("\nNext steps:\n");
    out.push_str("  clipmem service status\n");
    out.push_str("  clipmem doctor\n");
    for note in &report.action.notes {
        out.push_str(&format!("  note: {note}\n"));
    }
    out
}

pub(super) fn render_service_action_text(report: &ServiceActionReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("clipmem service {} completed\n", report.action));
    out.push_str(&format!(
        "provider: {}\nlabel: {}\nbinary: {}\ndatabase: {}\n",
        report.provider.as_str(),
        report.label,
        report.binary_path.display(),
        report.db_path.display(),
    ));
    for note in &report.notes {
        out.push_str(&format!("note: {note}\n"));
    }
    out
}

pub(super) fn render_service_status_text(report: &ServiceStatusReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("binary: {}\n", report.binary_path));
    out.push_str(&format!("database: {}\n", report.db_path));
    out.push_str(&format!(
        "preferred provider: {} ({})\n",
        report.preferred_provider, report.preferred_provider_reason
    ));
    out.push('\n');
    render_provider_status(&mut out, &report.homebrew);
    out.push('\n');
    render_provider_status(&mut out, &report.launchagent);
    out.push('\n');
    out.push_str(&format!("database exists: {}\n", report.db_exists));
    if let Some(db_size_bytes) = report.db_size_bytes {
        out.push_str(&format!("database size: {db_size_bytes} bytes\n"));
    }
    if let Some(recent_capture_at) = &report.recent_capture_at {
        out.push_str(&format!("latest capture: {recent_capture_at}\n"));
    } else {
        out.push_str("latest capture: none\n");
    }
    if let Some(fresh) = report.recent_capture_within_last_hour {
        out.push_str(&format!("capture within last hour: {fresh}\n"));
    } else {
        out.push_str("capture within last hour: unknown\n");
    }
    if let Some(paused) = report.paused {
        out.push_str(&format!("paused: {paused}\n"));
    } else {
        out.push_str("paused: unknown\n");
    }
    if let Some(enabled) = report.api_key_filter_enabled {
        out.push_str(&format!("api key filter: {enabled}\n"));
    } else {
        out.push_str("api key filter: unknown\n");
    }
    if let Some(retention) = &report.retention {
        out.push_str(&format!("retention: {retention}\n"));
    } else {
        out.push_str("retention: unknown\n");
    }
    if let Some(count) = report.ignored_bundle_id_count {
        out.push_str(&format!("ignored bundle ids: {count}\n"));
    } else {
        out.push_str("ignored bundle ids: unknown\n");
    }
    out.push_str(&format!("stale: {}\n", report.stale));
    if let Some(db_error) = &report.db_error {
        out.push_str(&format!("database error: {db_error}\n"));
    }
    if !report.notes.is_empty() {
        out.push_str("\nNotes:\n");
        for note in &report.notes {
            out.push_str(&format!("  - {note}\n"));
        }
    }
    out
}

fn render_provider_status(out: &mut String, status: &ServiceProviderStatus) {
    out.push_str(&format!("{} service:\n", status.provider.as_str()));
    out.push_str(&format!("  label: {}\n", status.label));
    out.push_str(&format!("  state: {:?}\n", status.state));
    out.push_str(&format!("  installed: {}\n", status.installed));
    out.push_str(&format!("  loaded: {}\n", status.loaded));
    out.push_str(&format!("  running: {}\n", status.running));
    if let Some(pid) = status.pid {
        out.push_str(&format!("  pid: {pid}\n"));
    }
    if let Some(path) = &status.plist_path {
        out.push_str(&format!("  plist: {path}\n"));
    }
    if let Some(path) = &status.stdout_log_path {
        out.push_str(&format!("  stdout log: {path}\n"));
    }
    if let Some(path) = &status.stderr_log_path {
        out.push_str(&format!("  stderr log: {path}\n"));
    }
}

fn ensure_supported() -> Result<()> {
    if cfg!(target_os = "macos") {
        Ok(())
    } else {
        bail!("clipmem setup and service commands are only supported on macOS");
    }
}

fn build_context(db_path: &Path) -> Result<ServiceContext> {
    let home = home_dir()?;
    let binary_path = trusted_binary_path()?;
    let direct_plist_path = home
        .join("Library/LaunchAgents")
        .join(format!("{DIRECT_LABEL}.plist"));
    let homebrew_plist_path = home
        .join("Library/LaunchAgents")
        .join(format!("{HOMEBREW_LABEL}.plist"));
    let app_support_dir = db_path.parent().ok_or_else(|| {
        anyhow!(
            "database path {} has no parent directory",
            db_path.display()
        )
    })?;
    let log_dir = app_support_dir.join("logs");
    let brew_path = find_executable("brew");
    let homebrew_prefix = homebrew_prefix_for_binary(&binary_path);

    Ok(ServiceContext {
        binary_path,
        db_path: db_path.to_path_buf(),
        default_db_path: default_db_path(),
        direct_plist_path,
        homebrew_plist_path,
        direct_stdout_path: log_dir.join("clipmem.stdout.log"),
        direct_stderr_path: log_dir.join("clipmem.stderr.log"),
        brew_path,
        homebrew_prefix,
    })
}

fn select_provider(context: &ServiceContext) -> Result<ProviderSelection> {
    let mut notes = Vec::new();
    if let Some(brew_path) = &context.brew_path {
        if context.homebrew_prefix.is_some() {
            if brew_services_available(brew_path) && brew_services_can_manage_clipmem(brew_path) {
                return Ok(ProviderSelection {
                    provider: ServiceProvider::Homebrew,
                    reason:
                        "current binary is the Homebrew formula and `brew services` is available"
                            .to_string(),
                    notes,
                });
            }
            notes.push(
                "Homebrew install detected, but `brew services` cannot manage the clipmem formula; falling back to a direct LaunchAgent.".to_string(),
            );
        }
    }

    if context.db_path != context.default_db_path {
        notes.push(format!(
            "Direct LaunchAgent provider will use the explicit database path {}.",
            context.db_path.display()
        ));
    }

    Ok(ProviderSelection {
        provider: ServiceProvider::Launchagent,
        reason: "using the direct per-user LaunchAgent managed by clipmem".to_string(),
        notes,
    })
}

fn ensure_no_conflict(status: &ServiceStatusReport) -> Result<()> {
    if status.conflict {
        bail!(conflict_message());
    }
    Ok(())
}

fn conflict_message() -> String {
    "Both the Homebrew service and the direct LaunchAgent are installed. Remove one first with `brew services stop clipmem` or `clipmem service uninstall`.".to_string()
}

fn render_retention_value(settings: &CaptureSettings) -> String {
    settings
        .retention_seconds()
        .map(format_duration_compact)
        .unwrap_or_else(|| "forever".to_string())
}

fn render_seed_capture_outcome(outcome: SeedCaptureOutcome) -> &'static str {
    match outcome {
        SeedCaptureOutcome::Stored => "stored",
        SeedCaptureOutcome::Skipped(CaptureSkipReason::ApiKeyFilter) => "skipped_api_key_filter",
        SeedCaptureOutcome::NotAttempted => "not_attempted",
    }
}

fn format_duration_compact(seconds: u64) -> String {
    let day = 24 * 60 * 60;
    let hour = 60 * 60;
    let minute = 60;

    if seconds % day == 0 {
        format!("{}d", seconds / day)
    } else if seconds % hour == 0 {
        format!("{}h", seconds / hour)
    } else if seconds % minute == 0 {
        format!("{}m", seconds / minute)
    } else {
        format!("{seconds}s")
    }
}

fn start_with_provider(
    context: &ServiceContext,
    selection: &ProviderSelection,
) -> Result<ServiceActionReport> {
    match selection.provider {
        ServiceProvider::Homebrew => start_homebrew_provider(context, &selection.notes),
        ServiceProvider::Launchagent => start_direct_provider(context, &selection.notes),
    }
}

fn start_homebrew_provider(
    context: &ServiceContext,
    notes: &[String],
) -> Result<ServiceActionReport> {
    let brew = context
        .brew_path
        .as_ref()
        .ok_or_else(|| anyhow!("`brew` is not available on PATH"))?;
    let output = ProcessCommand::new(brew)
        .args(["services", "start", "clipmem"])
        .output()
        .context("run `brew services start clipmem`")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if brew_services_formula_unavailable(&stderr) {
            let mut fallback_notes = notes.to_vec();
            fallback_notes.push(
                "`brew services` is installed, but the clipmem formula does not expose a service file; using a direct LaunchAgent with the Homebrew binary.".to_string(),
            );
            return start_direct_provider(context, &fallback_notes);
        }

        run_command_checked(Ok(output), "brew services start clipmem")?;
    }
    Ok(ServiceActionReport {
        action: "start",
        provider: ServiceProvider::Homebrew,
        binary_path: context.binary_path.clone(),
        db_path: context.default_db_path.clone(),
        label: HOMEBREW_LABEL,
        notes: notes.to_vec(),
    })
}

fn brew_services_formula_unavailable(stderr: &str) -> bool {
    stderr.contains("has not implemented #plist")
        || stderr.contains("has not implemented #service")
        || stderr.contains("provided a locatable service file")
}

fn brew_services_can_manage_clipmem(brew: &Path) -> bool {
    let Ok(output) = ProcessCommand::new(brew)
        .args(["services", "info", "clipmem", "--json"])
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.contains("\"schedulable\":true") || stdout.contains("\"schedulable\": true")
}

fn stop_homebrew_provider(context: &ServiceContext) -> Result<ServiceActionReport> {
    let brew = context
        .brew_path
        .as_ref()
        .ok_or_else(|| anyhow!("`brew` is not available on PATH"))?;
    run_command_checked(
        ProcessCommand::new(brew)
            .args(["services", "stop", "clipmem"])
            .output(),
        "brew services stop clipmem",
    )?;
    Ok(ServiceActionReport {
        action: "stop",
        provider: ServiceProvider::Homebrew,
        binary_path: context.binary_path.clone(),
        db_path: context.default_db_path.clone(),
        label: HOMEBREW_LABEL,
        notes: vec!["The Homebrew formula remains installed.".to_string()],
    })
}

fn uninstall_homebrew_provider(context: &ServiceContext) -> Result<ServiceActionReport> {
    let mut report = stop_homebrew_provider(context)?;
    report.action = "uninstall";
    report.notes.push(
        "Only the Homebrew-managed service was removed; the formula stays installed.".to_string(),
    );
    Ok(report)
}

fn start_direct_provider(
    context: &ServiceContext,
    notes: &[String],
) -> Result<ServiceActionReport> {
    ensure_parent_dir(&context.direct_plist_path)?;
    ensure_parent_dir(&context.direct_stdout_path)?;
    touch_log_file(&context.direct_stdout_path)?;
    touch_log_file(&context.direct_stderr_path)?;
    write_direct_plist(context)?;
    launchctl_bootout(DIRECT_LABEL)?;
    launchctl_enable(DIRECT_LABEL)?;
    launchctl_bootstrap(&context.direct_plist_path)?;
    launchctl_kickstart(DIRECT_LABEL)?;
    Ok(ServiceActionReport {
        action: "start",
        provider: ServiceProvider::Launchagent,
        binary_path: context.binary_path.clone(),
        db_path: context.db_path.clone(),
        label: DIRECT_LABEL,
        notes: notes.to_vec(),
    })
}

fn stop_direct_provider(context: &ServiceContext) -> Result<ServiceActionReport> {
    launchctl_bootout(DIRECT_LABEL)?;
    launchctl_disable(DIRECT_LABEL)?;
    Ok(ServiceActionReport {
        action: "stop",
        provider: ServiceProvider::Launchagent,
        binary_path: context.binary_path.clone(),
        db_path: context.db_path.clone(),
        label: DIRECT_LABEL,
        notes: vec!["The LaunchAgent plist remains installed.".to_string()],
    })
}

fn uninstall_direct_provider(context: &ServiceContext) -> Result<ServiceActionReport> {
    launchctl_bootout(DIRECT_LABEL)?;
    launchctl_disable(DIRECT_LABEL)?;
    let _ = fs::remove_file(&context.direct_plist_path);
    Ok(ServiceActionReport {
        action: "uninstall",
        provider: ServiceProvider::Launchagent,
        binary_path: context.binary_path.clone(),
        db_path: context.db_path.clone(),
        label: DIRECT_LABEL,
        notes: vec!["Removed the direct LaunchAgent plist.".to_string()],
    })
}

fn seed_capture(db_path: &Path) -> Result<SeedCaptureOutcome> {
    if env::var_os("CLIPMEM_TEST_SKIP_SETUP_CAPTURE_ONCE").as_deref() == Some("1".as_ref()) {
        return Ok(SeedCaptureOutcome::NotAttempted);
    }

    let mut db = Database::open_or_init(db_path)?;
    let snapshot = capture_snapshot().context("setup clipboard read failed")?;
    match db
        .store_capture_if_allowed(&snapshot)
        .context("setup database write failed")?
    {
        CaptureStoreOutcome::Stored(_) => Ok(SeedCaptureOutcome::Stored),
        CaptureStoreOutcome::Skipped(reason) => Ok(SeedCaptureOutcome::Skipped(reason)),
    }
}

fn launchctl_row(label: &str) -> Result<Option<LaunchctlRow>> {
    let output = ProcessCommand::new("launchctl")
        .arg("list")
        .output()
        .context("run launchctl list")?;
    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
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
        return Ok(Some(LaunchctlRow {
            pid: if pid == "-" {
                None
            } else {
                pid.parse::<i64>().ok()
            },
        }));
    }

    Ok(None)
}

fn provider_status(
    provider: ServiceProvider,
    label: &str,
    installed: bool,
    row: Option<LaunchctlRow>,
    plist_path: Option<PathBuf>,
    stdout_log_path: Option<PathBuf>,
    stderr_log_path: Option<PathBuf>,
) -> ServiceProviderStatus {
    let (loaded, running, pid) = match row {
        Some(row) if row.pid.is_some() => (true, true, row.pid),
        Some(_) => (true, false, None),
        None => (false, false, None),
    };
    let state = if running {
        ServiceState::Running
    } else if loaded {
        ServiceState::Loaded
    } else if installed {
        ServiceState::Installed
    } else {
        ServiceState::NotInstalled
    };

    ServiceProviderStatus {
        provider,
        label: label.to_string(),
        state,
        installed,
        loaded,
        running,
        pid,
        plist_path: plist_path.map(|path| path.display().to_string()),
        stdout_log_path: stdout_log_path.map(|path| path.display().to_string()),
        stderr_log_path: stderr_log_path.map(|path| path.display().to_string()),
    }
}

fn write_direct_plist(context: &ServiceContext) -> Result<()> {
    let plist = DIRECT_PLIST_TEMPLATE
        .replace(
            "{{CLIPMEM_BIN}}",
            &xml_escape(&context.binary_path.display().to_string()),
        )
        .replace(
            "{{CLIPMEM_DB_PATH}}",
            &xml_escape(&context.db_path.display().to_string()),
        )
        .replace("{{CLIPMEM_INTERVAL_MS}}", &DEFAULT_INTERVAL_MS.to_string())
        .replace(
            "{{STDOUT_PATH}}",
            &xml_escape(&context.direct_stdout_path.display().to_string()),
        )
        .replace(
            "{{STDERR_PATH}}",
            &xml_escape(&context.direct_stderr_path.display().to_string()),
        );
    fs::write(&context.direct_plist_path, plist)
        .with_context(|| format!("write {}", context.direct_plist_path.display()))?;
    set_mode_600(&context.direct_plist_path)?;
    Ok(())
}

fn run_command_checked(output: std::io::Result<std::process::Output>, command: &str) -> Result<()> {
    let output = output.with_context(|| format!("run `{command}`"))?;
    if output.status.success() {
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    bail!(
        "`{command}` failed with {}.\nstdout:\n{}\nstderr:\n{}",
        output.status,
        stdout.trim(),
        stderr.trim()
    );
}

fn launchctl_bootout(label: &str) -> Result<()> {
    let _ = ProcessCommand::new("launchctl")
        .args(["bootout", &format!("gui/{}/{}", uid()?, label)])
        .output();
    Ok(())
}

fn launchctl_bootstrap(plist_path: &Path) -> Result<()> {
    run_command_checked(
        ProcessCommand::new("launchctl")
            .args([
                "bootstrap",
                &format!("gui/{}", uid()?),
                &plist_path.display().to_string(),
            ])
            .output(),
        "launchctl bootstrap",
    )
}

fn launchctl_enable(label: &str) -> Result<()> {
    run_command_checked(
        ProcessCommand::new("launchctl")
            .args(["enable", &format!("gui/{}/{}", uid()?, label)])
            .output(),
        "launchctl enable",
    )
}

fn launchctl_disable(label: &str) -> Result<()> {
    let _ = ProcessCommand::new("launchctl")
        .args(["disable", &format!("gui/{}/{}", uid()?, label)])
        .output();
    Ok(())
}

fn launchctl_kickstart(label: &str) -> Result<()> {
    run_command_checked(
        ProcessCommand::new("launchctl")
            .args(["kickstart", "-k", &format!("gui/{}/{}", uid()?, label)])
            .output(),
        "launchctl kickstart",
    )
}

fn uid() -> Result<String> {
    if let Some(uid) = env::var_os("UID") {
        return Ok(uid.to_string_lossy().into_owned());
    }

    let output = ProcessCommand::new("id")
        .arg("-u")
        .output()
        .context("run `id -u`")?;
    if !output.status.success() {
        bail!("`id -u` failed with {}", output.status);
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    set_mode_700(parent)?;
    Ok(())
}

fn touch_log_file(path: &Path) -> Result<()> {
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("create {}", path.display()))?;
    set_mode_600(path)?;
    Ok(())
}

#[cfg(unix)]
fn set_mode_700(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o700);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_mode_700(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_mode_600(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_mode_600(_path: &Path) -> Result<()> {
    Ok(())
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn brew_services_available(brew_path: &Path) -> bool {
    ProcessCommand::new(brew_path)
        .args(["services", "list"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn trusted_binary_path() -> Result<PathBuf> {
    if let Some(path) = env::var_os("CLIPMEM_TEST_ACTIVE_BINARY") {
        return Ok(PathBuf::from(path));
    }

    env::current_exe().context("resolve current executable path")
}

fn find_executable(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path).find_map(|dir| {
        let candidate = dir.join(name);
        candidate.is_file().then_some(candidate)
    })
}

fn homebrew_prefix_for_binary(path: &Path) -> Option<PathBuf> {
    for prefix in ["/opt/homebrew", "/usr/local"] {
        let prefix = Path::new(prefix);
        if path == prefix.join("bin/clipmem") {
            return Some(prefix.to_path_buf());
        }

        let Ok(relative_path) = path.strip_prefix(prefix) else {
            continue;
        };
        let parts: Vec<_> = relative_path.iter().collect();
        if parts.len() == 5
            && parts[0] == "Cellar"
            && parts[1] == "clipmem"
            && !parts[2].is_empty()
            && parts[3] == "bin"
            && parts[4] == "clipmem"
        {
            return Some(prefix.to_path_buf());
        }
    }

    None
}

fn home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME is not set"))
}

#[cfg(test)]
mod tests {
    use super::homebrew_prefix_for_binary;
    use std::path::{Path, PathBuf};

    #[test]
    fn homebrew_prefix_matches_opt_homebrew_wrapper_path() {
        assert_eq!(
            homebrew_prefix_for_binary(Path::new("/opt/homebrew/bin/clipmem")),
            Some(PathBuf::from("/opt/homebrew"))
        );
    }

    #[test]
    fn homebrew_prefix_matches_usr_local_wrapper_path() {
        assert_eq!(
            homebrew_prefix_for_binary(Path::new("/usr/local/bin/clipmem")),
            Some(PathBuf::from("/usr/local"))
        );
    }

    #[test]
    fn homebrew_prefix_matches_opt_homebrew_cellar_path() {
        assert_eq!(
            homebrew_prefix_for_binary(Path::new("/opt/homebrew/Cellar/clipmem/1.2.3/bin/clipmem")),
            Some(PathBuf::from("/opt/homebrew"))
        );
    }

    #[test]
    fn homebrew_prefix_matches_usr_local_cellar_path() {
        assert_eq!(
            homebrew_prefix_for_binary(Path::new("/usr/local/Cellar/clipmem/1.2.3/bin/clipmem")),
            Some(PathBuf::from("/usr/local"))
        );
    }

    #[test]
    fn homebrew_prefix_rejects_non_homebrew_paths() {
        assert_eq!(
            homebrew_prefix_for_binary(Path::new("/Users/tristan/.cargo/bin/clipmem")),
            None
        );
    }
}
