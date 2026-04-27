use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use anyhow::Result;

use crate::cli::commands::agents::support::{find_executable, home_dir};
use crate::cli::commands::types::{
    PackagedSkillFile, HERMES_CHECK_SETUP_SH, HERMES_COMMANDS_REF, HERMES_EXAMPLES_REF,
    HERMES_JSON_SCHEMA_REF, HERMES_SETUP_CHECK_REF, HERMES_SKILL_MD, HERMES_SKILL_NAME,
    HERMES_SKILL_ROOT, HERMES_TROUBLESHOOTING_REF, OPENCLAW_CHECK_SETUP_SH, OPENCLAW_COMMANDS_REF,
    OPENCLAW_EXAMPLES_REF, OPENCLAW_JSON_SCHEMA_REF, OPENCLAW_SETUP_CHECK_REF,
    OPENCLAW_SHARED_ROOT, OPENCLAW_SKILL_MD, OPENCLAW_SKILL_NAME, OPENCLAW_TROUBLESHOOTING_REF,
    OPENCLAW_WORKSPACE_ROOT,
};

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

pub(in crate::cli) fn resolve_hermes_skill_dir(dest: Option<&Path>) -> Result<PathBuf> {
    if let Some(dest) = dest {
        return Ok(dest.to_path_buf());
    }

    Ok(home_dir()?.join(HERMES_SKILL_ROOT).join(HERMES_SKILL_NAME))
}
