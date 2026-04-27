use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(in crate::cli) struct OpenClawDoctorReport {
    pub(in crate::cli) target_dir: PathBuf,
    pub(in crate::cli) checks: Vec<AgentDoctorCheck>,
}

#[derive(Debug, Clone)]
pub(in crate::cli) struct AgentDoctorCheck {
    pub(in crate::cli) status: AgentDoctorStatus,
    pub(in crate::cli) label: String,
    pub(in crate::cli) detail: String,
    pub(in crate::cli) next_steps: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::cli) enum AgentDoctorStatus {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone)]
pub(in crate::cli) struct HermesDoctorReport {
    pub(in crate::cli) target_dir: PathBuf,
    pub(in crate::cli) checks: Vec<AgentDoctorCheck>,
}

pub(in crate::cli) fn render_agent_doctor_report(
    target_label: &str,
    target_dir: &Path,
    checks: &[AgentDoctorCheck],
) -> String {
    let mut out = String::new();
    let _ = std::fmt::Write::write_fmt(
        &mut out,
        format_args!("{target_label}: {}\n\n", target_dir.display()),
    );

    for check in checks {
        let status = doctor_status_label(check.status);
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

fn doctor_status_label(status: AgentDoctorStatus) -> &'static str {
    match status {
        AgentDoctorStatus::Ok => "OK",
        AgentDoctorStatus::Warn => "WARN",
        AgentDoctorStatus::Fail => "FAIL",
    }
}

#[cfg(test)]
mod tests {
    use super::{render_agent_doctor_report, AgentDoctorCheck, AgentDoctorStatus};

    #[test]
    fn render_agent_doctor_report_lists_statuses_details_and_next_steps() {
        let checks = vec![
            AgentDoctorCheck {
                label: "Binary available".to_string(),
                status: AgentDoctorStatus::Ok,
                detail: "clipmem is on PATH".to_string(),
                next_steps: Vec::new(),
            },
            AgentDoctorCheck {
                label: "Skill installed".to_string(),
                status: AgentDoctorStatus::Fail,
                detail: "SKILL.md missing".to_string(),
                next_steps: vec!["Run clipmem agents openclaw install-skill".to_string()],
            },
        ];

        let report = render_agent_doctor_report(
            "OpenClaw skill",
            std::path::Path::new("/tmp/skill"),
            &checks,
        );

        assert!(report.contains("OpenClaw skill: /tmp/skill"));
        assert!(report.contains("[OK] Binary available"));
        assert!(report.contains("clipmem is on PATH"));
        assert!(report.contains("[FAIL] Skill installed"));
        assert!(report.contains("  - Run clipmem agents openclaw install-skill"));
    }
}
