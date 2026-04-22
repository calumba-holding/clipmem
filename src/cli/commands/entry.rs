use super::*;

pub(in crate::cli) fn run_command(command: Command, db_path: &Path) -> Result<()> {
    match command {
        Command::Agents(args) => agents(&args),
        Command::Setup(args) => setup(db_path, &args),
        Command::Service(args) => service(db_path, &args),
        Command::Watch(args) => watch(db_path, &args),
        Command::CaptureOnce(args) => capture_once(db_path, &args),
        Command::Search(args) => search(db_path, &args),
        Command::Recent(args) => recent(db_path, &args),
        Command::Timeline(args) => timeline(db_path, &args),
        Command::Stats(args) => stats(db_path, &args),
        Command::Recall(args) => recall(db_path, &args),
        Command::Get(args) => show_snapshot(db_path, &args),
        Command::Export(args) => export_snapshot_bytes(db_path, &args),
        Command::Restore(args) => restore_snapshot(db_path, &args),
        Command::Forget(args) => forget_snapshot(db_path, &args),
        Command::Purge(args) => purge_snapshots(db_path, &args),
        Command::Storage(args) => storage(db_path, &args),
        Command::Ocr(args) => ocr(db_path, &args),
        Command::Settings(args) => settings(db_path, &args),
        Command::Doctor(args) => doctor(db_path, &args),
    }
}

pub(in crate::cli) fn setup(db_path: &Path, _args: &SetupArgs) -> Result<()> {
    let report = service::setup(db_path)?;
    print!("{}", render_setup_text(&report));
    Ok(())
}

pub(in crate::cli) fn service(db_path: &Path, args: &ServiceArgs) -> Result<()> {
    match &args.command {
        ServiceCommand::Start => {
            let report = service::start(db_path)?;
            print!("{}", render_service_action_text(&report));
            Ok(())
        }
        ServiceCommand::Stop => {
            let report = service::stop(db_path)?;
            print!("{}", render_service_action_text(&report));
            Ok(())
        }
        ServiceCommand::Status(status_args) => service_status(db_path, status_args),
        ServiceCommand::Uninstall => {
            let report = service::uninstall(db_path)?;
            print!("{}", render_service_action_text(&report));
            Ok(())
        }
    }
}

pub(in crate::cli) fn service_status(db_path: &Path, args: &ServiceStatusArgs) -> Result<()> {
    let report = service::status_report(db_path)?;
    if args.human {
        print!("{}", render_service_status_human(&report));
    } else if args.json {
        emit_json_or_text(true, &report, render_service_status_text)?;
    } else {
        print!("{}", render_service_status_text(&report));
    }
    Ok(())
}

pub(in crate::cli) fn agents(args: &AgentsArgs) -> Result<()> {
    match &args.command {
        AgentsCommand::Openclaw(args) => openclaw(args),
        AgentsCommand::Hermes(args) => hermes(args),
    }
}

pub(in crate::cli) fn openclaw(args: &OpenClawArgs) -> Result<()> {
    match &args.command {
        OpenClawCommand::InstallSkill(args) => openclaw_install_skill(args),
        OpenClawCommand::UninstallSkill(args) => openclaw_uninstall_skill(args),
        OpenClawCommand::PrintSkill => {
            print!("{}", packaged_openclaw_skill());
            Ok(())
        }
        OpenClawCommand::Doctor(args) => openclaw_doctor(args),
    }
}

pub(in crate::cli) fn hermes(args: &HermesArgs) -> Result<()> {
    match &args.command {
        HermesCommand::InstallSkill(args) => hermes_install_skill(args),
        HermesCommand::UninstallSkill(args) => hermes_uninstall_skill(args),
        HermesCommand::PrintSkill => {
            print!("{}", packaged_hermes_skill());
            Ok(())
        }
        HermesCommand::Doctor(args) => hermes_doctor(args),
    }
}
