use super::*;

pub(super) fn validate_cli(cli: &Cli) -> std::result::Result<(), clap::Error> {
    match &cli.command {
        Command::Agents(_args) => {}
        Command::Setup(_) => {}
        Command::Service(args) => {
            if let ServiceCommand::Status(args) = &args.command {
                validate_json_human_flags(args.json, args.human)?;
            }
        }
        Command::Search(args) => {
            args.output.resolved()?;
            args.filters.normalized()?;
        }
        Command::Recent(args) => {
            args.output.resolved()?;
            args.filters.normalized()?;
        }
        Command::Timeline(args) => {
            args.output.resolved()?;
            args.filters.normalized()?;
        }
        Command::Stats(args) => {
            args.output.resolved()?;
            args.filters.normalized()?;
        }
        Command::Ocr(args) => match &args.command {
            OcrCommand::Status(args) => {
                args.output.resolved()?;
            }
            OcrCommand::Run(args) => {
                args.output.resolved()?;
            }
        },
        Command::Recall(args) => {
            args.output.resolved()?;
            args.filters.normalized()?;
        }
        Command::Get(args) => {
            args.output.resolved()?;
            args.filters.normalized()?;
        }
        Command::Export(args) => {
            args.output.resolved()?;
            args.filters.normalized()?;
        }
        Command::Restore(args) => {
            args.output.resolved()?;
        }
        Command::Forget(args) => {
            args.output.resolved()?;
        }
        Command::Purge(args) => {
            args.output.resolved()?;
        }
        Command::Storage(args) => match &args.command {
            StorageCommand::Compact(args) => {
                args.output.resolved()?;
            }
            StorageCommand::OptimizeImages(args) => {
                validate_optimize_images_progress(args)?;
                args.output.resolved()?;
            }
        },
        Command::Settings(args) => match &args.command {
            SettingsCommand::Show(args) => {
                args.output.resolved()?;
            }
            SettingsCommand::Pause(_) | SettingsCommand::ApiKeyFilter(_) => {}
            SettingsCommand::Ocr(_) => {}
            SettingsCommand::Retention(_) => {}
            SettingsCommand::Ignore(args) => match &args.command {
                SettingsIgnoreCommand::Add(_) | SettingsIgnoreCommand::Remove(_) => {}
                SettingsIgnoreCommand::List(args) => {
                    args.output.resolved()?;
                }
            },
        },
        Command::Watch(_) => {}
        Command::CaptureOnce(args) => {
            validate_json_human_flags(args.json, args.human)?;
        }
        Command::Doctor(args) => {
            validate_json_human_flags(args.json, args.human)?;
        }
    }

    Ok(())
}

fn validate_optimize_images_progress(
    args: &StorageOptimizeImagesArgs,
) -> std::result::Result<(), clap::Error> {
    if let Some(progress) = args.progress {
        if args.output.format.is_some() || args.output.json || args.output.human {
            return Err(Cli::command().error(
                ErrorKind::ArgumentConflict,
                format!(
                    "`--progress {}` cannot be combined with `--format`, `--json`, or `--human`",
                    progress.as_str()
                ),
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_json_human_flags(
    json: bool,
    human: bool,
) -> std::result::Result<(), clap::Error> {
    if json && human {
        Err(Cli::command().error(
            ErrorKind::ArgumentConflict,
            "`--human` cannot be combined with `--json`",
        ))
    } else {
        Ok(())
    }
}

pub(super) fn validate_time_window(
    since: Option<&str>,
    until: Option<&str>,
) -> std::result::Result<(), clap::Error> {
    let Some(since) = since else {
        return Ok(());
    };
    let Some(until) = until else {
        return Ok(());
    };

    let since = OffsetDateTime::parse(since, &Rfc3339)
        .map_err(|error| Cli::command().error(ErrorKind::InvalidValue, error.to_string()))?;
    let until = OffsetDateTime::parse(until, &Rfc3339)
        .map_err(|error| Cli::command().error(ErrorKind::InvalidValue, error.to_string()))?;

    if since > until {
        Err(Cli::command().error(
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
) -> std::result::Result<(), clap::Error> {
    if matches!((min_bytes, max_bytes), (Some(min), Some(max)) if min > max) {
        return Err(Cli::command().error(
            ErrorKind::InvalidValue,
            "`--min-bytes` must be less than or equal to `--max-bytes`",
        ));
    }

    Ok(())
}

pub(super) fn normalize_nonempty_filter_value(
    value: Option<&str>,
    flag: &str,
) -> std::result::Result<Option<String>, clap::Error> {
    match value.map(str::trim) {
        Some("") => {
            Err(Cli::command().error(ErrorKind::InvalidValue, format!("{flag} cannot be empty")))
        }
        Some(trimmed) => Ok(Some(trimmed.to_string())),
        None => Ok(None),
    }
}
