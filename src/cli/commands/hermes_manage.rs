use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

use crate::cli::commands::agent_support::{home_dir, install_packaged_skill};
use crate::cli::commands::hermes_validate::{
    build_hermes_doctor_report, render_hermes_doctor_report,
};
use crate::cli::commands::types::{
    OpenClawDoctorStatus, PackagedSkillFile, HERMES_CHECK_SETUP_SH, HERMES_COMMANDS_REF,
    HERMES_EXAMPLES_REF, HERMES_JSON_SCHEMA_REF, HERMES_SETUP_CHECK_REF, HERMES_SKILL_MD,
    HERMES_SKILL_NAME, HERMES_SKILL_ROOT, HERMES_TROUBLESHOOTING_REF,
};
use crate::cli::schema::{HermesDoctorArgs, HermesInstallSkillArgs, HermesUninstallSkillArgs};

pub(in crate::cli) fn hermes_install_skill(args: &HermesInstallSkillArgs) -> Result<()> {
    let target_dir = resolve_hermes_skill_dir(args.dest.as_deref())?;
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

    install_hermes_package(&target_dir)?;

    println!("Installed Hermes Agent skill into {}", target_dir.display());
    println!("Skill file: {}", target_dir.join("SKILL.md").display());
    println!("Restart Hermes or open a fresh session if the skill does not appear immediately.");
    Ok(())
}

pub(in crate::cli) fn hermes_uninstall_skill(args: &HermesUninstallSkillArgs) -> Result<()> {
    let target_dir = resolve_hermes_skill_dir(args.dest.as_deref())?;
    if !target_dir.exists() {
        println!("No installed skill found at {}", target_dir.display());
        return Ok(());
    }

    std::fs::remove_dir_all(&target_dir)
        .with_context(|| format!("failed to remove {}", target_dir.display()))?;
    println!("Removed Hermes Agent skill from {}", target_dir.display());
    Ok(())
}

pub(in crate::cli) fn hermes_doctor(args: &HermesDoctorArgs) -> Result<()> {
    let report = build_hermes_doctor_report(args)?;
    print!("{}", render_hermes_doctor_report(&report));

    if report
        .checks
        .iter()
        .any(|check| matches!(check.status, OpenClawDoctorStatus::Fail))
    {
        Err(anyhow!("Hermes Agent integration checks failed"))
    } else {
        Ok(())
    }
}

pub(in crate::cli) fn packaged_hermes_skill() -> String {
    HERMES_SKILL_MD.to_string()
}

pub(in crate::cli) fn packaged_hermes_files() -> &'static [PackagedSkillFile] {
    &[
        PackagedSkillFile {
            relative_path: "SKILL.md",
            contents: HERMES_SKILL_MD,
        },
        PackagedSkillFile {
            relative_path: "references/commands.md",
            contents: HERMES_COMMANDS_REF,
        },
        PackagedSkillFile {
            relative_path: "references/troubleshooting.md",
            contents: HERMES_TROUBLESHOOTING_REF,
        },
        PackagedSkillFile {
            relative_path: "references/json-schema.md",
            contents: HERMES_JSON_SCHEMA_REF,
        },
        PackagedSkillFile {
            relative_path: "references/examples.md",
            contents: HERMES_EXAMPLES_REF,
        },
        PackagedSkillFile {
            relative_path: "references/setup-check.md",
            contents: HERMES_SETUP_CHECK_REF,
        },
        PackagedSkillFile {
            relative_path: "scripts/check-setup.sh",
            contents: HERMES_CHECK_SETUP_SH,
        },
    ]
}

pub(in crate::cli) fn install_hermes_package(target_dir: &Path) -> Result<()> {
    install_packaged_skill(target_dir, packaged_hermes_files())
}

pub(in crate::cli) fn resolve_hermes_skill_dir(dest: Option<&Path>) -> Result<PathBuf> {
    if let Some(dest) = dest {
        return Ok(dest.to_path_buf());
    }

    Ok(home_dir()?.join(HERMES_SKILL_ROOT).join(HERMES_SKILL_NAME))
}

#[cfg(test)]
mod tests {
    use super::{packaged_hermes_files, resolve_hermes_skill_dir};
    use crate::cli::commands::types::HERMES_SKILL_NAME;

    #[test]
    fn resolve_hermes_skill_dir_uses_explicit_destination() {
        let explicit = std::path::Path::new("/tmp/custom-hermes-skill");

        assert_eq!(
            resolve_hermes_skill_dir(Some(explicit)).expect("explicit path should resolve"),
            explicit
        );
    }

    #[test]
    fn packaged_hermes_files_include_skill_and_setup_script() {
        let files = packaged_hermes_files();

        assert!(files
            .iter()
            .any(|file| file.relative_path == "SKILL.md"
                && file.contents.contains(HERMES_SKILL_NAME)));
        assert!(files
            .iter()
            .any(|file| file.relative_path == "scripts/check-setup.sh"
                && file.contents.starts_with("#!/")));
    }
}
