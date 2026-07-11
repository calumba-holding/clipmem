use anyhow::Result;

use crate::db::{Database, OcrRunReport};

#[cfg(target_os = "macos")]
mod macos;

pub(crate) trait OcrEngine {
    fn engine_name(&self) -> &'static str;
    fn recognition_level(&self) -> &'static str;
    fn recognize_text(&self, image_bytes: &[u8]) -> Result<String>;
}

#[cfg(target_os = "macos")]
pub(crate) fn default_engine() -> macos::VisionOcrEngine {
    macos::VisionOcrEngine
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn default_engine() -> UnsupportedOcrEngine {
    UnsupportedOcrEngine
}

#[cfg(not(target_os = "macos"))]
pub(crate) struct UnsupportedOcrEngine;

#[cfg(not(target_os = "macos"))]
impl OcrEngine for UnsupportedOcrEngine {
    fn engine_name(&self) -> &'static str {
        "unsupported"
    }

    fn recognition_level(&self) -> &'static str {
        "fast"
    }

    fn recognize_text(&self, _image_bytes: &[u8]) -> Result<String> {
        anyhow::bail!("image OCR is only supported on macOS")
    }
}

pub(crate) fn run_ocr_jobs(
    db: &mut Database,
    engine: &dyn OcrEngine,
    limit: usize,
    snapshot_id: Option<i64>,
    retry_failed: bool,
) -> Result<OcrRunReport> {
    let candidates = db.next_ocr_candidates(limit, snapshot_id, retry_failed)?;
    let mut ready = 0;
    let mut failed = 0;
    let mut skipped = 0;

    for candidate in &candidates {
        match engine.recognize_text(candidate.blob_value()) {
            Ok(text) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    skipped += 1;
                } else {
                    ready += 1;
                }
                db.store_ocr_candidate_text(
                    candidate,
                    engine.engine_name(),
                    engine.recognition_level(),
                    trimmed,
                )?;
            }
            Err(error) => {
                failed += 1;
                db.store_ocr_candidate_failure(
                    candidate,
                    engine.engine_name(),
                    engine.recognition_level(),
                    &error.to_string(),
                )?;
            }
        }
    }

    let remaining_pending = db.ocr_status_report()?.pending();
    Ok(OcrRunReport::new(
        candidates.len(),
        ready,
        failed,
        skipped,
        remaining_pending,
    ))
}

#[cfg(test)]
mod tests {
    use super::{run_ocr_jobs, OcrEngine};
    use crate::db::Database;
    use crate::model::{build_item, build_representation, build_snapshot, CaptureContext};

    struct FakeOcrEngine;

    impl OcrEngine for FakeOcrEngine {
        fn engine_name(&self) -> &'static str {
            "fake"
        }

