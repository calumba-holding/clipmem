use anyhow::{Context, Result};

use crate::db::sqlite_helpers::collect_rows;
use crate::db::types::{CapturePolicy, CaptureSettings, Database};

impl Database {
    pub(crate) fn capture_settings(&self) -> Result<CaptureSettings> {
        self.conn
            .query_row(
                "SELECT paused, retention_seconds, api_key_filter_enabled, ocr_enabled FROM clipmem_settings WHERE id = 1",
                [],
                |row| {
                    let paused = row.get::<_, i64>(0)? != 0;
                    let retention_seconds = row
                        .get::<_, Option<i64>>(1)?
                        .map(u64::try_from)
                        .transpose()
                        .map_err(|source| {
                            rusqlite::Error::FromSqlConversionFailure(
                                1,
                                rusqlite::types::Type::Integer,
                                Box::new(source),
                            )
                        })?;
                    let api_key_filter_enabled = row.get::<_, i64>(2)? != 0;
                    let ocr_enabled = row.get::<_, i64>(3)? != 0;
                    Ok(CaptureSettings::new(
                        paused,
                        retention_seconds,
                        api_key_filter_enabled,
                        ocr_enabled,
                    ))
                },
            )
            .context("load capture settings")
    }

    pub(crate) fn capture_policy(&self) -> Result<CapturePolicy> {
        Ok(CapturePolicy::new(
            self.capture_settings()?,
            self.list_ignored_bundle_ids()?,
        ))
    }

    pub(crate) fn set_paused(&self, paused: bool) -> Result<CaptureSettings> {
        self.conn
            .execute(
                "UPDATE clipmem_settings SET paused = ?1 WHERE id = 1",
                [if paused { 1_i64 } else { 0_i64 }],
            )
            .context("update paused setting")?;
        self.capture_settings()
    }

    pub(crate) fn set_retention_seconds(
        &self,
        retention_seconds: Option<u64>,
    ) -> Result<CaptureSettings> {
        let retention_seconds = retention_seconds
            .map(i64::try_from)
            .transpose()
            .context("retention exceeds SQLite INTEGER range")?;
        self.conn
            .execute(
                "UPDATE clipmem_settings SET retention_seconds = ?1 WHERE id = 1",
                [retention_seconds],
            )
            .context("update retention setting")?;
        self.capture_settings()
    }

    pub(crate) fn set_api_key_filter_enabled(&self, enabled: bool) -> Result<CaptureSettings> {
        self.conn
            .execute(
                "UPDATE clipmem_settings SET api_key_filter_enabled = ?1 WHERE id = 1",
                [if enabled { 1_i64 } else { 0_i64 }],
            )
            .context("update api key filter setting")?;
        self.capture_settings()
    }

    pub(crate) fn set_ocr_enabled(&self, enabled: bool) -> Result<CaptureSettings> {
        self.conn
            .execute(
                "UPDATE clipmem_settings SET ocr_enabled = ?1 WHERE id = 1",
                [if enabled { 1_i64 } else { 0_i64 }],
            )
            .context("update ocr setting")?;
        self.capture_settings()
    }

    pub(crate) fn list_ignored_bundle_ids(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT bundle_id FROM ignored_bundle_ids ORDER BY bundle_id ASC")
            .context("prepare ignored bundle-id query")?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .context("execute ignored bundle-id query")?;
        collect_rows(rows).context("collect ignored bundle ids")
    }

    pub(crate) fn add_ignored_bundle_id(&self, bundle_id: &str) -> Result<bool> {
        let bundle_id = normalize_bundle_id(bundle_id)?;
        let changed = self
            .conn
            .execute(
                "INSERT OR IGNORE INTO ignored_bundle_ids (bundle_id) VALUES (?1)",
                [bundle_id],
            )
            .context("insert ignored bundle id")?;
        Ok(changed != 0)
    }

    pub(crate) fn remove_ignored_bundle_id(&self, bundle_id: &str) -> Result<bool> {
        let bundle_id = normalize_bundle_id(bundle_id)?;
        let changed = self
            .conn
            .execute(
                "DELETE FROM ignored_bundle_ids WHERE bundle_id = ?1",
                [bundle_id],
            )
            .context("delete ignored bundle id")?;
        Ok(changed != 0)
    }

    pub(crate) fn bundle_id_is_ignored(&self, bundle_id: Option<&str>) -> Result<bool> {
        let Some(bundle_id) = bundle_id else {
            return Ok(false);
        };
        let bundle_id = normalize_bundle_id(bundle_id)?;
        self.conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM ignored_bundle_ids WHERE bundle_id = ?1)",
                [bundle_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|value| value != 0)
            .context("check ignored bundle id")
    }
}

fn normalize_bundle_id(bundle_id: &str) -> Result<String> {
    let normalized = bundle_id.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        anyhow::bail!("bundle id cannot be empty");
    }
    Ok(normalized)
}
