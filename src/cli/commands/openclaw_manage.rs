use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use anyhow::{anyhow, Context, Result};

use crate::cli::commands::openclaw_validate::{
    build_openclaw_doctor_report, render_openclaw_doctor_report,
};
use crate::cli::commands::types::{
    OpenClawDoctorStatus, PackagedSkillFile, OPENCLAW_CHECK_SETUP_SH, OPENCLAW_COMMANDS_REF,
    OPENCLAW_EXAMPLES_REF, OPENCLAW_JSON_SCHEMA_REF, OPENCLAW_SETUP_CHECK_REF,
    OPENCLAW_SHARED_ROOT, OPENCLAW_SKILL_MD, OPENCLAW_SKILL_NAME, OPENCLAW_TROUBLESHOOTING_REF,
    OPENCLAW_WORKSPACE_ROOT,
};
use crate::cli::schema::{
    OpenClawDoctorArgs, OpenClawInstallSkillArgs, OpenClawUninstallSkillArgs,
};

pub(in crate::cli) fn openclaw_install_skill(args: &OpenClawInstallSkillArgs) -> Result<()> {
    let target_dir = resolve_openclaw_skill_dir(args.dest.as_deref(), args.shared)?;
    if target_dir.exists() {
        if args.force {
            std::fs::remove_dir_all(&target_dir).with_context(|| {
                format!(
                    "failed to remove existing skill at {}",
                    target_dir.display()
                )
            })?;
        } else {
            return Err(anyhow!(
                "skill directory already exists at {} (pass --force to replace it)",
                target_dir.display()
            ));
        }
    }

    install_openclaw_package(&target_dir)?;

    println!("Installed OpenClaw skill into {}", target_dir.display());
    println!("Skill file: {}", target_dir.join("SKILL.md").display());
    println!(
        "Reload OpenClaw skills or restart OpenClaw if the skill does not appear immediately."
    );
    Ok(())
}

pub(in crate::cli) fn openclaw_uninstall_skill(args: &OpenClawUninstallSkillArgs) -> Result<()> {
    let target_dir = resolve_openclaw_skill_dir(args.dest.as_deref(), args.shared)?;
    if !target_dir.exists() {
        println!("No installed skill found at {}", target_dir.display());
        return Ok(());
    }

    std::fs::remove_dir_all(&target_dir)
        .with_context(|| format!("failed to remove {}", target_dir.display()))?;
    println!("Removed OpenClaw skill from {}", target_dir.display());
    Ok(())
}

pub(in crate::cli) fn openclaw_doctor(args: &OpenClawDoctorArgs) -> Result<()> {
    let report = build_openclaw_doctor_report(args)?;
    print!("{}", render_openclaw_doctor_report(&report));

    if report
        .checks
        .iter()
        .any(|check| matches!(check.status, OpenClawDoctorStatus::Fail))
    {
        Err(anyhow!("OpenClaw integration checks failed"))
    } else {
        Ok(())
    }
}

pub(in crate::cli) fn packaged_openclaw_skill() -> String {
    OPENCLAW_SKILL_MD.to_string()
}

pub(in crate::cli) fn packaged_openclaw_files() -> &'static [PackagedSkillFile] {
    &[
        PackagedSkillFile {
            relative_path: "SKILL.md",
            contents: OPENCLAW_SKILL_MD,
        },
        PackagedSkillFile {
            relative_path: "references/commands.md",
            contents: OPENCLAW_COMMANDS_REF,
        },
        PackagedSkillFile {
            relative_path: "references/troubleshooting.md",
            contents: OPENCLAW_TROUBLESHOOTING_REF,
        },
        PackagedSkillFile {
            relative_path: "references/json-schema.md",
            contents: OPENCLAW_JSON_SCHEMA_REF,
        },
        PackagedSkillFile {
            relative_path: "references/examples.md",
            contents: OPENCLAW_EXAMPLES_REF,
        },
        PackagedSkillFile {
            relative_path: "references/setup-check.md",
            contents: OPENCLAW_SETUP_CHECK_REF,
        },
        PackagedSkillFile {
            relative_path: "scripts/check-setup.sh",
            contents: OPENCLAW_CHECK_SETUP_SH,
        },
    ]
}

pub(in crate::cli) fn install_openclaw_package(target_dir: &Path) -> Result<()> {
    install_packaged_skill(target_dir, packaged_openclaw_files())
}

