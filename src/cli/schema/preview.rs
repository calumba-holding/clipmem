use std::path::PathBuf;

use clap::Args;

use super::super::formats::OutputArgs;
use super::super::help::EXPORT_AFTER_HELP;
use super::super::parsing::{parse_item_index, parse_representation_uti};
use super::RetrievalFilterArgs;

#[derive(Debug, Args)]
#[command(after_help = EXPORT_AFTER_HELP)]
pub(in crate::cli) struct ExportArgs {
    /// Snapshot identifier.
    pub(in crate::cli) snapshot_id: i64,
    /// Item index inside the stored snapshot.
    #[arg(long, value_parser = parse_item_index)]
    pub(in crate::cli) item: usize,
    /// Representation UTI to export.
    #[arg(long, value_parser = parse_representation_uti)]
    pub(in crate::cli) uti: String,
    /// Destination path for the raw bytes.
    #[arg(long)]
    pub(in crate::cli) out: PathBuf,
    /// Replace an existing regular file at the destination path.
    #[arg(long, default_value_t = false)]
    pub(in crate::cli) force: bool,
    #[command(flatten)]
    pub(in crate::cli) filters: RetrievalFilterArgs,
    #[command(flatten)]
    pub(in crate::cli) output: OutputArgs,
}

#[derive(Debug, Args)]
pub(in crate::cli) struct PreviewArgs {
    /// Snapshot identifier.
    pub(in crate::cli) snapshot_id: i64,
    /// Item index inside the stored snapshot.
    #[arg(value_parser = parse_item_index)]
    pub(in crate::cli) item: usize,
    /// Source representation UTI whose preview should be read.
    #[arg(value_parser = parse_representation_uti)]
    pub(in crate::cli) uti: String,
    /// Destination path for the preview bytes.
    #[arg(long)]
    pub(in crate::cli) out: PathBuf,
    /// Replace an existing regular file at the destination path.
    #[arg(long, default_value_t = false)]
    pub(in crate::cli) force: bool,
    #[command(flatten)]
    pub(in crate::cli) output: OutputArgs,
}
