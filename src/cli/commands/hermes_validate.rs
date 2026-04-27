use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use anyhow::{anyhow, Context, Result};

use crate::cli::commands::agent_doctor::render_agent_doctor_report;
use crate::cli::commands::agent_package::{packaged_hermes_files, resolve_hermes_skill_dir};
use crate::cli::commands::agent_support::{
    binary_check, find_executable, referenced_markdown_files, validate_packaged_skill_file,
};
use crate::cli::commands::types::{
    HermesDoctorReport, OpenClawDoctorCheck, OpenClawDoctorStatus, HERMES_SKILL_NAME,
};
use crate::cli::schema::HermesDoctorArgs;

#[cfg(test)]
use crate::cli::commands::agent_package::packaged_hermes_skill;

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

pub(in crate::cli) fn build_hermes_doctor_report(
    args: &HermesDoctorArgs,
) -> Result<HermesDoctorReport> {
    let target_dir = resolve_hermes_skill_dir(args.dest.as_deref())?;
    let mut checks = Vec::new();

    let clipmem_path = find_executable("clipmem");
    checks.push(binary_check(
        "Host clipmem on PATH",
        clipmem_path.as_ref(),
        &[
            "Install clipmem with `brew install tristanmanchester/tap/clipmem`.",
            "Or install it with `cargo install clipmem`.",
            "Then run `clipmem setup` to initialize the database and start background capture.",
        ],
    ));

    let hermes_path = find_executable("hermes");
    checks.push(hermes_binary_check(hermes_path.as_ref()));

    if target_dir.exists() {
        checks.push(OpenClawDoctorCheck {
            status: OpenClawDoctorStatus::Ok,
            label: "Installed skill directory".to_string(),
            detail: format!("Found {}", target_dir.display()),
            next_steps: Vec::new(),
        });
    } else {
        checks.push(OpenClawDoctorCheck {
            status: OpenClawDoctorStatus::Fail,
            label: "Installed skill directory".to_string(),
            detail: format!("Missing {}", target_dir.display()),
            next_steps: vec![
                "Install the skill with `clipmem agents hermes install-skill`.".to_string(),
            ],
        });
    }

    let skill_path = target_dir.join("SKILL.md");
    if skill_path.is_file() {
        match validate_hermes_skill_dir(&target_dir) {
            Ok(()) => checks.push(OpenClawDoctorCheck {
                status: OpenClawDoctorStatus::Ok,
                label: "SKILL.md metadata".to_string(),
                detail: format!(
                    "Validated {} and referenced package files under {}",
                    skill_path.display(),
                    target_dir.display()
                ),
                next_steps: Vec::new(),
            }),
            Err(error) => checks.push(OpenClawDoctorCheck {
                status: OpenClawDoctorStatus::Fail,
                label: "SKILL.md metadata".to_string(),
                detail: error.to_string(),
                next_steps: vec![
                    "Reinstall the packaged skill with `clipmem agents hermes install-skill --force`.".to_string(),
                    "Use `clipmem agents hermes print-skill` to inspect the packaged content.".to_string(),
                ],
            }),
        }
    } else {
        checks.push(OpenClawDoctorCheck {
            status: OpenClawDoctorStatus::Fail,
            label: "SKILL.md file".to_string(),
            detail: format!("Missing {}", skill_path.display()),
            next_steps: vec![
                "Install the packaged skill with `clipmem agents hermes install-skill`."
                    .to_string(),
            ],
        });
    }

    checks.push(hermes_skill_discovery_check(hermes_path.as_ref()));

    Ok(HermesDoctorReport { target_dir, checks })
}

pub(in crate::cli) fn hermes_binary_check(path: Option<&PathBuf>) -> OpenClawDoctorCheck {
    match path {
        Some(path) => OpenClawDoctorCheck {
            status: OpenClawDoctorStatus::Ok,
            label: "Host hermes on PATH".to_string(),
            detail: format!("Found {}", path.display()),
            next_steps: Vec::new(),
        },
        None => OpenClawDoctorCheck {
            status: OpenClawDoctorStatus::Warn,
            label: "Host hermes on PATH".to_string(),
            detail: "hermes is not available; skipping live Hermes skill discovery checks."
                .to_string(),
            next_steps: vec![
                "Install Hermes Agent if you want to verify live skill discovery.".to_string(),
            ],
        },
    }
}

