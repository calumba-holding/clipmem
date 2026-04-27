use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use anyhow::{anyhow, bail, Context, Result};

use super::model::{ServiceContext, DEFAULT_INTERVAL_MS, DIRECT_PLIST_TEMPLATE};

pub(in crate::cli) fn write_direct_plist(context: &ServiceContext) -> Result<()> {
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

pub(in crate::cli) fn run_command_checked(
    output: std::io::Result<std::process::Output>,
    command: &str,
) -> Result<()> {
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

pub(in crate::cli) fn launchctl_bootout(label: &str) -> Result<()> {
    let target = format!("gui/{}/{}", uid()?, label);
    run_launchctl_idempotent(["bootout", target.as_str()], "launchctl bootout")?;
    Ok(())
}

pub(in crate::cli) fn launchctl_bootstrap(plist_path: &Path) -> Result<()> {
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

pub(in crate::cli) fn launchctl_enable(label: &str) -> Result<()> {
    run_command_checked(
        ProcessCommand::new("launchctl")
            .args(["enable", &format!("gui/{}/{}", uid()?, label)])
            .output(),
        "launchctl enable",
    )
}

pub(in crate::cli) fn launchctl_disable(label: &str) -> Result<()> {
    let target = format!("gui/{}/{}", uid()?, label);
    run_launchctl_idempotent(["disable", target.as_str()], "launchctl disable")?;
    Ok(())
}

pub(in crate::cli) fn launchctl_kickstart(label: &str) -> Result<()> {
    run_command_checked(
        ProcessCommand::new("launchctl")
            .args(["kickstart", "-k", &format!("gui/{}/{}", uid()?, label)])
            .output(),
        "launchctl kickstart",
    )
}

fn run_launchctl_idempotent<const N: usize>(args: [&str; N], command: &str) -> Result<()> {
    let output = ProcessCommand::new("launchctl")
        .args(args)
        .output()
        .with_context(|| format!("run `{command}`"))?;
    if output.status.success() || launchctl_reports_absent_service(&output) {
        return Ok(());
    }

    run_command_checked(Ok(output), command)
}

fn launchctl_reports_absent_service(output: &std::process::Output) -> bool {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    [
        "could not find specified service",
        "does not exist",
        "no such file",
        "no such process",
        "not loaded",
        "service is not loaded",
        "unknown service",
    ]
    .iter()
    .any(|marker| combined.contains(marker))
}

pub(in crate::cli) fn uid() -> Result<String> {
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

pub(in crate::cli) fn ensure_parent_dir(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    set_mode_700(parent)?;
    Ok(())
}

pub(in crate::cli) fn touch_log_file(path: &Path) -> Result<()> {
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("create {}", path.display()))?;
    set_mode_600(path)?;
    Ok(())
}

#[cfg(unix)]
pub(in crate::cli) fn set_mode_700(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o700);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
pub(in crate::cli) fn set_mode_700(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
pub(in crate::cli) fn set_mode_600(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
pub(in crate::cli) fn set_mode_600(_path: &Path) -> Result<()> {
    Ok(())
}

pub(in crate::cli) fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub(in crate::cli) fn brew_services_available(brew_path: &Path) -> bool {
    ProcessCommand::new(brew_path)
        .args(["services", "list"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub(in crate::cli) fn trusted_binary_path() -> Result<PathBuf> {
    if let Some(path) = env::var_os("CLIPMEM_TEST_ACTIVE_BINARY") {
        return Ok(PathBuf::from(path));
    }

    env::current_exe().context("resolve current executable path")
}

pub(in crate::cli) fn find_executable(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path).find_map(|dir| {
        let candidate = dir.join(name);
        candidate.is_file().then_some(candidate)
    })
}

pub(in crate::cli) fn homebrew_prefix_for_binary(path: &Path) -> Option<PathBuf> {
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

pub(in crate::cli) fn home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME is not set"))
}
