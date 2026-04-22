use super::*;

pub(in crate::cli) fn build_openclaw_doctor_report(
    args: &OpenClawDoctorArgs,
) -> Result<OpenClawDoctorReport> {
    let target_dir = resolve_openclaw_skill_dir(args.dest.as_deref(), args.shared)?;
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

    let openclaw_path = find_executable("openclaw");
    checks.push(binary_check(
        "Host openclaw on PATH",
        openclaw_path.as_ref(),
        &["Install OpenClaw and ensure `openclaw` is available on the host PATH."],
    ));

    let workspace_root = resolve_openclaw_workspace_root()?;
    checks.push(OpenClawDoctorCheck {
        status: OpenClawDoctorStatus::Ok,
        label: "OpenClaw workspace root".to_string(),
        detail: format!("Resolved workspace root: {}", workspace_root.display()),
        next_steps: Vec::new(),
    });

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
                "Install the skill with `clipmem agents openclaw install-skill`.".to_string(),
                "Use `--shared` if you intended a shared install under ~/.openclaw/skills."
                    .to_string(),
            ],
        });
    }

    let skill_path = target_dir.join("SKILL.md");
    if skill_path.is_file() {
        match validate_openclaw_skill_dir(&target_dir) {
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
                    "Reinstall the packaged skill with `clipmem agents openclaw install-skill --force`.".to_string(),
                    "Use `clipmem agents openclaw print-skill` to inspect the packaged content.".to_string(),
                ],
            }),
        }
    } else {
        checks.push(OpenClawDoctorCheck {
            status: OpenClawDoctorStatus::Fail,
            label: "SKILL.md file".to_string(),
            detail: format!("Missing {}", skill_path.display()),
            next_steps: vec![
                "Install the packaged skill with `clipmem agents openclaw install-skill`."
                    .to_string(),
            ],
        });
    }

    checks.push(openclaw_sandbox_check(openclaw_path.as_ref()));

    Ok(OpenClawDoctorReport { target_dir, checks })
}

pub(in crate::cli) fn binary_check(
    label: &str,
    path: Option<&PathBuf>,
    next_steps: &[&str],
) -> OpenClawDoctorCheck {
    match path {
        Some(path) => OpenClawDoctorCheck {
            status: OpenClawDoctorStatus::Ok,
            label: label.to_string(),
            detail: format!("Found {}", path.display()),
            next_steps: Vec::new(),
        },
        None => OpenClawDoctorCheck {
            status: OpenClawDoctorStatus::Fail,
            label: label.to_string(),
            detail: format!("{label} is missing"),
            next_steps: next_steps.iter().map(|s| s.to_string()).collect(),
        },
    }
}

pub(in crate::cli) fn openclaw_sandbox_check(
    openclaw_path: Option<&PathBuf>,
) -> OpenClawDoctorCheck {
    let Some(openclaw_path) = openclaw_path else {
        return OpenClawDoctorCheck {
            status: OpenClawDoctorStatus::Warn,
            label: "Sandbox visibility".to_string(),
            detail: "Skipped sandbox checks because `openclaw` is not available.".to_string(),
            next_steps: vec![
                "Install OpenClaw first if you want sandbox-specific guidance.".to_string(),
            ],
        };
    };

    let output = ProcessCommand::new(openclaw_path)
        .args(["sandbox", "explain"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let lower = stdout.to_ascii_lowercase();
            if lower.contains("disabled") || lower.contains("off") {
                OpenClawDoctorCheck {
                    status: OpenClawDoctorStatus::Ok,
                    label: "Sandbox visibility".to_string(),
                    detail: "OpenClaw sandboxing appears disabled; host PATH should be sufficient.".to_string(),
                    next_steps: Vec::new(),
                }
            } else {
                OpenClawDoctorCheck {
                    status: OpenClawDoctorStatus::Warn,
                    label: "Sandbox visibility".to_string(),
                    detail: "OpenClaw sandboxing appears active; `clipmem` may need to be available inside sandbox containers as well as on the host.".to_string(),
                    next_steps: vec![
                        "Ensure `clipmem` is installed in a path visible inside the sandbox image, not only your host shell.".to_string(),
                        "If you installed clipmem after sandbox creation, recreate containers with `openclaw sandbox recreate --all`.".to_string(),
                        "If commands still fail in the sandbox, use `openclaw sandbox explain` and verify the container PATH.".to_string(),
                    ],
                }
            }
        }
        Ok(output) => OpenClawDoctorCheck {
            status: OpenClawDoctorStatus::Warn,
            label: "Sandbox visibility".to_string(),
            detail: format!(
                "Could not inspect sandbox state (`openclaw sandbox explain` exited with {}).",
                output.status
            ),
            next_steps: vec![
                "Run `openclaw sandbox explain` manually and verify whether `clipmem` is present in the sandbox environment.".to_string(),
            ],
        },
        Err(error) => OpenClawDoctorCheck {
            status: OpenClawDoctorStatus::Warn,
            label: "Sandbox visibility".to_string(),
            detail: format!("Could not inspect sandbox state: {error}"),
            next_steps: vec![
                "Run `openclaw sandbox explain` manually and verify whether `clipmem` is present in the sandbox environment.".to_string(),
            ],
        },
    }
}

pub(in crate::cli) fn validate_openclaw_skill_file(path: &Path) -> Result<()> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    validate_openclaw_skill_content(&content)
}

pub(in crate::cli) fn validate_openclaw_skill_dir(path: &Path) -> Result<()> {
    let skill_path = path.join("SKILL.md");
    validate_openclaw_skill_file(&skill_path)?;

    for file in packaged_openclaw_files() {
        let installed_path = path.join(file.relative_path);
        validate_packaged_openclaw_file(&installed_path, file.relative_path)?;
    }

    Ok(())
}

