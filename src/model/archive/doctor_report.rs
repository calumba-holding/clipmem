use super::DoctorReport;

impl DoctorReport {
    #[must_use]
    pub(crate) fn new(
        db_path: String,
        sqlite_version: String,
        journal_mode: String,
        fts5_compile_option_present: bool,
        fts5_create_virtual_table_ok: bool,
        compile_options: Vec<String>,
    ) -> Self {
        Self {
            db_path,
            sqlite_version,
            journal_mode,
            fts5_compile_option_present,
            fts5_create_virtual_table_ok,
            compile_options,
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
}
