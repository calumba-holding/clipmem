use std::path::Path;

use anyhow::{Context, Result};

use crate::cli::human::render_doctor_human;
use crate::cli::output::{emit_json_or_text, render_doctor_text};
use crate::cli::schema::DoctorArgs;

use super::runtime::open_existing_db;

pub(in crate::cli) fn doctor(db_path: &Path, args: &DoctorArgs) -> Result<()> {
    let db = open_existing_db(db_path)?;
    let report = db.doctor().context("doctor diagnostics failed")?;
    if args.human {
        print!("{}", render_doctor_human(&report));
    } else {
        emit_json_or_text(args.json, &report, render_doctor_text)?;
    }

    Ok(())
}
