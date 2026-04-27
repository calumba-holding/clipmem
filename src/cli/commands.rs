use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::process::Command as ProcessCommand;

use crate::app::{format_watch_capture_line, mark_change_handled, WatchState};
use crate::db::{
    CapturePolicy, CaptureSettings, CaptureSkipReason, CaptureStoreOutcome, Database,
    ImageOptimizationReport, OcrRunReport, OcrStatusReport, PurgeReport, RecentCursorState,
    RetrievalFilters, SearchCursorState, SearchMode, SearchResults, SnapshotDeletionReport,
    StorageCompactReport, TimelineCursorState, TimelineSort,
};
use crate::model::{
    CaptureStoreResult, ClipboardSnapshot, FlattenedTextProjection, SearchHit, TimelineEvent,
};
use crate::platform::{capture_snapshot, current_change_count, restore_items};

use super::human::{
    render_capture_once_human, render_doctor_human, render_export_human, render_forget_human,
    render_image_optimization_human, render_ocr_run_human, render_ocr_status_human,
    render_purge_human, render_restore_human, render_service_status_human,
    render_settings_ignore_list_human, render_settings_view_human, render_stats_human,
    render_storage_compact_human,
};
use super::output::{
    emit_get_output, emit_json_or_text, emit_list_output, emit_recall_output, generated_at_now,
    print_json_line, render_capture_once_text, render_doctor_text, render_hits_text,
    render_search_results_text, render_snapshot_text, render_stats_text, render_timeline_text,
    GetEnvelope, ListEnvelope, ListRow, RecallEnvelope, RecallMatchConfidence, RecallOutputRow,
    StatsEnvelope, UnsupportedFormatError, OUTPUT_SCHEMA_VERSION,
};
use super::service::{
    self, render_service_action_text, render_service_status_text, render_setup_text,
};
use super::{
    format_duration_compact, AgentsArgs, AgentsCommand, CaptureOnceArgs, Command, DoctorArgs,
    ExportArgs, ForgetArgs, GetArgs, HermesArgs, HermesCommand, HermesDoctorArgs,
    HermesInstallSkillArgs, HermesUninstallSkillArgs, OcrArgs, OcrCommand, OcrRunArgs,
    OcrStatusArgs, OpenClawArgs, OpenClawCommand, OpenClawDoctorArgs, OpenClawInstallSkillArgs,
    OpenClawUninstallSkillArgs, OutputFormat, ProgressFormat, PurgeArgs, RecallArgs, RecentArgs,
    RestoreArgs, RetrievalFilterArgs, SearchArgs, ServiceArgs, ServiceCommand, ServiceStatusArgs,
    SettingsApiKeyFilterArgs, SettingsArgs, SettingsCommand, SettingsIgnoreArgs,
    SettingsIgnoreCommand, SettingsIgnoreListArgs, SettingsOcrArgs, SettingsPauseArgs,
    SettingsRetentionArgs, SettingsShowArgs, SetupArgs, StatsArgs, StatsOutputFormat, StorageArgs,
    StorageCommand, StorageCompactArgs, StorageOptimizeImagesArgs, TimelineArgs, WatchArgs,
};

mod entry;
mod hermes_manage;
mod hermes_validate;
mod mutate;
mod openclaw_manage;
mod openclaw_validate;
mod retrieval;
mod runtime;
mod types;

pub(super) use self::entry::run_command;
pub(super) use self::hermes_manage::*;
pub(super) use self::hermes_validate::*;
pub(super) use self::mutate::*;
pub(super) use self::openclaw_manage::*;
pub(super) use self::openclaw_validate::*;
pub(super) use self::retrieval::*;
pub(super) use self::runtime::*;
pub(super) use self::types::*;

static OCR_WORKER_RUNNING: AtomicBool = AtomicBool::new(false);
