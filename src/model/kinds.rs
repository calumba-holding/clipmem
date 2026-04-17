use std::error::Error;
use std::fmt;
use std::str::FromStr;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardKind {
    PlainText,
    Url,
    FileUrl,
    Html,
    Json,
    Xml,
    Rtf,
    Pdf,
    Image,
    Binary,
    Empty,
}

impl ClipboardKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PlainText => "plain_text",
            Self::Url => "url",
            Self::FileUrl => "file_url",
            Self::Html => "html",
            Self::Json => "json",
            Self::Xml => "xml",
            Self::Rtf => "rtf",
            Self::Pdf => "pdf",
            Self::Image => "image",
            Self::Binary => "binary",
            Self::Empty => "empty",
        }
    }

    #[must_use]
    pub const fn is_textual(self) -> bool {
        matches!(
            self,
            Self::PlainText
                | Self::Url
                | Self::FileUrl
                | Self::Html
                | Self::Rtf
                | Self::Json
                | Self::Xml
        )
    }

    #[must_use]
    pub const fn priority(self) -> u8 {
        match self {
            Self::PlainText => 0,
            Self::Url => 1,
            Self::FileUrl => 2,
            Self::Html => 3,
            Self::Json => 4,
            Self::Xml => 5,
            Self::Rtf => 6,
            Self::Pdf => 7,
            Self::Image => 8,
            Self::Binary => 9,
            Self::Empty => 10,
        }
    }
}

impl fmt::Display for ClipboardKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ClipboardKind {
    type Err = ParseDomainValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "plain_text" => Ok(Self::PlainText),
            "url" => Ok(Self::Url),
            "file_url" => Ok(Self::FileUrl),
            "html" => Ok(Self::Html),
            "json" => Ok(Self::Json),
            "xml" => Ok(Self::Xml),
            "rtf" => Ok(Self::Rtf),
            "pdf" => Ok(Self::Pdf),
            "image" => Ok(Self::Image),
            "binary" => Ok(Self::Binary),
            "empty" => Ok(Self::Empty),
            _ => Err(ParseDomainValueError::new("clipboard kind", value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotKind {
    Empty,
    Mixed,
    PlainText,
    Url,
    FileUrl,
    Html,
    Json,
    Xml,
    Rtf,
    Pdf,
    Image,
    Binary,
}

impl SnapshotKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Mixed => "mixed",
            Self::PlainText => "plain_text",
            Self::Url => "url",
            Self::FileUrl => "file_url",
            Self::Html => "html",
            Self::Json => "json",
            Self::Xml => "xml",
            Self::Rtf => "rtf",
            Self::Pdf => "pdf",
            Self::Image => "image",
            Self::Binary => "binary",
        }
    }
}

impl fmt::Display for SnapshotKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl From<ClipboardKind> for SnapshotKind {
    fn from(value: ClipboardKind) -> Self {
        match value {
            ClipboardKind::PlainText => Self::PlainText,
            ClipboardKind::Url => Self::Url,
            ClipboardKind::FileUrl => Self::FileUrl,
            ClipboardKind::Html => Self::Html,
            ClipboardKind::Json => Self::Json,
            ClipboardKind::Xml => Self::Xml,
            ClipboardKind::Rtf => Self::Rtf,
            ClipboardKind::Pdf => Self::Pdf,
            ClipboardKind::Image => Self::Image,
            ClipboardKind::Binary => Self::Binary,
            ClipboardKind::Empty => Self::Empty,
        }
    }
}

impl FromStr for SnapshotKind {
    type Err = ParseDomainValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "empty" => Ok(Self::Empty),
            "mixed" => Ok(Self::Mixed),
            "plain_text" => Ok(Self::PlainText),
            "url" => Ok(Self::Url),
            "file_url" => Ok(Self::FileUrl),
            "html" => Ok(Self::Html),
            "json" => Ok(Self::Json),
            "xml" => Ok(Self::Xml),
            "rtf" => Ok(Self::Rtf),
            "pdf" => Ok(Self::Pdf),
            "image" => Ok(Self::Image),
            "binary" => Ok(Self::Binary),
            _ => Err(ParseDomainValueError::new("snapshot kind", value)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParseDomainValueError {
    domain: &'static str,
    value: String,
}

impl ParseDomainValueError {
    pub(crate) fn new(domain: &'static str, value: impl Into<String>) -> Self {
        Self {
            domain,
            value: value.into(),
        }
    }
}

impl fmt::Display for ParseDomainValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown {} value '{}'", self.domain, self.value)
    }
}

impl Error for ParseDomainValueError {}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{ClipboardKind, SnapshotKind};

    #[test]
    fn clipboard_kind_round_trips_through_strings() {
        assert_eq!(
            ClipboardKind::from_str(ClipboardKind::Html.as_str()).expect("kind should parse"),
            ClipboardKind::Html
        );
        assert!(ClipboardKind::Html.is_textual());
        assert!(ClipboardKind::Image.priority() > ClipboardKind::Url.priority());
    }

    #[test]
    fn snapshot_kind_parses_and_maps_from_clipboard_kind() {
        assert_eq!(
            SnapshotKind::from(ClipboardKind::PlainText),
            SnapshotKind::PlainText
        );
        assert_eq!(
            SnapshotKind::from_str("mixed").expect("snapshot kind should parse"),
            SnapshotKind::Mixed
        );
    }
}