        fn recognition_level(&self) -> &'static str {
            "fast"
        }

        fn recognize_text(&self, _image_bytes: &[u8]) -> anyhow::Result<String> {
            Ok("hello from image".to_string())
        }
    }

    struct FailingOcrEngine;

    impl OcrEngine for FailingOcrEngine {
        fn engine_name(&self) -> &'static str {
            "fake"
        }

        fn recognition_level(&self) -> &'static str {
            "fast"
        }

        fn recognize_text(&self, _image_bytes: &[u8]) -> anyhow::Result<String> {
            anyhow::bail!("forced OCR failure")
        }
    }

    fn image_snapshot(change_count: i64, bytes: Vec<u8>) -> crate::model::ClipboardSnapshot {
        build_snapshot(
            CaptureContext::new(change_count),
            vec![build_item(
                0,
                vec![build_representation("public.png".to_string(), None, bytes)],
            )],
        )
    }

    #[test]
    fn run_ocr_jobs_processes_pending_images() -> anyhow::Result<()> {
        let mut db = Database::open_in_memory()?;
        let snapshot = image_snapshot(1, vec![0x89, b'P', b'N', b'G']);
        let stored = db.store_capture(&snapshot)?;
        db.enqueue_ocr_for_snapshot(stored.snapshot_id())?;

        let report = run_ocr_jobs(&mut db, &FakeOcrEngine, 25, None, false)?;

        assert_eq!(report.processed(), 1);
        assert_eq!(report.ready(), 1);
        assert_eq!(db.ocr_status_report()?.snapshots_with_ocr_text(), 1);
        Ok(())
    }

    #[test]
    fn run_ocr_jobs_respects_limit_snapshot_and_retry_failed() -> anyhow::Result<()> {
        let mut db = Database::open_in_memory()?;
        let first = db.store_capture(&image_snapshot(1, b"first-image".to_vec()))?;
        let second = db.store_capture(&image_snapshot(2, b"second-image".to_vec()))?;

        let second_only = run_ocr_jobs(
            &mut db,
            &FakeOcrEngine,
            25,
            Some(second.snapshot_id()),
            false,
        )?;
        assert_eq!(second_only.processed(), 1);
        assert_eq!(second_only.ready(), 1);
        assert_eq!(second_only.remaining_pending(), 0);

        let limited = run_ocr_jobs(&mut db, &FailingOcrEngine, 1, None, false)?;
        assert_eq!(limited.processed(), 1);
        assert_eq!(limited.failed(), 1);
        assert_eq!(limited.remaining_pending(), 0);

        let without_retry = run_ocr_jobs(&mut db, &FakeOcrEngine, 25, None, false)?;
        assert_eq!(without_retry.processed(), 0);

        let retried = run_ocr_jobs(&mut db, &FakeOcrEngine, 25, Some(first.snapshot_id()), true)?;
        assert_eq!(retried.processed(), 1);
        assert_eq!(retried.ready(), 1);
        assert_eq!(db.ocr_status_report()?.failed(), 0);
        Ok(())
    }

    #[test]
    fn scoped_ocr_claim_does_not_consume_unrelated_jobs_or_miss_target() -> anyhow::Result<()> {
        let mut db = Database::open_in_memory()?;
        let unrelated = db.store_capture(&image_snapshot(1, b"unrelated-image".to_vec()))?;
        let target = db.store_capture(&image_snapshot(2, b"target-image".to_vec()))?;
        db.enqueue_ocr_for_snapshot(unrelated.snapshot_id())?;
        db.enqueue_ocr_for_snapshot(target.snapshot_id())?;

        let target_hash: String = db.conn.query_row(
            "SELECT raw_sha256 FROM item_representations WHERE snapshot_id = ?1",
            [target.snapshot_id()],
            |row| row.get(0),
        )?;
        let unrelated_hash: String = db.conn.query_row(
            "SELECT raw_sha256 FROM item_representations WHERE snapshot_id = ?1",
            [unrelated.snapshot_id()],
            |row| row.get(0),
        )?;

        let candidates = db.next_ocr_candidates(1, Some(target.snapshot_id()), false)?;
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].raw_sha256(), target_hash);
        let target_attempts: i64 = db.conn.query_row(
            "SELECT attempts FROM jobs WHERE kind = 'ocr' AND dedupe_key = ?1",
            [&target_hash],
            |row| row.get(0),
        )?;
        let unrelated_attempts: i64 = db.conn.query_row(
            "SELECT attempts FROM jobs WHERE kind = 'ocr' AND dedupe_key = ?1",
            [&unrelated_hash],
            |row| row.get(0),
        )?;
        assert_eq!(target_attempts, 1);
        assert_eq!(unrelated_attempts, 0);
        Ok(())
    }

    #[test]
    fn clearing_completed_ocr_makes_same_source_claimable_again() -> anyhow::Result<()> {
        let mut db = Database::open_in_memory()?;
        let stored = db.store_capture(&image_snapshot(1, b"repeat-image".to_vec()))?;
        let first = db.next_ocr_candidates(1, Some(stored.snapshot_id()), false)?;
        assert_eq!(first.len(), 1);
        let hash = first[0].raw_sha256().to_string();
        db.store_ocr_candidate_text(&first[0], "fake", "fast", "first result")?;

        assert!(db.clear_ocr_result(&hash)?);
        let second = db.next_ocr_candidates(1, Some(stored.snapshot_id()), false)?;
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].raw_sha256(), hash);
        Ok(())
    }

    #[test]
    fn vanished_ocr_source_becomes_skipped_and_next_run_claims_valid_work() -> anyhow::Result<()> {
        let mut db = Database::open_in_memory()?;
        db.conn.execute(
            "INSERT INTO jobs (kind, dedupe_key, algorithm_version, source_ref_json) VALUES ('ocr', 'orphan-hash', 'vision-v1', '{}')",
            [],
        )?;
        let stored = db.store_capture(&image_snapshot(1, b"valid-after-orphan".to_vec()))?;
        db.enqueue_ocr_for_snapshot(stored.snapshot_id())?;

        let first = db.next_ocr_candidates(1, None, false)?;
        assert!(first.is_empty());
        let orphan_state: String = db.conn.query_row(
            "SELECT state FROM jobs WHERE kind = 'ocr' AND dedupe_key = 'orphan-hash'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(orphan_state, "skipped");

        let second = db.next_ocr_candidates(1, None, false)?;
        assert_eq!(second.len(), 1);
        Ok(())
    }

    #[test]
    fn purge_removing_last_ocr_source_deletes_its_durable_job() -> anyhow::Result<()> {
        let mut db = Database::open_in_memory()?;
        let stored = db.store_capture(&image_snapshot(1, b"purged-image".to_vec()))?;
        db.enqueue_ocr_for_snapshot(stored.snapshot_id())?;
        db.conn.execute(
            "UPDATE snapshot_stats SET last_observed_at = datetime('now', '-1 day') WHERE snapshot_id = ?1",
            [stored.snapshot_id()],
        )?;

        let report = db.purge_snapshots_older_than(60, false)?;
        assert_eq!(report.snapshot_count(), 1);
        let remaining: i64 =
            db.conn
                .query_row("SELECT COUNT(*) FROM jobs WHERE kind = 'ocr'", [], |row| {
                    row.get(0)
                })?;
        assert_eq!(remaining, 0);
        Ok(())
    }
}
