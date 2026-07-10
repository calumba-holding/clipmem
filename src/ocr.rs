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
}
