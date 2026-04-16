mod read;
mod schema;
mod store;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result};
use rusqlite::{types::Type, Connection, OpenFlags, Row};

use self::schema::{configure_connection, SCHEMA};

pub struct Database {
    pub(super) conn: Connection,
    pub(super) path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Auto,
    Fts,
    Literal,
}

impl Database {
    /// Open the archive database at `path`, creating parent directories and schema state as needed.
    ///
    /// # Errors
    ///
    /// Returns an error if the parent directory cannot be created, the database cannot be opened,
    /// or the connection cannot be configured and bootstrapped.
    pub fn open_or_init(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let conn = open_connection(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        bootstrap_connection(&conn)?;

        Ok(Self {
            conn,
            path: path.to_path_buf(),
        })
    }

    /// Open an existing archive database without creating parent directories or schema state.
    ///
    /// # Errors
    ///
    /// Returns an error if the database file does not already exist, cannot be opened, or the
    /// connection cannot be configured.
    pub fn open_existing(path: &Path) -> Result<Self> {
        let conn = open_connection(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        configure_connection(&conn)?;

        Ok(Self {
            conn,
            path: path.to_path_buf(),
        })
    }

    #[cfg(test)]
    pub(crate) fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        bootstrap_connection(&conn)?;
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
    i64::try_from(value).context("value exceeds SQLite INTEGER range")
}

pub(super) fn row_usize(row: &Row<'_>, index: usize) -> rusqlite::Result<usize> {
    let value = row.get::<_, i64>(index)?;
    usize::try_from(value).map_err(|source| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Integer, Box::new(source))
    })
}

pub(super) fn row_enum<T>(row: &Row<'_>, index: usize) -> rusqlite::Result<T>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    let value = row.get::<_, String>(index)?;
    value.parse().map_err(|source| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(source))
    })
}

fn open_connection(path: &Path, flags: OpenFlags) -> Result<Connection> {
    Connection::open_with_flags(path, flags)
        .with_context(|| format!("failed to open {}", path.display()))
}

fn bootstrap_connection(conn: &Connection) -> Result<()> {
    configure_connection(conn)?;
    conn.execute_batch(SCHEMA)?;
    Ok(())
}
