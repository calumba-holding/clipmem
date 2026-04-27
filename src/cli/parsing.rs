use anyhow::Result;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DurationValue {
    pub(in crate::cli) raw: String,
    pub(in crate::cli) seconds: u64,
}

impl DurationValue {
    #[must_use]
    pub(super) fn new(raw: String, seconds: u64) -> Self {
        Self { raw, seconds }
    }

    #[must_use]
    pub(super) fn raw(&self) -> &str {
        &self.raw
    }

    #[must_use]
    pub(super) fn seconds(&self) -> u64 {
        self.seconds
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RetentionValue {
    Forever,
    Duration(DurationValue),
}

impl RetentionValue {
    #[must_use]
    pub(super) fn retention_seconds(&self) -> Option<u64> {
        match self {
            Self::Forever => None,
            Self::Duration(duration) => Some(duration.seconds()),
        }
    }
}

pub(super) fn parse_normalized_score(value: &str) -> Result<f64, LimitParseError> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| LimitParseError(format!("invalid floating-point value '{value}'")))?;

    if (0.0..=1.0).contains(&parsed) {
        Ok(parsed)
    } else {
        Err(LimitParseError(format!(
            "value must be between 0.0 and 1.0, got {parsed}"
        )))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LimitParseError(String);

impl std::fmt::Display for LimitParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for LimitParseError {}

pub(super) fn parse_bounded_limit(value: &str) -> Result<usize, LimitParseError> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| LimitParseError(format!("invalid integer value '{value}'")))?;

    if (1..=250).contains(&parsed) {
        Ok(parsed)
    } else {
        Err(LimitParseError(format!(
            "value must be between 1 and 250, got {parsed}"
        )))
    }
}

pub(super) fn parse_rfc3339_timestamp(value: &str) -> Result<String, LimitParseError> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map_err(|_| LimitParseError(format!("invalid RFC3339 timestamp '{value}'")))
        .and_then(|timestamp| {
            timestamp
                .format(&Rfc3339)
                .map_err(|error| LimitParseError(format!("format timestamp '{value}': {error}")))
        })
}

pub(super) fn parse_nonnegative_bytes(value: &str) -> Result<usize, LimitParseError> {
    value
        .parse::<usize>()
        .map_err(|_| LimitParseError(format!("invalid integer value '{value}'")))
}

pub(super) fn parse_duration_value(value: &str) -> Result<DurationValue, LimitParseError> {
    let trimmed = value.trim();
    if trimmed.len() < 2 {
        return Err(LimitParseError(format!(
            "invalid duration '{value}'; expected <integer><unit> like 30d, 12h, or 15m"
        )));
    }

    let (amount, unit) = trimmed.split_at(trimmed.len() - 1);
    let amount = amount.parse::<u64>().map_err(|_| {
        LimitParseError(format!(
            "invalid duration '{value}'; expected an integer amount before the unit"
        ))
    })?;
    if amount == 0 {
        return Err(LimitParseError(
            "duration must be greater than zero".to_string(),
        ));
    }

    let seconds = match unit.to_ascii_lowercase().as_str() {
        "d" => amount.saturating_mul(24 * 60 * 60),
        "h" => amount.saturating_mul(60 * 60),
        "m" => amount.saturating_mul(60),
        _ => {
            return Err(LimitParseError(format!(
                "invalid duration unit '{unit}'; expected d, h, or m"
            )))
        }
    };

    if seconds == u64::MAX {
        return Err(LimitParseError(format!(
            "duration '{value}' exceeds supported range"
        )));
    }

    Ok(DurationValue::new(trimmed.to_string(), seconds))
}

pub(super) fn parse_retention_value(value: &str) -> Result<RetentionValue, LimitParseError> {
    if value.trim().eq_ignore_ascii_case("forever") {
        Ok(RetentionValue::Forever)
    } else {
        parse_duration_value(value).map(RetentionValue::Duration)
    }
}

#[cfg(test)]
mod tests {
    use super::{DurationValue, RetentionValue};

    #[test]
    fn retention_value_returns_none_for_forever_and_seconds_for_duration() {
        assert_eq!(RetentionValue::Forever.retention_seconds(), None);
        assert_eq!(
            RetentionValue::Duration(DurationValue::new("2h".to_string(), 7_200))
                .retention_seconds(),
            Some(7_200)
        );
    }
}
