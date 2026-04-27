use std::path::Path;

use crate::cli::commands::types::{OpenClawDoctorCheck, OpenClawDoctorStatus};

pub(in crate::cli) fn render_agent_doctor_report(
    target_label: &str,
    target_dir: &Path,
    checks: &[OpenClawDoctorCheck],
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

fn doctor_status_label(status: OpenClawDoctorStatus) -> &'static str {
    match status {
        OpenClawDoctorStatus::Ok => "OK",
        OpenClawDoctorStatus::Warn => "WARN",
        OpenClawDoctorStatus::Fail => "FAIL",
    }
}
