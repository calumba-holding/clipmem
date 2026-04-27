use anyhow::{anyhow, Result};
use serde::Serialize;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::cli::formats::{OutputFormat, RecallOutputFormat};
use crate::cli::human::{render_get_human, render_list_human, render_recall_human};
use crate::cli::output::json::{print_json, print_json_line, print_jsonl_list};
use crate::cli::output::markdown::{
    render_get_markdown, render_list_markdown, render_recall_markdown,
};
use crate::cli::output::model::{
    GetEnvelope, ListEnvelope, RecallEnvelope, UnsupportedFormatError,
};
use crate::cli::output::toon::{render_list_toon, render_recall_toon};

pub(in crate::cli) fn generated_at_now() -> Result<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| anyhow!("format generated timestamp: {error}"))
}

pub(in crate::cli) fn emit_json_or_text<T>(
    json: bool,
    value: &T,
    render_text: impl FnOnce(&T) -> String,
) -> Result<()>
where
    T: Serialize,
{
    if json {
        print_json(value)
    } else {
        print!("{}", render_text(value));
        Ok(())
    }
}

pub(in crate::cli) fn emit_list_output(
    format: OutputFormat,
    envelope: &ListEnvelope,
) -> Result<()> {
    match format {
        OutputFormat::Text => unreachable!("list text output uses the legacy text renderer"),
        OutputFormat::Json => print_json(envelope),
        OutputFormat::Jsonl => print_jsonl_list(envelope),
        OutputFormat::Md => {
            print!("{}", render_list_markdown(envelope));
            Ok(())
        }
        OutputFormat::Toon => {
            print!("{}", render_list_toon(envelope));
            Ok(())
        }
        OutputFormat::Human => {
            print!("{}", render_list_human(envelope));
            Ok(())
        }
    }
}

pub(in crate::cli) fn emit_get_output(format: OutputFormat, envelope: &GetEnvelope) -> Result<()> {
    match format {
        OutputFormat::Text => unreachable!("get text output uses the legacy text renderer"),
        OutputFormat::Json => print_json(envelope),
        OutputFormat::Jsonl => {
            print_json_line(envelope)
        }
        OutputFormat::Md => {
            print!("{}", render_get_markdown(envelope));
            Ok(())
        }
        OutputFormat::Toon => Err(UnsupportedFormatError::new(
            "format toon is only supported for flattened list outputs; `clipmem get` returns nested snapshot detail",
        )
        .into()),
        OutputFormat::Human => {
            print!("{}", render_get_human(envelope));
            Ok(())
        }
    }
}

pub(in crate::cli) fn emit_recall_output(
    format: RecallOutputFormat,
    envelope: &RecallEnvelope,
) -> Result<()> {
    match format {
        RecallOutputFormat::Json => print_json(envelope),
        RecallOutputFormat::Md => {
            print!("{}", render_recall_markdown(envelope));
            Ok(())
        }
        RecallOutputFormat::Toon => {
            print!("{}", render_recall_toon(envelope));
            Ok(())
        }
        RecallOutputFormat::Human => {
            print!("{}", render_recall_human(envelope));
            Ok(())
        }
    }
}