pub(in crate::cli) fn install_packaged_skill(
    target_dir: &Path,
    files: &[PackagedSkillFile],
) -> Result<()> {
    std::fs::create_dir_all(target_dir)
        .with_context(|| format!("failed to create {}", target_dir.display()))?;

    for file in files {
        let path = target_dir.join(file.relative_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        std::fs::write(&path, file.contents)
            .with_context(|| format!("failed to write {}", path.display()))?;
        if file.relative_path.ends_with(".sh") {
            set_executable(&path)
                .with_context(|| format!("failed to mark {} executable", path.display()))?;
        }
    }

    Ok(())
}

#[cfg(unix)]
pub(in crate::cli) fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
pub(in crate::cli) fn set_executable(_path: &Path) -> Result<()> {
    // On non-Unix targets there is no concept of the executable bit to set;
    // scripts are invoked via their interpreter explicitly.
    Ok(())
}

pub(in crate::cli) fn resolve_openclaw_skill_dir(
    dest: Option<&Path>,
    shared: bool,
) -> Result<PathBuf> {
    if let Some(dest) = dest {
        return Ok(dest.to_path_buf());
    }

    let base = if shared {
        home_dir()?.join(OPENCLAW_SHARED_ROOT)
    } else {
        resolve_openclaw_workspace_root()?.join("skills")
    };
    Ok(base.join(OPENCLAW_SKILL_NAME))
}

pub(in crate::cli) fn resolve_openclaw_workspace_root() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("CLIPMEM_OPENCLAW_WORKSPACE") {
        return Ok(PathBuf::from(path));
    }

    if let Some(openclaw_bin) = find_executable("openclaw") {
        let output = ProcessCommand::new(openclaw_bin)
            .args(["config", "get", "agents.defaults.workspace"])
            .output();
        if let Ok(output) = output {
            if output.status.success() {
                let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !value.is_empty() {
                    return Ok(PathBuf::from(value));
                }
            }
        }
    }

    Ok(home_dir()?.join(OPENCLAW_WORKSPACE_ROOT))
}

pub(in crate::cli) fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME is not set"))
}

pub(in crate::cli) fn find_executable(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        let candidate = dir.join(name);
        candidate.is_file().then_some(candidate)
    })
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{install_packaged_skill, packaged_openclaw_files, resolve_openclaw_skill_dir};
    use crate::cli::commands::types::{PackagedSkillFile, OPENCLAW_SKILL_NAME};

    fn temp_skill_dir(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("clipmem-{name}-{}-{nonce}", std::process::id()))
    }

    #[test]
    fn resolve_openclaw_skill_dir_uses_explicit_destination() {
        let explicit = std::path::Path::new("/tmp/custom-openclaw-skill");

        assert_eq!(
            resolve_openclaw_skill_dir(Some(explicit), false)
                .expect("explicit path should resolve"),
            explicit
        );
    }

    #[test]
    fn packaged_openclaw_files_include_skill_and_setup_script() {
        let files = packaged_openclaw_files();

        assert!(files
            .iter()
            .any(|file| file.relative_path == "SKILL.md"
                && file.contents.contains(OPENCLAW_SKILL_NAME)));
        assert!(files
            .iter()
            .any(|file| file.relative_path == "scripts/check-setup.sh"
                && file.contents.starts_with("#!/")));
    }

    #[test]
    fn install_packaged_skill_writes_nested_files_and_marks_scripts_executable() {
        let target_dir = temp_skill_dir("packaged-skill");
        let files = [
            PackagedSkillFile {
                relative_path: "SKILL.md",
                contents: "name: test\n",
            },
            PackagedSkillFile {
                relative_path: "scripts/check-setup.sh",
                contents: "#!/bin/sh\nexit 0\n",
            },
        ];

        install_packaged_skill(&target_dir, &files).expect("package should install");

        let skill_path = target_dir.join("SKILL.md");
        let script_path = target_dir.join("scripts/check-setup.sh");
        assert_eq!(
            std::fs::read_to_string(&skill_path).expect("skill file should be readable"),
            "name: test\n"
        );
        assert!(script_path.is_file());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&script_path)
                .expect("script metadata should be readable")
                .permissions()
                .mode();
            assert_ne!(mode & 0o111, 0);
        }

        let _ = std::fs::remove_dir_all(target_dir);
    }
}
