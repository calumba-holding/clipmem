use super::*;

#[derive(Debug, Serialize)]
pub(in crate::cli) struct CaptureOnceStoredOutput {
    pub(in crate::cli) store: CaptureStoreResult,
    pub(in crate::cli) snapshot: ClipboardSnapshot,
}

#[derive(Debug, Serialize)]
pub(in crate::cli) struct CaptureOnceSkippedOutput {
    pub(in crate::cli) status: &'static str,
    pub(in crate::cli) reason: CaptureSkipReason,
    pub(in crate::cli) kind: String,
    pub(in crate::cli) total_bytes: usize,
    pub(in crate::cli) frontmost_app_name: Option<String>,
    pub(in crate::cli) frontmost_app_bundle_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(in crate::cli) enum CaptureOnceOutput {
    Stored(CaptureOnceStoredOutput),
    Skipped(CaptureOnceSkippedOutput),
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::cli) struct RestoreOutput {
    pub(in crate::cli) snapshot_id: i64,
    pub(in crate::cli) item_count: usize,
    pub(in crate::cli) representation_count: usize,
    pub(in crate::cli) total_bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::cli) struct ExportOutput {
    pub(in crate::cli) snapshot_id: i64,
    pub(in crate::cli) item_index: usize,
    pub(in crate::cli) uti: String,
    pub(in crate::cli) byte_count: usize,
    pub(in crate::cli) raw_sha256: String,
    pub(in crate::cli) out: String,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::cli) struct SettingsView {
    pub(in crate::cli) paused: bool,
    pub(in crate::cli) api_key_filter_enabled: bool,
    pub(in crate::cli) ocr_enabled: bool,
    pub(in crate::cli) retention_seconds: Option<u64>,
    pub(in crate::cli) retention: String,
    pub(in crate::cli) ignored_bundle_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::cli) struct SettingsIgnoreListOutput {
    pub(in crate::cli) ignored_bundle_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::cli) struct SearchCursorToken {
    pub(in crate::cli) command: String,
    pub(in crate::cli) query: String,
    pub(in crate::cli) requested_mode: SearchMode,
    pub(in crate::cli) mode_used: SearchMode,
    pub(in crate::cli) filters: RetrievalFilters,
    pub(in crate::cli) last_seen_at: String,
    pub(in crate::cli) snapshot_id: i64,
    pub(in crate::cli) score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::cli) struct RecentCursorToken {
    pub(in crate::cli) command: String,
    pub(in crate::cli) filters: RetrievalFilters,
    pub(in crate::cli) last_seen_at: String,
    pub(in crate::cli) snapshot_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::cli) struct TimelineCursorToken {
    pub(in crate::cli) command: String,
    pub(in crate::cli) filters: RetrievalFilters,
    pub(in crate::cli) sort: TimelineSort,
    pub(in crate::cli) observed_at: String,
    pub(in crate::cli) event_id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::cli) enum RecallCandidateSource {
    Search,
    Recent,
}

pub(in crate::cli) fn capture_skip_reason_label(reason: CaptureSkipReason) -> &'static str {
    reason.as_str()
}

#[derive(Debug, Clone)]
pub(in crate::cli) struct RecallCandidate {
    pub(in crate::cli) hit: SearchHit,
    pub(in crate::cli) source: RecallCandidateSource,
    pub(in crate::cli) normalized_score: f64,
    pub(in crate::cli) sort_score: f64,
    pub(in crate::cli) app_preferred: bool,
}

#[derive(Debug)]
pub(in crate::cli) struct RecallComputation {
    pub(in crate::cli) best: RecallCandidate,
    pub(in crate::cli) alternatives: Vec<RecallCandidate>,
    pub(in crate::cli) why_selected: String,
    pub(in crate::cli) search_mode_used: Option<SearchMode>,
}

pub(in crate::cli) const OPENCLAW_SKILL_NAME: &str = "clipboard-memory";
pub(in crate::cli) const OPENCLAW_SHARED_ROOT: &str = ".openclaw/skills";
pub(in crate::cli) const OPENCLAW_WORKSPACE_ROOT: &str = ".openclaw/workspace";
pub(in crate::cli) const HERMES_SKILL_NAME: &str = "clipboard-memory";
pub(in crate::cli) const HERMES_SKILL_ROOT: &str = ".hermes/skills/productivity";

pub(in crate::cli) const OPENCLAW_SKILL_MD: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/extras/openclaw/clipboard-memory/SKILL.md"
));
pub(in crate::cli) const OPENCLAW_COMMANDS_REF: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/extras/openclaw/clipboard-memory/references/commands.md"
));
pub(in crate::cli) const OPENCLAW_TROUBLESHOOTING_REF: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/extras/openclaw/clipboard-memory/references/troubleshooting.md"
));
pub(in crate::cli) const OPENCLAW_JSON_SCHEMA_REF: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/extras/openclaw/clipboard-memory/references/json-schema.md"
));
pub(in crate::cli) const OPENCLAW_EXAMPLES_REF: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/extras/openclaw/clipboard-memory/references/examples.md"
));
pub(in crate::cli) const OPENCLAW_SETUP_CHECK_REF: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/extras/openclaw/clipboard-memory/references/setup-check.md"
));
pub(in crate::cli) const OPENCLAW_CHECK_SETUP_SH: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/extras/openclaw/clipboard-memory/scripts/check-setup.sh"
));

#[derive(Debug, Clone, Copy)]
pub(in crate::cli) struct PackagedSkillFile {
    pub(in crate::cli) relative_path: &'static str,
    pub(in crate::cli) contents: &'static str,
}

pub(in crate::cli) const HERMES_SKILL_MD: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/extras/hermes/clipboard-memory/SKILL.md"
));
pub(in crate::cli) const HERMES_COMMANDS_REF: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/extras/hermes/clipboard-memory/references/commands.md"
));
pub(in crate::cli) const HERMES_TROUBLESHOOTING_REF: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/extras/hermes/clipboard-memory/references/troubleshooting.md"
));
pub(in crate::cli) const HERMES_JSON_SCHEMA_REF: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/extras/hermes/clipboard-memory/references/json-schema.md"
));
pub(in crate::cli) const HERMES_EXAMPLES_REF: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/extras/hermes/clipboard-memory/references/examples.md"
));
pub(in crate::cli) const HERMES_SETUP_CHECK_REF: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/extras/hermes/clipboard-memory/references/setup-check.md"
));
pub(in crate::cli) const HERMES_CHECK_SETUP_SH: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/extras/hermes/clipboard-memory/scripts/check-setup.sh"
));

#[derive(Debug, Clone)]
pub(in crate::cli) struct OpenClawDoctorReport {
    pub(in crate::cli) target_dir: PathBuf,
    pub(in crate::cli) checks: Vec<OpenClawDoctorCheck>,
}

#[derive(Debug, Clone)]
pub(in crate::cli) struct OpenClawDoctorCheck {
    pub(in crate::cli) status: OpenClawDoctorStatus,
    pub(in crate::cli) label: String,
    pub(in crate::cli) detail: String,
    pub(in crate::cli) next_steps: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::cli) enum OpenClawDoctorStatus {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone)]
pub(in crate::cli) struct HermesDoctorReport {
    pub(in crate::cli) target_dir: PathBuf,
    pub(in crate::cli) checks: Vec<OpenClawDoctorCheck>,
}
