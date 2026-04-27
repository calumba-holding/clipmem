use clap::error::ErrorKind;

use super::exit::{CliError, CliExitCode};
use super::output::UnsupportedFormatError;

#[derive(Debug)]
pub(in crate::cli) struct CliCommandError {
    exit_code: CliExitCode,
    message: String,
}

impl CliCommandError {
    fn new(exit_code: CliExitCode, message: impl Into<String>) -> Self {
        Self {
            exit_code,
            message: message.into(),
        }
    }

    const fn exit_code(&self) -> CliExitCode {
        self.exit_code
    }

    fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for CliCommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CliCommandError {}

pub(in crate::cli) fn invalid_args_error(message: impl Into<String>) -> anyhow::Error {
    CliCommandError::new(CliExitCode::InvalidArgs, message).into()
}

pub(in crate::cli) fn not_found_error(message: impl Into<String>) -> anyhow::Error {
    CliCommandError::new(CliExitCode::NotFound, message).into()
}

pub(in crate::cli) fn db_error(message: impl Into<String>) -> anyhow::Error {
    CliCommandError::new(CliExitCode::DbError, message).into()
}

pub(in crate::cli) fn platform_error(message: impl Into<String>) -> anyhow::Error {
    CliCommandError::new(CliExitCode::PlatformError, message).into()
}

pub(super) fn classify_clap_error(error: clap::Error) -> CliError {
    CliError::new(classify_clap_exit_code(error.kind()), error.to_string())
}

pub(super) fn classify_clap_exit_code(kind: ErrorKind) -> CliExitCode {
    match kind {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => CliExitCode::Ok,
        _ => CliExitCode::InvalidArgs,
    }
}

pub(super) fn classify_command_error(error: anyhow::Error) -> CliError {
    if let Some(error) = error.downcast_ref::<CliCommandError>() {
        return CliError::new(error.exit_code(), error.message().to_string());
    }

    let message = sanitize_error_message(&error);
    let exit_code = if is_unsupported_format_error(&error, &message) {
        CliExitCode::UnsupportedFormat
    } else if is_not_found_error(&error, &message) {
        CliExitCode::NotFound
    } else if is_platform_error(&error, &message) {
        CliExitCode::PlatformError
    } else if is_invalid_argument_error(&error, &message) {
        CliExitCode::InvalidArgs
    } else if is_db_error(&error, &message) {
        CliExitCode::DbError
    } else {
        CliExitCode::Internal
    };

    CliError::new(exit_code, message)
}

pub(super) fn sanitize_error_message(error: &anyhow::Error) -> String {
    let chain_messages = error
        .chain()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>();
    let message = chain_messages
        .first()
        .cloned()
        .unwrap_or_else(|| "command failed".to_string());
    let lower_chain = chain_messages.join(" | ").to_ascii_lowercase();

    if lower_chain.contains("prerelease schema")
        || lower_chain.contains("no such column:")
        || lower_chain.contains("sql logic error")
    {
        "database operation failed; this may be an incompatible prerelease schema. Move the database aside and run `clipmem setup`.".to_string()
    } else if lower_chain.contains("sqlite") {
        "database operation failed; run `clipmem doctor`, and if this is an older prerelease archive, move it aside and run `clipmem setup`.".to_string()
    } else {
        message
    }
}

pub(super) fn is_invalid_argument_error(error: &anyhow::Error, message: &str) -> bool {
    error.downcast_ref::<clap::Error>().is_some()
        || message.contains("cursor does not match")
        || message.contains("cursor is for command")
        || message.contains("cursor mode")
        || message.contains("invalid cursor")
        || message.contains("already exists (pass --force to replace it)")
        || message.contains("symbolic link")
        || message.contains("not a regular file")
}

pub(super) fn is_not_found_error(error: &anyhow::Error, message: &str) -> bool {
    if error
        .chain()
        .any(|cause| cause.downcast_ref::<rusqlite::Error>().is_some())
    {
        return false;
    }

    message.contains(" was not found")
        || message.contains("representation not found")
        || message.starts_with("get failed for snapshot ")
        || message.contains("Missing /")
        || message.starts_with("Missing ")
}

pub(super) fn is_unsupported_format_error(_error: &anyhow::Error, message: &str) -> bool {
    _error.downcast_ref::<UnsupportedFormatError>().is_some()
        || message.contains("unsupported format")
}

pub(super) fn is_platform_error(error: &anyhow::Error, message: &str) -> bool {
    message.contains("clipboard capture is only supported on macOS")
        || message.contains("clipboard restore is only supported on macOS")
        || message.contains("setup and service commands are only supported on macOS")
        || message.contains("capture failed")
        || message.contains("capture-once clipboard read failed")
        || message.contains("restore failed")
        || message.contains("read clipboard change count failed")
        || error.chain().any(|cause| {
            let cause = cause.to_string();
            cause.contains("clipboard capture is only supported on macOS")
                || cause.contains("clipboard restore is only supported on macOS")
        })
}

pub(super) fn is_db_error(error: &anyhow::Error, message: &str) -> bool {
    error.downcast_ref::<rusqlite::Error>().is_some()
        || error
            .chain()
            .any(|cause| cause.downcast_ref::<rusqlite::Error>().is_some())
        || message.contains("database")
        || message.contains("SQLite")
        || message.contains("failed to open database")
        || message.contains("failed to write")
        || message.contains("query failed")
}
