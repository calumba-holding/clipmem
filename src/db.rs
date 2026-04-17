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
use serde::Serialize;

use crate::model::SearchHit;

const SCHEMA: &str = include_str!("db/schema.sql");
const CURRENT_SCHEMA_VERSION: i64 = 1;

pub struct Database {
    pub(super) conn: Connection,
    pub(super) path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ValueEnum)]
pub enum SearchMode {
    Auto,
    Fts,
    Literal,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResults {
    mode_used: SearchMode,
    hits: Vec<SearchHit>,
}

impl SearchResults {
    #[must_use]
    pub(crate) fn new(mode_used: SearchMode, hits: Vec<SearchHit>) -> Self {
        Self { mode_used, hits }
    }

    #[must_use]
    pub fn mode_used(&self) -> SearchMode {
        self.mode_used
    }

    #[must_use]
    pub fn hits(&self) -> &[SearchHit] {
        &self.hits
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
            tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
                .context("set PRAGMA user_version")?;
        }
        CURRENT_SCHEMA_VERSION => {}
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

    tx.commit().context("commit schema transaction")?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn explain_query_plan(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Vec<String>> {
    let explain = format!("EXPLAIN QUERY PLAN {sql}");
    let mut stmt = conn.prepare(&explain).context("prepare EXPLAIN QUERY PLAN")?;
    let rows = stmt
        .query_map(params, |row| row.get::<_, String>(3))
        .context("execute EXPLAIN QUERY PLAN")?;
    collect_rows(rows).context("collect EXPLAIN QUERY PLAN rows")
}
