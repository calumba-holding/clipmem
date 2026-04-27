use std::path::Path;

use anyhow::Result;

use crate::db::{CapturePolicy, CaptureSettings};

use crate::cli::commands::runtime::open_or_init_db;
use crate::cli::commands::types::{SettingsIgnoreListOutput, SettingsView};
use crate::cli::formats::{format_duration_compact, OutputFormat};
use crate::cli::human::{render_settings_ignore_list_human, render_settings_view_human};
use crate::cli::output::emit_json_or_text;
use crate::cli::schema::{
    SettingsApiKeyFilterArgs, SettingsArgs, SettingsCommand, SettingsIgnoreArgs,
    SettingsIgnoreCommand, SettingsIgnoreListArgs, SettingsOcrArgs, SettingsPauseArgs,
    SettingsRetentionArgs, SettingsShowArgs,
};

use super::mutation_support::require_text_or_json;

pub(in crate::cli) fn settings(db_path: &Path, args: &SettingsArgs) -> Result<()> {
    match &args.command {
        SettingsCommand::Show(args) => settings_show(db_path, args),
        SettingsCommand::Pause(args) => settings_pause(db_path, args),
        SettingsCommand::ApiKeyFilter(args) => settings_api_key_filter(db_path, args),
        SettingsCommand::Ocr(args) => settings_ocr(db_path, args),
        SettingsCommand::Retention(args) => settings_retention(db_path, args),
        SettingsCommand::Ignore(args) => settings_ignore(db_path, args),
    }
}

fn settings_show(db_path: &Path, args: &SettingsShowArgs) -> Result<()> {
    let format = require_text_or_json(args.output.resolved()?, "settings show")?;
    let db = open_or_init_db(db_path)?;
    let view = settings_view(db.capture_policy()?);
    match format {
        OutputFormat::Json => emit_json_or_text(true, &view, render_settings_view_text)?,
        OutputFormat::Human => print!("{}", render_settings_view_human(&view)),
        OutputFormat::Text => print!("{}", render_settings_view_text(&view)),
        _ => unreachable!("unsupported settings show format should be rejected earlier"),
    }
    Ok(())
}

fn settings_pause(db_path: &Path, args: &SettingsPauseArgs) -> Result<()> {
    let db = open_or_init_db(db_path)?;
    let settings = db.set_paused(args.state.is_on())?;
    let view = settings_view(CapturePolicy::new(settings, db.list_ignored_bundle_ids()?));
    emit_json_or_text(false, &view, render_settings_view_text)?;
    Ok(())
}

fn settings_api_key_filter(db_path: &Path, args: &SettingsApiKeyFilterArgs) -> Result<()> {
    let db = open_or_init_db(db_path)?;
    let settings = db.set_api_key_filter_enabled(args.state.is_on())?;
    let view = settings_view(CapturePolicy::new(settings, db.list_ignored_bundle_ids()?));
    emit_json_or_text(false, &view, render_settings_view_text)?;
    Ok(())
}

fn settings_ocr(db_path: &Path, args: &SettingsOcrArgs) -> Result<()> {
    let db = open_or_init_db(db_path)?;
    let settings = db.set_ocr_enabled(args.state.is_on())?;
    let view = settings_view(CapturePolicy::new(settings, db.list_ignored_bundle_ids()?));
    emit_json_or_text(false, &view, render_settings_view_text)?;
    Ok(())
}

fn settings_retention(db_path: &Path, args: &SettingsRetentionArgs) -> Result<()> {
    let db = open_or_init_db(db_path)?;
    let settings = db.set_retention_seconds(args.value.retention_seconds())?;
    let view = settings_view(CapturePolicy::new(settings, db.list_ignored_bundle_ids()?));
    emit_json_or_text(false, &view, render_settings_view_text)?;
    Ok(())
}

fn settings_ignore(db_path: &Path, args: &SettingsIgnoreArgs) -> Result<()> {
    match &args.command {
        SettingsIgnoreCommand::Add(args) => settings_ignore_add(db_path, &args.bundle_id),
        SettingsIgnoreCommand::Remove(args) => settings_ignore_remove(db_path, &args.bundle_id),
        SettingsIgnoreCommand::List(args) => settings_ignore_list(db_path, args),
    }
}

fn settings_ignore_add(db_path: &Path, bundle_id: &str) -> Result<()> {
    let db = open_or_init_db(db_path)?;
    db.add_ignored_bundle_id(bundle_id)?;
    let output = SettingsIgnoreListOutput {
        ignored_bundle_ids: db.list_ignored_bundle_ids()?,
    };
    emit_json_or_text(false, &output, render_settings_ignore_list_text)?;
    Ok(())
}