pub(in crate::cli) fn hermes_skill_discovery_check(
    hermes_path: Option<&PathBuf>,
) -> OpenClawDoctorCheck {
    let Some(hermes_path) = hermes_path else {
        return OpenClawDoctorCheck {
            status: OpenClawDoctorStatus::Warn,
            label: "Hermes skill discovery".to_string(),
            detail: "Skipped live discovery because `hermes` is not available.".to_string(),
            next_steps: vec![
                "Install Hermes Agent, then run `clipmem agents hermes doctor` again.".to_string(),
            ],
        };
    };

    let output = ProcessCommand::new(hermes_path)
        .args(["skills", "list"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            if stdout.contains(HERMES_SKILL_NAME) {
                OpenClawDoctorCheck {
                    status: OpenClawDoctorStatus::Ok,
                    label: "Hermes skill discovery".to_string(),
                    detail: format!("`hermes skills list` includes {HERMES_SKILL_NAME}."),
                    next_steps: Vec::new(),
                }
            } else {
                OpenClawDoctorCheck {
                    status: OpenClawDoctorStatus::Warn,
                    label: "Hermes skill discovery".to_string(),
                    detail: format!(
                        "`hermes skills list` did not show {HERMES_SKILL_NAME}."
                    ),
                    next_steps: vec![
                        "Restart Hermes or open a fresh session so it rescans skills.".to_string(),
                        "If you installed to a custom path, add that directory under `skills.external_dirs` in ~/.hermes/config.yaml.".to_string(),
                    ],
                }
            }
        }
        Ok(output) => OpenClawDoctorCheck {
            status: OpenClawDoctorStatus::Warn,
            label: "Hermes skill discovery".to_string(),
            detail: format!(
                "Could not list Hermes skills (`hermes skills list` exited with {}).",
                output.status
            ),
            next_steps: vec![
                "Run `hermes skills list` manually and verify whether the skill appears."
                    .to_string(),
            ],
        },
        Err(error) => OpenClawDoctorCheck {
            status: OpenClawDoctorStatus::Warn,
            label: "Hermes skill discovery".to_string(),
            detail: format!("Could not list Hermes skills: {error}"),
            next_steps: vec![
                "Run `hermes skills list` manually and verify whether the skill appears."
                    .to_string(),
            ],
        },
    }
}

pub(in crate::cli) fn validate_hermes_skill_file(path: &Path) -> Result<()> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    validate_hermes_skill_content(&content)
}

pub(in crate::cli) fn validate_hermes_skill_dir(path: &Path) -> Result<()> {
    let skill_path = path.join("SKILL.md");
    validate_hermes_skill_file(&skill_path)?;

    for file in packaged_hermes_files() {
        let installed_path = path.join(file.relative_path);
        validate_packaged_skill_file(&installed_path, file.relative_path)?;
    }

    Ok(())
}

pub(in crate::cli) fn validate_hermes_skill_content(content: &str) -> Result<()> {
    let frontmatter_lines = parse_frontmatter_lines(content)?;

    let name = frontmatter_value(&frontmatter_lines, "name").filter(|value| !value.is_empty());
    if name != Some(HERMES_SKILL_NAME) {
        return Err(anyhow!(
            "skill frontmatter must include `name: {HERMES_SKILL_NAME}`"
        ));
    }

    let description =
        frontmatter_value(&frontmatter_lines, "description").filter(|value| !value.is_empty());
    if description.is_none() {
        return Err(anyhow!(
            "skill frontmatter must include a non-empty `description`"
        ));
    }

    let version =
        frontmatter_value(&frontmatter_lines, "version").filter(|value| !value.is_empty());
    if version.is_none() {
        return Err(anyhow!("skill frontmatter must include `version`"));
    }

    let platforms = frontmatter_value(&frontmatter_lines, "platforms")
        .ok_or_else(|| anyhow!("skill frontmatter must include `platforms`"))?;
    if !platforms.contains("macos") {
        return Err(anyhow!(
            "skill frontmatter `platforms` must include `macos`"
        ));
    }

    if !frontmatter_lines
        .iter()
        .any(|line| line.trim() == "metadata:")
    {
        return Err(anyhow!("skill frontmatter must include `metadata`"));
    }
    if !frontmatter_lines
        .iter()
        .any(|line| line.trim() == "hermes:")
    {
        return Err(anyhow!("metadata must include `hermes`"));
    }
    let category = frontmatter_lines
        .iter()
        .find_map(|line| line.trim().strip_prefix("category:").map(str::trim));
    if category != Some("productivity") {
        return Err(anyhow!("metadata.hermes.category must be `productivity`"));
    }

    let tags = frontmatter_lines
        .iter()
        .find_map(|line| line.trim().strip_prefix("tags:").map(str::trim))
        .ok_or_else(|| anyhow!("metadata.hermes.tags must be present"))?;
    for tag in [
        "clipboard",
        "memory",
        "macos",
        "local-first",
        "cli",
        "retrieval",
    ] {
        if !tags.contains(tag) {
            return Err(anyhow!("metadata.hermes.tags must include `{tag}`"));
        }
    }

    let references = referenced_markdown_files(content);
    if !references
        .iter()
        .any(|reference| reference == Path::new("references/commands.md"))
    {
        return Err(anyhow!(
            "skill content must reference `references/commands.md`"
        ));
    }
    if !references
        .iter()
        .any(|reference| reference == Path::new("references/troubleshooting.md"))
    {
        return Err(anyhow!(
            "skill content must reference `references/troubleshooting.md`"
        ));
    }

    Ok(())
}

