use anyhow::{anyhow, Result};
use serde::Serialize;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::cli::commands::{SettingsIgnoreListOutput, SettingsView};
use crate::cli::errors::UnsupportedFormatError;
use crate::cli::formats::{OutputFormat, RecallOutputFormat, StatsOutputFormat};
use crate::cli::human::{
    render_get_human, render_image_optimization_human, render_list_human, render_recall_human,
    render_settings_ignore_list_human, render_settings_view_human, render_stats_human,
    render_storage_compact_human,
};
use crate::cli::output::json::{print_json, print_json_line, print_jsonl_list};
use crate::cli::output::markdown::{
    render_get_markdown, render_list_markdown, render_recall_markdown,
};
use crate::cli::output::model::{GetEnvelope, ListEnvelope, RecallEnvelope, StatsEnvelope};
use crate::cli::output::text::{
    render_get_text, render_image_optimization_text, render_list_text,
    render_settings_ignore_list_text, render_settings_view_text, render_stats_text,
    render_storage_compact_text,
};
use crate::cli::output::toon::{render_list_toon, render_recall_toon};
use crate::db::{ImageOptimizationReport, StorageCompactReport};

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
        OutputFormat::Text => {
            print!("{}", render_list_text(envelope));
            Ok(())
        }
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
        OutputFormat::Text => {
            print!("{}", render_get_text(envelope));
            Ok(())
        }
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

pub(in crate::cli) fn emit_stats_output(
    format: StatsOutputFormat,
    envelope: &StatsEnvelope,
) -> Result<()> {
    match format {
        StatsOutputFormat::Text => {
            print!("{}", render_stats_text(envelope));
            Ok(())
        }
        StatsOutputFormat::Json => print_json(envelope),
        StatsOutputFormat::Human => {
            print!("{}", render_stats_human(envelope));
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

pub(in crate::cli) fn emit_storage_compact_output(
    format: OutputFormat,
    report: &StorageCompactReport,
) -> Result<()> {
    match format {
        OutputFormat::Json => print_json(report),
        OutputFormat::Human => {
            print!("{}", render_storage_compact_human(report));
            Ok(())
        }
        OutputFormat::Text => {
            print!("{}", render_storage_compact_text(report));
            Ok(())
        }
        _ => unreachable!("unsupported storage compact format should be rejected earlier"),
    }
}

pub(in crate::cli) fn emit_image_optimization_output(
    format: OutputFormat,
    report: &ImageOptimizationReport,
) -> Result<()> {
    match format {
        OutputFormat::Json => print_json(report),
        OutputFormat::Human => {
            print!("{}", render_image_optimization_human(report));
            Ok(())
        }
        OutputFormat::Text => {
            print!("{}", render_image_optimization_text(report));
            Ok(())
        }
        _ => unreachable!("unsupported storage optimize-images format should be rejected earlier"),
    }
}

pub(in crate::cli) fn emit_settings_view_output(
    format: OutputFormat,
    view: &SettingsView,
) -> Result<()> {
    match format {
        OutputFormat::Json => print_json(view),
        OutputFormat::Human => {
            print!("{}", render_settings_view_human(view));
            Ok(())
        }
        OutputFormat::Text => {
            print!("{}", render_settings_view_text(view));
            Ok(())
        }
        _ => unreachable!("unsupported settings show format should be rejected earlier"),
    }
}

pub(in crate::cli) fn emit_settings_ignore_list_output(
    format: OutputFormat,
    output: &SettingsIgnoreListOutput,
) -> Result<()> {
    match format {
        OutputFormat::Json => print_json(output),
        OutputFormat::Human => {
            print!("{}", render_settings_ignore_list_human(output));
            Ok(())
        }
        OutputFormat::Text => {
            print!("{}", render_settings_ignore_list_text(output));
            Ok(())
        }
        _ => unreachable!("unsupported settings ignore list format should be rejected earlier"),
    }
}