fn settings_ignore_remove(db_path: &Path, bundle_id: &str) -> Result<()> {
    let db = open_or_init_db(db_path)?;
    db.remove_ignored_bundle_id(bundle_id)?;
    let output = SettingsIgnoreListOutput {
        ignored_bundle_ids: db.list_ignored_bundle_ids()?,
    };
    emit_json_or_text(false, &output, render_settings_ignore_list_text)?;
    Ok(())
}

fn settings_ignore_list(db_path: &Path, args: &SettingsIgnoreListArgs) -> Result<()> {
    let format = require_text_or_json(args.output.resolved()?, "settings ignore list")?;
    let db = open_or_init_db(db_path)?;
    let output = SettingsIgnoreListOutput {
        ignored_bundle_ids: db.list_ignored_bundle_ids()?,
    };
    match format {
        OutputFormat::Json => emit_json_or_text(true, &output, render_settings_ignore_list_text)?,
        OutputFormat::Human => print!("{}", render_settings_ignore_list_human(&output)),
        OutputFormat::Text => print!("{}", render_settings_ignore_list_text(&output)),
        _ => unreachable!("unsupported settings ignore list format should be rejected earlier"),
    }
    Ok(())
}

fn settings_view(policy: CapturePolicy) -> SettingsView {
    SettingsView {
        paused: policy.settings().paused(),
        api_key_filter_enabled: policy.settings().api_key_filter_enabled(),
        ocr_enabled: policy.settings().ocr_enabled(),
        retention_seconds: policy.settings().retention_seconds(),
        retention: render_retention_value(policy.settings()),
        ignored_bundle_ids: policy.ignored_bundle_ids().to_vec(),
    }
}

fn render_settings_view_text(view: &SettingsView) -> String {
    let mut out = String::new();
    out.push_str(&format!("paused: {}\n", view.paused));
    out.push_str(&format!(
        "api key filter: {}\n",
        view.api_key_filter_enabled
    ));
    out.push_str(&format!("ocr: {}\n", view.ocr_enabled));
    out.push_str(&format!("retention: {}\n", view.retention));
    out.push_str(&format!(
        "ignored bundle ids: {}\n",
        view.ignored_bundle_ids.len()
    ));
    for bundle_id in &view.ignored_bundle_ids {
        out.push_str(&format!("  - {bundle_id}\n"));
    }
    out
}

fn render_settings_ignore_list_text(output: &SettingsIgnoreListOutput) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "ignored bundle ids: {}\n",
        output.ignored_bundle_ids.len()
    ));
    for bundle_id in &output.ignored_bundle_ids {
        out.push_str(&format!("  - {bundle_id}\n"));
    }
    out
}

fn render_retention_value(settings: &CaptureSettings) -> String {
    settings
        .retention_seconds()
        .map(format_duration_compact)
        .unwrap_or_else(|| "forever".to_string())
}

#[cfg(test)]
mod tests {
    use crate::cli::commands::types::{SettingsIgnoreListOutput, SettingsView};

    use super::{render_settings_ignore_list_text, render_settings_view_text};

    #[test]
    fn render_settings_view_text_lists_policy_and_ignored_bundle_ids() {
        let view = SettingsView {
            paused: true,
            api_key_filter_enabled: false,
            ocr_enabled: true,
            retention_seconds: Some(3_600),
            retention: "1h".to_string(),
            ignored_bundle_ids: vec![
                "com.apple.Terminal".to_string(),
                "com.example.SecretApp".to_string(),
            ],
        };

        let rendered = render_settings_view_text(&view);

        assert_eq!(
            rendered,
            concat!(
                "paused: true\n",
                "api key filter: false\n",
                "ocr: true\n",
                "retention: 1h\n",
                "ignored bundle ids: 2\n",
                "  - com.apple.Terminal\n",
                "  - com.example.SecretApp\n",
            )
        );
    }

    #[test]
    fn render_settings_ignore_list_text_handles_empty_and_populated_lists() {
        let empty = SettingsIgnoreListOutput {
            ignored_bundle_ids: Vec::new(),
        };
        assert_eq!(
            render_settings_ignore_list_text(&empty),
            "ignored bundle ids: 0\n"
        );

        let populated = SettingsIgnoreListOutput {
            ignored_bundle_ids: vec!["com.apple.Terminal".to_string()],
        };
        assert_eq!(
            render_settings_ignore_list_text(&populated),
            "ignored bundle ids: 1\n  - com.apple.Terminal\n"
        );
    }
}