pub(in crate::cli) fn validate_packaged_openclaw_file(
    path: &Path,
    relative_path: &str,
) -> Result<()> {
    if !path.is_file() {
        return Err(anyhow!("packaged file is missing: {}", path.display()));
    }

    if relative_path.ends_with(".sh") {
        ensure_executable(path)
            .with_context(|| format!("packaged script is not executable: {}", path.display()))?;
    }

    Ok(())
}

#[cfg(unix)]
pub(in crate::cli) fn ensure_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = std::fs::metadata(path)?.permissions().mode() & 0o777;
    if mode & 0o111 == 0 {
        return Err(anyhow!("mode {mode:o} does not include any execute bit"));
    }

    Ok(())
}

#[cfg(not(unix))]
pub(in crate::cli) fn ensure_executable(_path: &Path) -> Result<()> {
    Ok(())
}

pub(in crate::cli) fn validate_openclaw_skill_content(content: &str) -> Result<()> {
    let mut lines = content.lines();
    if lines.next() != Some("---") {
        return Err(anyhow!("skill frontmatter must start with `---`"));
    }

    let mut frontmatter_lines = Vec::new();
    let mut found_end = false;
    for line in lines {
        if line == "---" {
            found_end = true;
            break;
        }
        frontmatter_lines.push(line);
    }
    if !found_end {
        return Err(anyhow!("skill frontmatter is missing the closing `---`"));
    }

    let name = frontmatter_lines
        .iter()
        .find_map(|line| line.strip_prefix("name:").map(str::trim))
        .filter(|value| !value.is_empty());
    if name != Some(OPENCLAW_SKILL_NAME) {
        return Err(anyhow!(
            "skill frontmatter must include `name: {OPENCLAW_SKILL_NAME}`"
        ));
    }

    let description = frontmatter_lines
        .iter()
        .find_map(|line| line.strip_prefix("description:").map(str::trim))
        .filter(|value| !value.is_empty());
    if description.is_none() {
        return Err(anyhow!(
            "skill frontmatter must include a non-empty `description`"
        ));
    }

    let metadata_line = frontmatter_lines
        .iter()
        .find_map(|line| line.strip_prefix("metadata:").map(str::trim))
        .ok_or_else(|| anyhow!("skill frontmatter must include `metadata`"))?;
    let metadata: serde_json::Value = serde_json::from_str(metadata_line)
        .map_err(|error| anyhow!("invalid metadata JSON: {error}"))?;
    let openclaw = metadata
        .get("openclaw")
        .ok_or_else(|| anyhow!("metadata must include `openclaw`"))?;
    let bins = openclaw
        .pointer("/requires/bins")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow!("metadata.openclaw.requires.bins must be an array"))?;
    if !bins.iter().any(|value| value.as_str() == Some("clipmem")) {
        return Err(anyhow!(
            "metadata.openclaw.requires.bins must contain `clipmem`"
        ));
    }

    let install = openclaw
        .get("install")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow!("metadata.openclaw.install must be an array"))?;
    validate_openclaw_install_entries(install)?;

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

pub(in crate::cli) fn referenced_markdown_files(content: &str) -> Vec<PathBuf> {
    let mut references = Vec::new();

    for line in content.lines() {
        let mut remainder = line;
        while let Some(start) = remainder.find("(references/") {
            let after_start = &remainder[start + 1..];
            let Some(end) = after_start.find(')') else {
                break;
            };
            let candidate = &after_start[..end];
            if candidate.ends_with(".md") {
                let path = PathBuf::from(candidate);
                if !references.iter().any(|existing| existing == &path) {
                    references.push(path);
                }
            }
            remainder = &after_start[end + 1..];
        }
    }

    references
}

pub(in crate::cli) fn validate_openclaw_install_entries(
    entries: &[serde_json::Value],
) -> Result<()> {
    if entries.is_empty() {
        return Err(anyhow!(
            "metadata.openclaw.install must contain at least one entry"
        ));
    }

    for entry in entries {
        let object = entry
            .as_object()
            .ok_or_else(|| anyhow!("metadata.openclaw.install entries must be objects"))?;
        for key in ["id", "kind", "label", "bins"] {
            if !object.contains_key(key) {
                return Err(anyhow!(
                    "metadata.openclaw.install entry is missing `{key}`"
                ));
            }
        }
        let bins = object
            .get("bins")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| anyhow!("metadata.openclaw.install entry `bins` must be an array"))?;
        if bins.is_empty() || !bins.iter().any(|value| value.as_str() == Some("clipmem")) {
            return Err(anyhow!(
                "metadata.openclaw.install entry `bins` must contain `clipmem`"
            ));
        }
    }

    Ok(())
}

pub(in crate::cli) fn render_openclaw_doctor_report(report: &OpenClawDoctorReport) -> String {
    let mut out = String::new();
    let _ = std::fmt::Write::write_fmt(
        &mut out,
        format_args!(
            "OpenClaw integration target: {}\n\n",
            report.target_dir.display()
        ),
    );

    for check in &report.checks {
        let status = match check.status {
            OpenClawDoctorStatus::Ok => "OK",
            OpenClawDoctorStatus::Warn => "WARN",
            OpenClawDoctorStatus::Fail => "FAIL",
        };
        let _ = std::fmt::Write::write_fmt(
            &mut out,
            format_args!("[{status}] {}\n{}\n", check.label, check.detail),
        );
        for step in &check.next_steps {
            let _ = std::fmt::Write::write_fmt(&mut out, format_args!("  - {step}\n"));
        }
        out.push('\n');
    }

    out
}
