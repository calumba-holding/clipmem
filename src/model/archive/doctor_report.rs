use serde::Serialize;

use super::DoctorReport;

#[derive(Debug, Clone, Serialize)]
pub struct DoctorIntegrityReport {
    orphan_representation_count: usize,
    foreign_key_violation_count: usize,
    projection_mismatch_count: usize,
}

impl DoctorIntegrityReport {
    pub(crate) fn new(orphans: usize, foreign_keys: usize, projections: usize) -> Self {
        Self {
            orphan_representation_count: orphans,
            foreign_key_violation_count: foreign_keys,
            projection_mismatch_count: projections,
        }
    }
}

impl DoctorReport {
    #[must_use]
    pub(crate) fn new(
        db_path: String,
        sqlite_version: String,
        journal_mode: String,
        fts5_compile_option_present: bool,
        fts5_create_virtual_table_ok: bool,
        compile_options: Vec<String>,
        integrity: DoctorIntegrityReport,
    ) -> Self {
        Self {
            db_path,
            sqlite_version,
            journal_mode,
            fts5_compile_option_present,
            fts5_create_virtual_table_ok,
            compile_options,
            integrity,
        }
    }

    #[must_use]
    pub fn db_path(&self) -> &str {
        &self.db_path
    }

    #[must_use]
    pub fn sqlite_version(&self) -> &str {
        &self.sqlite_version
    }

    #[must_use]
    pub fn journal_mode(&self) -> &str {
        &self.journal_mode
    }

    #[must_use]
    pub fn fts5_compile_option_present(&self) -> bool {
        self.fts5_compile_option_present
    }

    #[must_use]
    pub fn fts5_create_virtual_table_ok(&self) -> bool {
        self.fts5_create_virtual_table_ok
    }

    #[must_use]
    pub fn compile_options(&self) -> &[String] {
        &self.compile_options
    }

    #[must_use]
    pub fn orphan_representation_count(&self) -> usize {
        self.integrity.orphan_representation_count
    }

    #[must_use]
    pub fn foreign_key_violation_count(&self) -> usize {
        self.integrity.foreign_key_violation_count
    }

    #[must_use]
    pub fn projection_mismatch_count(&self) -> usize {
        self.integrity.projection_mismatch_count
    }
}