pub(in crate::cli) fn parse_frontmatter_lines(content: &str) -> Result<Vec<&str>> {
    let mut lines = content.lines();
    if lines.next() != Some("---") {
        return Err(anyhow!("skill frontmatter must start with `---`"));
    }

    let mut frontmatter_lines = Vec::new();
    for line in lines {
        if line == "---" {
            return Ok(frontmatter_lines);
        }
        frontmatter_lines.push(line);
    }

    Err(anyhow!("skill frontmatter is missing the closing `---`"))
}

pub(in crate::cli) fn frontmatter_value<'a>(lines: &'a [&str], key: &str) -> Option<&'a str> {
    let prefix = format!("{key}:");
    lines
        .iter()
        .find_map(|line| line.strip_prefix(&prefix).map(str::trim))
}

pub(in crate::cli) fn render_hermes_doctor_report(report: &HermesDoctorReport) -> String {
    render_agent_doctor_report(
        "Hermes Agent integration target",
        &report.target_dir,
        &report.checks,
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        packaged_hermes_files, packaged_hermes_skill, referenced_markdown_files,
        validate_hermes_skill_content,
    };

    #[test]
    fn packaged_hermes_skill_includes_required_metadata() {
        let content = packaged_hermes_skill();

        validate_hermes_skill_content(&content).expect("packaged Hermes skill should validate");
        assert!(content.contains("platforms: [macos]"));
        assert!(content.contains("metadata:"));
        assert!(content.contains("hermes:"));
        assert!(content.contains("category: productivity"));
        assert!(content.contains("license:"));
        assert!(content.contains("clipmem recall"));
        assert!(content.contains("clipmem timeline"));
        assert!(content.contains("clipmem export"));
        assert!(content.contains("references/commands.md"));
        assert!(content.contains("references/json-schema.md"));
        assert!(content.contains("references/examples.md"));
        assert!(content.contains("references/setup-check.md"));
        assert!(content.contains("references/troubleshooting.md"));
        assert!(content.contains("scripts/check-setup.sh"));
        assert!(content.contains("what was that command I copied?"));
        assert!(content.contains("toon"));
        assert!(content.contains("--cursor"));
        assert!(content.contains("schema_version"));
    }

    #[test]
    fn packaged_hermes_package_embeds_reference_files() {
        let files = packaged_hermes_files();
        assert_eq!(files.len(), 7);
        assert!(files.iter().any(|file| file.relative_path == "SKILL.md"));
        assert!(files
            .iter()
            .any(|file| file.relative_path == "references/commands.md"));
        assert!(files
            .iter()
            .any(|file| file.relative_path == "references/troubleshooting.md"));
        assert!(files
            .iter()
            .any(|file| file.relative_path == "references/json-schema.md"));
        assert!(files
            .iter()
            .any(|file| file.relative_path == "references/examples.md"));
        assert!(files
            .iter()
            .any(|file| file.relative_path == "references/setup-check.md"));
        assert!(files
            .iter()
            .any(|file| file.relative_path == "scripts/check-setup.sh"));
    }

    #[test]
    fn packaged_hermes_skill_references_relative_markdown_files() {
        let references = referenced_markdown_files(&packaged_hermes_skill());
        assert!(references
            .iter()
            .any(|path| path == &PathBuf::from("references/commands.md")));
        assert!(references
            .iter()
            .any(|path| path == &PathBuf::from("references/troubleshooting.md")));
        assert!(references
            .iter()
            .any(|path| path == &PathBuf::from("references/json-schema.md")));
        assert!(references
            .iter()
            .any(|path| path == &PathBuf::from("references/examples.md")));
        assert!(references
            .iter()
            .any(|path| path == &PathBuf::from("references/setup-check.md")));
    }

    #[test]
    fn hermes_skill_validation_rejects_missing_required_frontmatter() {
        let valid = packaged_hermes_skill();

        let missing_name = valid.replacen("name: clipboard-memory\n", "", 1);
        assert!(validate_hermes_skill_content(&missing_name)
            .unwrap_err()
            .to_string()
            .contains("name: clipboard-memory"));

        let wrong_name = valid.replacen("name: clipboard-memory", "name: other", 1);
        assert!(validate_hermes_skill_content(&wrong_name)
            .unwrap_err()
            .to_string()
            .contains("name: clipboard-memory"));

        let missing_description = valid.replacen("description:", "summary:", 1);
        assert!(validate_hermes_skill_content(&missing_description)
            .unwrap_err()
            .to_string()
            .contains("description"));

        let missing_platforms = valid.replacen("platforms: [macos]\n", "", 1);
        assert!(validate_hermes_skill_content(&missing_platforms)
            .unwrap_err()
            .to_string()
            .contains("platforms"));

        let missing_metadata = valid.replacen("  hermes:", "  runtime:", 1);
        assert!(validate_hermes_skill_content(&missing_metadata)
            .unwrap_err()
            .to_string()
            .contains("metadata must include `hermes`"));

        let missing_references = valid.replace("references/commands.md", "references/missing.md");
        assert!(validate_hermes_skill_content(&missing_references)
            .unwrap_err()
            .to_string()
            .contains("references/commands.md"));
    }
}
