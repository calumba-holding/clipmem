use anyhow::Result;

use crate::cli::formats::OutputFormat;
use crate::cli::output::UnsupportedFormatError;

pub(in crate::cli) fn require_text_or_json(
    format: OutputFormat,
    command_name: &str,
) -> Result<OutputFormat> {
    match format {
        OutputFormat::Text | OutputFormat::Json | OutputFormat::Human => Ok(format),
        other => Err(UnsupportedFormatError::new(format!(
            "{command_name} only supports `text`, `json`, and `human` output, got `{}`",
            other.as_str()
        ))
        .into()),
    }
}

#[cfg(test)]
mod tests {
    use crate::cli::formats::OutputFormat;

    use super::require_text_or_json;

    #[test]
    fn require_text_or_json_accepts_text_json_and_human() {
        assert!(matches!(
            require_text_or_json(OutputFormat::Text, "export").unwrap(),
            OutputFormat::Text
        ));
        assert!(matches!(
            require_text_or_json(OutputFormat::Json, "export").unwrap(),
            OutputFormat::Json
        ));
        assert!(matches!(
            require_text_or_json(OutputFormat::Human, "export").unwrap(),
            OutputFormat::Human
        ));
    }

    #[test]
    fn require_text_or_json_rejects_other_formats_with_command_name() {
        for format in [OutputFormat::Jsonl, OutputFormat::Md, OutputFormat::Toon] {
            let error = require_text_or_json(format, "storage compact").unwrap_err();
            let message = error.to_string();

            assert!(message.contains("storage compact only supports"));
            assert!(message.contains(format.as_str()));
        }
    }
}
