use std::io::Cursor;
use std::path::Path;

use anyhow::{ensure, Context, Result};
use image::{ExtendedColorType, ImageEncoder, ImageReader, Limits};
use rusqlite::{params, OptionalExtension, TransactionBehavior};

use crate::model::{
    hash_bytes, truncate_chars, CaptureStoreResult, ClipboardItem, ClipboardSnapshot,
};
use crate::sensitive;

use super::{
    collect_rows, pragma_usize, row_usize, sanitise_limit, storage_file_sizes, usize_to_i64,
    CapturePolicy, CaptureSettings, CaptureSkipReason, CaptureStoreOutcome, Database,
    ImageOptimizationReport, OcrCandidate, OcrStatusReport, PurgeReport, SnapshotDeletionReport,
};

mod capture_and_settings;
mod ocr;
mod optimize;
mod rebuild;

pub(super) use self::capture_and_settings::*;
pub(super) use self::ocr::*;
pub(super) use self::optimize::*;
pub(super) use self::rebuild::*;

#[cfg(test)]
mod tests;
