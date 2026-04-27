use std::process::ExitCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliExitCode {
    Ok = 0,
    Internal = 1,
    InvalidArgs = 2,
    NotFound = 3,
    UnsupportedFormat = 4,
    DbError = 5,
    PlatformError = 6,
}

impl CliExitCode {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub fn as_exit_code(self) -> ExitCode {
        ExitCode::from(self.as_u8())
    }
}

#[derive(Debug)]
pub struct CliError {
    exit_code: CliExitCode,
    message: String,
}

impl CliError {
    #[must_use]
    pub const fn exit_code(&self) -> CliExitCode {
        self.exit_code
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    pub(super) fn new(exit_code: CliExitCode, message: impl Into<String>) -> Self {
        Self {
            exit_code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}
