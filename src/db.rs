mod read;
mod store;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::ValueEnum;
use rusqlite::{Connection, OpenFlags, Row};
use serde::{Deserialize, Serialize};

use crate::model::SearchHit;

const SCHEMA: &str = include_str!("db/schema.sql");
const CURRENT_SCHEMA_VERSION: i64 = 8;
const LEGACY_PRERELEASE_COLUMNS: &[&str] = &["classification", "is_text"];

pub struct Database {
    pub(super) conn: Connection,
    pub(super) path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum SearchMode {
    Auto,
    Fts,
    Literal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum TimelineSort {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalKind {
    Text,
    Html,
    Rtf,
    Url,
    File,
    Image,
    Pdf,
    Binary,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RetrievalFilters {
    since: Option<String>,
    until: Option<String>,
    hours: Option<u32>,
    app: Option<String>,
    bundle_id: Option<String>,
    kind: Option<RetrievalKind>,
    has_text: bool,
    has_url: bool,
    has_file_url: bool,
    has_image: bool,
    has_pdf: bool,
    min_bytes: Option<usize>,
    max_bytes: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) struct CaptureSettings {
    paused: bool,
    retention_seconds: Option<u64>,
    api_key_filter_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub(crate) struct CapturePolicy {
    settings: CaptureSettings,
    ignored_bundle_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CaptureSkipReason {
    ApiKeyFilter,
}

#[derive(Debug, Clone)]
pub(crate) enum CaptureStoreOutcome {
    Stored(crate::model::CaptureStoreResult),
    Skipped(CaptureSkipReason),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SnapshotDeletionReport {
    snapshot_id: i64,
    item_count: usize,
    representation_count: usize,
    capture_event_count: usize,
    total_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PurgeReport {
    older_than_seconds: u64,
    dry_run: bool,
    snapshot_count: usize,
    item_count: usize,
    representation_count: usize,
    capture_event_count: usize,
    total_bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResults {
    mode_used: SearchMode,
    hits: Vec<SearchHit>,
    has_more: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Page<T> {
    items: Vec<T>,
    has_more: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RecentCursorState {
    last_seen_at: String,
    snapshot_id: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SearchCursorState {
    mode_used: SearchMode,
    score: Option<f64>,
    last_seen_at: String,
    snapshot_id: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TimelineCursorState {
    observed_at: String,
    event_id: i64,
}

impl SearchResults {
    #[must_use]
    pub(crate) fn new(mode_used: SearchMode, hits: Vec<SearchHit>, has_more: bool) -> Self {
        Self {
            mode_used,
            hits,
            has_more,
        }
    }

    #[must_use]
    pub fn mode_used(&self) -> SearchMode {
        self.mode_used
    }

    #[must_use]
    pub fn hits(&self) -> &[SearchHit] {
        &self.hits
    }

    #[must_use]
    pub fn has_more(&self) -> bool {
        self.has_more
    }
}

impl<T> Page<T> {
    #[must_use]
    pub(crate) fn new(items: Vec<T>, has_more: bool) -> Self {
        Self { items, has_more }
    }

    #[must_use]
    pub(crate) fn items(&self) -> &[T] {
        &self.items
    }

    #[must_use]
    pub(crate) fn into_items(self) -> Vec<T> {
        self.items
    }

    #[must_use]
    pub(crate) fn has_more(&self) -> bool {
        self.has_more
    }
}

impl RecentCursorState {
    #[must_use]
    pub(crate) fn new(last_seen_at: String, snapshot_id: i64) -> Self {
        Self {
            last_seen_at,
            snapshot_id,
        }
    }

    #[must_use]
    pub(crate) fn last_seen_at(&self) -> &str {
        &self.last_seen_at
    }

    #[must_use]
    pub(crate) fn snapshot_id(&self) -> i64 {
        self.snapshot_id
    }
}

impl SearchCursorState {
    #[must_use]
    pub(crate) fn new(
        mode_used: SearchMode,
        score: Option<f64>,
        last_seen_at: String,
        snapshot_id: i64,
    ) -> Self {
        Self {
            mode_used,
            score,
            last_seen_at,
            snapshot_id,
        }
    }

    #[must_use]
    pub(crate) fn mode_used(&self) -> SearchMode {
        self.mode_used
    }

    #[must_use]
    pub(crate) fn score(&self) -> Option<f64> {
        self.score
    }

    #[must_use]
    pub(crate) fn last_seen_at(&self) -> &str {
        &self.last_seen_at
    }

    #[must_use]
    pub(crate) fn snapshot_id(&self) -> i64 {
        self.snapshot_id
    }
}

impl SearchMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Fts => "fts",
            Self::Literal => "literal",
        }
    }
}

impl TimelineSort {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

impl RetrievalKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Html => "html",
            Self::Rtf => "rtf",
            Self::Url => "url",
            Self::File => "file",
            Self::Image => "image",
            Self::Pdf => "pdf",
            Self::Binary => "binary",
            Self::Other => "other",
        }
    }
}

impl RetrievalFilters {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        since: Option<String>,
        until: Option<String>,
        hours: Option<u32>,
        app: Option<String>,
        bundle_id: Option<String>,
        kind: Option<RetrievalKind>,
        has_text: bool,
        has_url: bool,
        has_file_url: bool,
        has_image: bool,
        has_pdf: bool,
        min_bytes: Option<usize>,
        max_bytes: Option<usize>,
    ) -> Self {
        Self {
            since,
            until,
            hours,
            app,
            bundle_id,
            kind,
            has_text,
            has_url,
            has_file_url,
            has_image,
            has_pdf,
            min_bytes,
            max_bytes,
        }
    }

    #[must_use]
    pub fn since(&self) -> Option<&str> {
        self.since.as_deref()
    }

    #[must_use]
    pub fn until(&self) -> Option<&str> {
        self.until.as_deref()
    }

    #[must_use]
    pub fn hours(&self) -> Option<u32> {
        self.hours
    }

    #[must_use]
    pub fn app(&self) -> Option<&str> {
        self.app.as_deref()
    }

    #[must_use]
    pub fn bundle_id(&self) -> Option<&str> {
        self.bundle_id.as_deref()
    }

    #[must_use]
    pub fn kind(&self) -> Option<RetrievalKind> {
        self.kind
    }

    #[must_use]
    pub fn has_text(&self) -> bool {
        self.has_text
    }

    #[must_use]
    pub fn has_url(&self) -> bool {
        self.has_url
    }

    #[must_use]
    pub fn has_file_url(&self) -> bool {
        self.has_file_url
    }

    #[must_use]
    pub fn has_image(&self) -> bool {
        self.has_image
    }

    #[must_use]
    pub fn has_pdf(&self) -> bool {
        self.has_pdf
    }

    #[must_use]
    pub fn min_bytes(&self) -> Option<usize> {
        self.min_bytes
    }

    #[must_use]
    pub fn max_bytes(&self) -> Option<usize> {
        self.max_bytes
    }
}

impl CaptureSettings {
    #[must_use]
    pub(crate) fn new(
        paused: bool,
        retention_seconds: Option<u64>,
        api_key_filter_enabled: bool,
    ) -> Self {
        Self {
            paused,
            retention_seconds,
            api_key_filter_enabled,
        }
    }

    #[must_use]
    pub(crate) fn paused(&self) -> bool {
        self.paused
    }

    #[must_use]
    pub(crate) fn retention_seconds(&self) -> Option<u64> {
        self.retention_seconds
    }

    #[must_use]
    pub(crate) fn api_key_filter_enabled(&self) -> bool {
        self.api_key_filter_enabled
    }
}

impl CapturePolicy {
    #[must_use]
    pub(crate) fn new(settings: CaptureSettings, ignored_bundle_ids: Vec<String>) -> Self {
        Self {
            settings,
            ignored_bundle_ids,
        }
    }

    #[must_use]
    pub(crate) fn settings(&self) -> &CaptureSettings {
        &self.settings
    }

    #[must_use]
    pub(crate) fn ignored_bundle_ids(&self) -> &[String] {
        &self.ignored_bundle_ids
    }

    #[must_use]
    pub(crate) fn ignored_bundle_id_count(&self) -> usize {
        self.ignored_bundle_ids.len()
    }
}

impl CaptureSkipReason {
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ApiKeyFilter => "api_key_filter",
        }
    }
}

impl SnapshotDeletionReport {
    #[must_use]
    pub(crate) fn new(
        snapshot_id: i64,
        item_count: usize,
        representation_count: usize,
        capture_event_count: usize,
        total_bytes: usize,
    ) -> Self {
        Self {
            snapshot_id,
            item_count,
            representation_count,
            capture_event_count,
            total_bytes,
        }
    }

    #[must_use]
    pub(crate) fn snapshot_id(&self) -> i64 {
        self.snapshot_id
    }

    #[must_use]
    pub(crate) fn item_count(&self) -> usize {
        self.item_count
    }

    #[must_use]
    pub(crate) fn representation_count(&self) -> usize {
        self.representation_count
    }

    #[must_use]
    pub(crate) fn capture_event_count(&self) -> usize {
        self.capture_event_count
    }

    #[must_use]
    pub(crate) fn total_bytes(&self) -> usize {
        self.total_bytes
    }
}

impl PurgeReport {
    #[must_use]
    pub(crate) fn new(
        older_than_seconds: u64,
        dry_run: bool,
        snapshot_count: usize,
        item_count: usize,
        representation_count: usize,
        capture_event_count: usize,
        total_bytes: usize,
    ) -> Self {
        Self {
            older_than_seconds,
            dry_run,
            snapshot_count,
            item_count,
            representation_count,
            capture_event_count,
            total_bytes,
        }
    }

    #[must_use]
    pub(crate) fn older_than_seconds(&self) -> u64 {
        self.older_than_seconds
    }

    #[must_use]
    pub(crate) fn dry_run(&self) -> bool {
        self.dry_run
    }

    #[must_use]
    pub(crate) fn snapshot_count(&self) -> usize {
        self.snapshot_count
    }

    #[must_use]
    pub(crate) fn item_count(&self) -> usize {
        self.item_count
    }

    #[must_use]
    pub(crate) fn representation_count(&self) -> usize {
        self.representation_count
    }

    #[must_use]
    pub(crate) fn capture_event_count(&self) -> usize {
        self.capture_event_count
    }

    #[must_use]
    pub(crate) fn total_bytes(&self) -> usize {
        self.total_bytes
    }
}

impl TimelineCursorState {
    #[must_use]
    pub(crate) fn new(observed_at: String, event_id: i64) -> Self {
        Self {
            observed_at,
            event_id,
        }
    }

    #[must_use]
    pub(crate) fn observed_at(&self) -> &str {
        &self.observed_at
    }

    #[must_use]
    pub(crate) fn event_id(&self) -> i64 {
        self.event_id
    }
}

impl Database {
    /// Open the archive database at `path`, creating parent directories and schema state as needed.
    ///
    /// This method also hardens parent-directory and database-file permissions after opening.
    ///
    /// # Errors
    ///
    /// Returns an error if the parent directory cannot be created, the database cannot be opened,
    /// or the connection cannot be configured and bootstrapped.
    pub fn open_or_init(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            Context::with_context(std::fs::create_dir_all(parent), || {
                format!("failed to create {}", parent.display())
            })?;
            harden_path_permissions(parent, 0o700)?;
        }

        let mut conn = open_connection(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        prepare_connection(&mut conn)?;
        harden_path_permissions(path, 0o600)?;

        Ok(Self {
            conn,
            path: path.to_path_buf(),
        })
    }

    /// Open an existing archive database without creating parent directories or schema state.
    ///
    /// This method also hardens parent-directory and database-file permissions after opening.
    ///
    /// # Errors
    ///
    /// Returns an error if the database file does not already exist, cannot be opened, or the
    /// connection cannot be configured.
    pub fn open_existing(path: &Path) -> Result<Self> {
        let mut conn = open_connection(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        prepare_connection(&mut conn)?;
        if let Some(parent) = path.parent() {
            harden_path_permissions(parent, 0o700)?;
        }
        harden_path_permissions(path, 0o600)?;

        Ok(Self {
            conn,
            path: path.to_path_buf(),
        })
    }

    pub(crate) fn ensure_supported_schema_shape(&self) -> Result<()> {
        if legacy_prerelease_schema_detected(&self.conn)?
            || self
                .conn
                .prepare("SELECT kind FROM item_representations LIMIT 0")
                .is_err_and(|error| error.to_string().contains("no such column: kind"))
        {
            bail!(
                "database uses an incompatible prerelease schema; move it aside and run `clipmem setup` to initialize a fresh archive"
            );
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn open_in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        prepare_connection(&mut conn)?;
        Ok(Self {
            conn,
            path: PathBuf::from(":memory:"),
        })
    }
}

pub(super) fn sanitise_limit(limit: usize) -> usize {
    limit.clamp(1, 250)
}

pub(super) fn collect_rows<T, I>(rows: I) -> Result<Vec<T>>
where
    I: Iterator<Item = rusqlite::Result<T>>,
{
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub(super) fn usize_to_i64(value: usize) -> Result<i64> {
    anyhow::Context::context(i64::try_from(value), "value exceeds SQLite INTEGER range")
}

pub(super) fn row_usize(row: &Row<'_>, index: usize) -> rusqlite::Result<usize> {
    let value = row.get::<_, i64>(index)?;
    usize::try_from(value).map_err(|source| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            Box::new(source),
        )
    })
}

pub(super) fn row_enum<T>(row: &Row<'_>, index: usize) -> rusqlite::Result<T>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    let value = row.get::<_, String>(index)?;
    value.parse().map_err(|source| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(source),
        )
    })
}

fn open_connection(path: &Path, flags: OpenFlags) -> Result<Connection> {
    anyhow::Context::with_context(Connection::open_with_flags(path, flags), || {
        format!("failed to open {}", path.display())
    })
}

fn prepare_connection(conn: &mut Connection) -> Result<()> {
    Context::context(configure_connection(conn), "configure database connection")?;
    Context::context(prepare_schema(conn), "prepare database schema")?;
    Ok(())
}

#[cfg(unix)]
fn harden_path_permissions(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    anyhow::Context::with_context(
        std::fs::set_permissions(path, PermissionsExt::from_mode(mode)),
        || format!("failed to set permissions on {}", path.display()),
    )
}

#[cfg(not(unix))]
fn harden_path_permissions(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

fn configure_connection(conn: &Connection) -> Result<()> {
    configure_pragma(conn, "journal_mode", "WAL")?;
    configure_pragma(conn, "synchronous", "NORMAL")?;
    configure_pragma(conn, "foreign_keys", "ON")?;
    configure_pragma(conn, "temp_store", "MEMORY")?;
    conn.busy_timeout(Duration::from_millis(1_500))
        .context("configure SQLite busy timeout")?;
    Ok(())
}

fn configure_pragma(conn: &Connection, pragma: &str, value: &str) -> Result<()> {
    Context::with_context(conn.pragma_update(None, pragma, value), || {
        format!("configure {pragma} pragma")
    })?;
    Ok(())
}

fn prepare_schema(conn: &mut Connection) -> Result<()> {
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .context("begin schema transaction")?;

    tx.execute_batch(SCHEMA).context("apply database schema")?;

    let user_version: i64 = tx
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .context("read PRAGMA user_version")?;

    match user_version {
        0 => {
            tx.execute(
                "INSERT INTO snapshots_fts(snapshots_fts) VALUES ('rebuild')",
                [],
            )
            .context("rebuild FTS5 index")?;
            rebuild_snapshot_stats(&tx)?;
            rebuild_snapshot_projection_cache(&tx)?;
            rebuild_snapshot_event_filter_cache(&tx)?;
            rebuild_snapshot_literal_cache(&tx)?;
            rebuild_snapshot_file_url_fts(&tx)?;
            ensure_api_key_filter_setting_column(&tx)?;
            tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
                .context("set PRAGMA user_version")?;
        }
        1 => {
            if legacy_prerelease_schema_detected(&tx)? {
                bail!(
                    "database at the current user_version uses an incompatible prerelease schema; move it aside and run `clipmem setup` to initialize a fresh archive"
                );
            }
            rebuild_snapshot_stats(&tx)?;
            rebuild_snapshot_projection_cache(&tx)?;
            rebuild_snapshot_event_filter_cache(&tx)?;
            rebuild_snapshot_literal_cache(&tx)?;
            rebuild_snapshot_file_url_fts(&tx)?;
            ensure_api_key_filter_setting_column(&tx)?;
            tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
                .context("set PRAGMA user_version")?;
        }
        2 => {
            if legacy_prerelease_schema_detected(&tx)? {
                bail!(
                    "database at the current user_version uses an incompatible prerelease schema; move it aside and run `clipmem setup` to initialize a fresh archive"
                );
            }
            rebuild_snapshot_projection_cache(&tx)?;
            rebuild_snapshot_event_filter_cache(&tx)?;
            rebuild_snapshot_literal_cache(&tx)?;
            rebuild_snapshot_file_url_fts(&tx)?;
            ensure_api_key_filter_setting_column(&tx)?;
            tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
                .context("set PRAGMA user_version")?;
        }
        3 => {
            if legacy_prerelease_schema_detected(&tx)? {
                bail!(
                    "database at the current user_version uses an incompatible prerelease schema; move it aside and run `clipmem setup` to initialize a fresh archive"
                );
            }
            rebuild_snapshot_event_filter_cache(&tx)?;
            rebuild_snapshot_literal_cache(&tx)?;
            rebuild_snapshot_file_url_fts(&tx)?;
            ensure_api_key_filter_setting_column(&tx)?;
            tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
                .context("set PRAGMA user_version")?;
        }
        4 => {
            if legacy_prerelease_schema_detected(&tx)? {
                bail!(
                    "database at the current user_version uses an incompatible prerelease schema; move it aside and run `clipmem setup` to initialize a fresh archive"
                );
            }
            rebuild_snapshot_literal_cache(&tx)?;
            rebuild_snapshot_file_url_fts(&tx)?;
            ensure_api_key_filter_setting_column(&tx)?;
            tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
                .context("set PRAGMA user_version")?;
        }
        5 => {
            if legacy_prerelease_schema_detected(&tx)? {
                bail!(
                    "database at the current user_version uses an incompatible prerelease schema; move it aside and run `clipmem setup` to initialize a fresh archive"
                );
            }
            rebuild_snapshot_file_url_fts(&tx)?;
            ensure_api_key_filter_setting_column(&tx)?;
            tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
                .context("set PRAGMA user_version")?;
        }
        6 => {
            if legacy_prerelease_schema_detected(&tx)? {
                bail!(
                    "database at the current user_version uses an incompatible prerelease schema; move it aside and run `clipmem setup` to initialize a fresh archive"
                );
            }
            ensure_api_key_filter_setting_column(&tx)?;
            tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
                .context("set PRAGMA user_version")?;
        }
        7 => {
            if legacy_prerelease_schema_detected(&tx)? {
                bail!(
                    "database at the current user_version uses an incompatible prerelease schema; move it aside and run `clipmem setup` to initialize a fresh archive"
                );
            }
            ensure_api_key_filter_setting_column(&tx)?;
            tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
                .context("set PRAGMA user_version")?;
        }
        CURRENT_SCHEMA_VERSION => {
            if legacy_prerelease_schema_detected(&tx)? {
                bail!(
                    "database at the current user_version uses an incompatible prerelease schema; move it aside and run `clipmem setup` to initialize a fresh archive"
                );
            }
        }
        version if version > CURRENT_SCHEMA_VERSION => {
            bail!(
                "database schema version {version} is newer than supported version {}",
                CURRENT_SCHEMA_VERSION
            );
        }
        version => {
            bail!("unsupported database schema version {version}");
        }
    }

    tx.execute(
        "INSERT OR IGNORE INTO clipmem_settings (id, paused, retention_seconds, api_key_filter_enabled) VALUES (1, 0, NULL, 0)",
        [],
    )
    .context("seed clipmem settings row")?;

    tx.commit().context("commit schema transaction")?;
    Ok(())
}

fn ensure_api_key_filter_setting_column(conn: &Connection) -> Result<()> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(clipmem_settings)")
        .context("prepare clipmem_settings table info query")?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .context("query clipmem_settings columns")?;
    let columns = collect_rows(rows).context("collect clipmem_settings columns")?;

    if columns
        .iter()
        .any(|column| column == "api_key_filter_enabled")
    {
        return Ok(());
    }

    conn.execute(
        "ALTER TABLE clipmem_settings ADD COLUMN api_key_filter_enabled INTEGER NOT NULL DEFAULT 0 CHECK (api_key_filter_enabled IN (0, 1))",
        [],
    )
    .context("add api_key_filter_enabled column")?;
    Ok(())
}

fn rebuild_snapshot_stats(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM snapshot_stats", [])
        .context("clear snapshot stats")?;
    conn.execute_batch(
        r"
        INSERT INTO snapshot_stats (
            snapshot_id,
            capture_count,
            first_observed_at,
            last_observed_at,
            last_event_id,
            last_frontmost_app_bundle_id,
            last_frontmost_app_name
        )
        SELECT
            ce.snapshot_id,
            COUNT(*) AS capture_count,
            MIN(ce.observed_at) AS first_observed_at,
            MAX(ce.observed_at) AS last_observed_at,
            (
                SELECT latest.id
                FROM capture_events latest
                WHERE latest.snapshot_id = ce.snapshot_id
                ORDER BY latest.observed_at DESC, latest.id DESC
                LIMIT 1
            ) AS last_event_id,
            (
                SELECT latest.frontmost_app_bundle_id
                FROM capture_events latest
                WHERE latest.snapshot_id = ce.snapshot_id
                ORDER BY latest.observed_at DESC, latest.id DESC
                LIMIT 1
            ) AS last_frontmost_app_bundle_id,
            (
                SELECT latest.frontmost_app_name
                FROM capture_events latest
                WHERE latest.snapshot_id = ce.snapshot_id
                ORDER BY latest.observed_at DESC, latest.id DESC
                LIMIT 1
            ) AS last_frontmost_app_name
        FROM capture_events ce
        GROUP BY ce.snapshot_id;
        ",
    )
    .context("rebuild snapshot stats")?;
    Ok(())
}

fn rebuild_snapshot_projection_cache(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM snapshot_projection_cache", [])
        .context("clear snapshot projection cache")?;
    conn.execute_batch(
        r"
        WITH url_values AS (
            SELECT
                snapshot_id,
                GROUP_CONCAT(text_value, char(31)) AS urls
            FROM (
                SELECT DISTINCT snapshot_id, text_value
                FROM item_representations
                WHERE kind = 'url' AND text_value IS NOT NULL AND text_value != ''
                ORDER BY text_value
            )
            GROUP BY snapshot_id
        ),
        file_url_values AS (
            SELECT
                snapshot_id,
                GROUP_CONCAT(text_value, char(31)) AS file_urls
            FROM (
                SELECT DISTINCT snapshot_id, text_value
                FROM item_representations
                WHERE kind = 'file_url' AND text_value IS NOT NULL AND text_value != ''
                ORDER BY text_value
            )
            GROUP BY snapshot_id
        )
        INSERT INTO snapshot_projection_cache (snapshot_id, urls, file_urls)
        SELECT
            s.id,
            COALESCE(uv.urls, ''),
            COALESCE(fv.file_urls, '')
        FROM snapshots s
        LEFT JOIN url_values uv ON uv.snapshot_id = s.id
        LEFT JOIN file_url_values fv ON fv.snapshot_id = s.id;
        ",
    )
    .context("rebuild snapshot projection cache")?;
    Ok(())
}

fn rebuild_snapshot_event_filter_cache(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM snapshot_event_filter_cache", [])
        .context("clear snapshot event filter cache")?;
    conn.execute_batch(
        r"
        INSERT INTO snapshot_event_filter_cache (snapshot_id, app_names_lower, bundle_ids_lower)
        SELECT
            s.id,
            COALESCE((
                SELECT GROUP_CONCAT(app_name, char(31))
                FROM (
                    SELECT DISTINCT lower(ce.frontmost_app_name) AS app_name
                    FROM capture_events ce
                    WHERE ce.snapshot_id = s.id
                      AND ce.frontmost_app_name IS NOT NULL
                      AND ce.frontmost_app_name != ''
                    ORDER BY app_name
                )
            ), '') AS app_names_lower,
            COALESCE((
                SELECT GROUP_CONCAT(bundle_id, char(31))
                FROM (
                    SELECT DISTINCT lower(ce.frontmost_app_bundle_id) AS bundle_id
                    FROM capture_events ce
                    WHERE ce.snapshot_id = s.id
                      AND ce.frontmost_app_bundle_id IS NOT NULL
                      AND ce.frontmost_app_bundle_id != ''
                    ORDER BY bundle_id
                )
            ), '') AS bundle_ids_lower
        FROM snapshots s;
        ",
    )
    .context("rebuild snapshot event filter cache")?;
    Ok(())
}

fn rebuild_snapshot_literal_cache(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM snapshot_literal_cache", [])
        .context("clear snapshot literal cache")?;
    conn.execute("DELETE FROM snapshots_literal_fts", [])
        .context("clear literal FTS cache")?;
    conn.execute_batch(
        r"
        INSERT INTO snapshot_literal_cache (snapshot_id, haystack)
        SELECT
            s.id,
            lower(
                COALESCE(NULLIF(s.preview_text, ''), s.search_text, '') || char(31) ||
                COALESCE(s.preview_text, '') || char(31) ||
                COALESCE(s.search_text, '') || char(31) ||
                COALESCE(sp.urls, '') || char(31) ||
                COALESCE(sp.file_urls, '') || char(31) ||
                COALESCE(ss.last_frontmost_app_name, '') || char(31) ||
                COALESCE(ss.last_frontmost_app_bundle_id, '')
            )
        FROM snapshots s
        LEFT JOIN snapshot_projection_cache sp ON sp.snapshot_id = s.id
        LEFT JOIN snapshot_stats ss ON ss.snapshot_id = s.id;
        ",
    )
    .context("rebuild snapshot literal cache")?;
    Ok(())
}

fn rebuild_snapshot_file_url_fts(conn: &Connection) -> Result<()> {
    conn.execute(
        "INSERT INTO snapshot_file_url_fts(snapshot_file_url_fts) VALUES ('rebuild')",
        [],
    )
    .context("rebuild snapshot file-url FTS")?;
    Ok(())
}

fn legacy_prerelease_schema_detected(conn: &Connection) -> Result<bool> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(item_representations)")
        .context("prepare PRAGMA table_info(item_representations)")?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .context("read item_representations columns")?;
    let columns = collect_rows(rows).context("collect item_representations columns")?;
    if columns.is_empty() {
        return Ok(false);
    }

    let has_kind = columns.iter().any(|column| column == "kind");
    let has_legacy_marker = LEGACY_PRERELEASE_COLUMNS
        .iter()
        .any(|legacy| columns.iter().any(|column| column == legacy));
    Ok(!has_kind || has_legacy_marker)
}

#[cfg(test)]
pub(crate) fn explain_query_plan(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Vec<String>> {
    let explain = format!("EXPLAIN QUERY PLAN {sql}");
    let mut stmt = conn
        .prepare(&explain)
        .context("prepare EXPLAIN QUERY PLAN")?;
    let rows = stmt
        .query_map(params, |row| row.get::<_, String>(3))
        .context("execute EXPLAIN QUERY PLAN")?;
    collect_rows(rows).context("collect EXPLAIN QUERY PLAN rows")
}
