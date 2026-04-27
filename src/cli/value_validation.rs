use clap::error::ErrorKind;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::errors::{value_error, CliValueError};

pub(super) fn validate_time_window(
    since: Option<&str>,
    until: Option<&str>,
) -> Result<(), CliValueError> {
    let Some(since) = since else {
        return Ok(());
    };
    let Some(until) = until else {
        return Ok(());
    };

    let since = OffsetDateTime::parse(since, &Rfc3339)
        .map_err(|error| value_error(ErrorKind::InvalidValue, error.to_string()))?;
    let until = OffsetDateTime::parse(until, &Rfc3339)
        .map_err(|error| value_error(ErrorKind::InvalidValue, error.to_string()))?;

    if since > until {
        Err(value_error(
            ErrorKind::InvalidValue,
            "`--since` must be earlier than or equal to `--until`",
        ))
    } else {
        Ok(())
    }
}

pub(super) fn validate_byte_window(
    min_bytes: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<(), CliValueError> {
    if matches!((min_bytes, max_bytes), (Some(min), Some(max)) if min > max) {
        return Err(value_error(
            ErrorKind::InvalidValue,
            "`--min-bytes` must be less than or equal to `--max-bytes`",
        ));
    }

    Ok(())
}

pub(super) fn normalize_nonempty_filter_value(
    value: Option<&str>,
    flag: &str,
) -> Result<Option<String>, CliValueError> {
    match value.map(str::trim) {
        Some("") => Err(value_error(
            ErrorKind::InvalidValue,
            format!("{flag} cannot be empty"),
        )),
        Some(trimmed) => Ok(Some(trimmed.to_string())),
        None => Ok(None),
    }
}
