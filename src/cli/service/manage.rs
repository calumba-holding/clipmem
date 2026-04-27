use std::env;
use std::fs;
use std::path::Path;
use std::process::Command as ProcessCommand;

use anyhow::{anyhow, bail, Context, Result};

use crate::cli::db_path::default_db_path;
use crate::db::{CaptureStoreOutcome, Database};
use crate::platform::capture_snapshot;

use super::launchctl::{
    brew_services_available, ensure_parent_dir, find_executable, home_dir,
    homebrew_prefix_for_binary, launchctl_bootout, launchctl_bootstrap, launchctl_disable,
    launchctl_enable, launchctl_kickstart, run_command_checked, touch_log_file,
    trusted_binary_path, write_direct_plist,
};
use super::model::*;
use super::status::status_report;

pub(crate) fn setup(db_path: &Path) -> Result<SetupReport> {
    ensure_supported()?;
    let context = build_context(db_path)?;
    let status = status_report(db_path)?;
    let selection = select_provider(&context);
    ensure_no_conflict(&status)?;
    let seed_capture = seed_capture(db_path)?;
    let action = start_with_provider(&context, &selection)?;
    Ok(SetupReport {
        seed_capture,
        action,
    })
}

pub(crate) fn start(db_path: &Path) -> Result<ServiceActionReport> {
    ensure_supported()?;
    let context = build_context(db_path)?;
    let status = status_report(db_path)?;
    let selection = select_provider(&context);
    ensure_no_conflict(&status)?;
    start_with_provider(&context, &selection)
}

pub(crate) fn stop(db_path: &Path) -> Result<ServiceActionReport> {
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

pub(crate) fn uninstall(db_path: &Path) -> Result<ServiceActionReport> {
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

pub(crate) fn ensure_supported() -> Result<()> {
    if cfg!(target_os = "macos") {
        Ok(())
    } else {
        bail!("clipmem setup and service commands are only supported on macOS");
    }
}

pub(crate) fn build_context(db_path: &Path) -> Result<ServiceContext> {
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

pub(in crate::cli) fn select_provider(context: &ServiceContext) -> ProviderSelection {
    let mut notes = Vec::new();
    if let Some(brew_path) = &context.brew_path {
        if context.homebrew_prefix.is_some() {
            if brew_services_available(brew_path) && brew_services_can_manage_clipmem(brew_path) {
                return ProviderSelection {
                    provider: ServiceProvider::Homebrew,
                    reason:
                        "current binary is the Homebrew formula and `brew services` is available"
                            .to_string(),
                    notes,
                };
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

    ProviderSelection {
        provider: ServiceProvider::Launchagent,
        reason: "using the direct per-user LaunchAgent managed by clipmem".to_string(),
        notes,
    }
}

pub(in crate::cli) fn ensure_no_conflict(status: &ServiceStatusReport) -> Result<()> {
    if status.conflict {
        bail!(conflict_message());
    }
    Ok(())
}

pub(in crate::cli) fn conflict_message() -> String {
    "Both the Homebrew service and the direct LaunchAgent are installed. Remove one first with `brew services stop clipmem` or `clipmem service uninstall`.".to_string()
}

pub(in crate::cli) fn start_with_provider(
    context: &ServiceContext,
    selection: &ProviderSelection,
) -> Result<ServiceActionReport> {
    match selection.provider {
        ServiceProvider::Homebrew => start_homebrew_provider(context, &selection.notes),
        ServiceProvider::Launchagent => start_direct_provider(context, &selection.notes),
    }
}

pub(in crate::cli) fn start_homebrew_provider(
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

pub(in crate::cli) fn brew_services_formula_unavailable(stderr: &str) -> bool {
    stderr.contains("has not implemented #plist")
        || stderr.contains("has not implemented #service")
        || stderr.contains("provided a locatable service file")
}

pub(in crate::cli) fn brew_services_can_manage_clipmem(brew: &Path) -> bool {
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

pub(in crate::cli) fn stop_homebrew_provider(
    context: &ServiceContext,
) -> Result<ServiceActionReport> {
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

pub(in crate::cli) fn uninstall_homebrew_provider(
    context: &ServiceContext,
) -> Result<ServiceActionReport> {
    let mut report = stop_homebrew_provider(context)?;
    report.action = "uninstall";
    report.notes.push(
        "Only the Homebrew-managed service was removed; the formula stays installed.".to_string(),
    );
    Ok(report)
}

pub(in crate::cli) fn start_direct_provider(
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

pub(in crate::cli) fn stop_direct_provider(
    context: &ServiceContext,
) -> Result<ServiceActionReport> {
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

pub(in crate::cli) fn uninstall_direct_provider(
    context: &ServiceContext,
) -> Result<ServiceActionReport> {
    launchctl_bootout(DIRECT_LABEL)?;
    launchctl_disable(DIRECT_LABEL)?;
    let mut notes = Vec::new();
    match fs::remove_file(&context.direct_plist_path) {
        Ok(()) => notes.push("Removed the direct LaunchAgent plist.".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            notes.push("The direct LaunchAgent plist was already absent.".to_string());
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "remove direct LaunchAgent plist {}",
                    context.direct_plist_path.display()
                )
            });
        }
    }
    Ok(ServiceActionReport {
        action: "uninstall",
        provider: ServiceProvider::Launchagent,
        binary_path: context.binary_path.clone(),
        db_path: context.db_path.clone(),
        label: DIRECT_LABEL,
        notes,
    })
}

pub(in crate::cli) fn seed_capture(db_path: &Path) -> Result<SeedCaptureOutcome> {
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
