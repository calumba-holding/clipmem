use anyhow::Result;

use crate::db::Database;
use crate::model::{CaptureStoreResult, ClipboardSnapshot};

#[derive(Debug, Clone)]
pub struct WatchState {
    last_handled_change_count: Option<i64>,
    first_loop: bool,
}

impl WatchState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            last_handled_change_count: None,
            first_loop: true,
        }
    }
}

impl Default for WatchState {
    fn default() -> Self {
        Self::new()
    }
}

/// Update watch state for one captured snapshot and persist it when needed.
///
/// # Errors
///
/// Returns an error if storing the snapshot or its capture event fails.
pub fn process_watch_snapshot(
    db: &mut Database,
    snapshot: &ClipboardSnapshot,
    skip_initial: bool,
    state: &mut WatchState,
) -> Result<Option<CaptureStoreResult>> {
    if state.first_loop && skip_initial {
        state.last_handled_change_count = Some(snapshot.change_count);
        state.first_loop = false;
        return Ok(None);
    }
    state.first_loop = false;

    if state.last_handled_change_count == Some(snapshot.change_count) {
        return Ok(None);
    }

    let result = db.store_capture(snapshot)?;
    state.last_handled_change_count = Some(snapshot.change_count);
    Ok(Some(result))
}

#[must_use]
pub fn format_watch_capture_line(
    snapshot: &ClipboardSnapshot,
    result: &CaptureStoreResult,
) -> String {
    let source = snapshot
        .frontmost_app_name
        .as_deref()
        .or(snapshot.frontmost_app_bundle_id.as_deref())
        .unwrap_or("unknown app");

    format!(
        "event={} snapshot={} {} kind={} bytes={} source={} preview={}",
        result.event_id,
        result.snapshot_id,
        if result.inserted_new_snapshot {
            "new"
        } else {
            "seen"
        },
        snapshot.snapshot_kind,
        snapshot.total_bytes,
        source,
        snapshot.preview_text
    )
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::{format_watch_capture_line, process_watch_snapshot, WatchState};
    use crate::clipboard::{build_item, build_representation, build_snapshot};
    use crate::db::Database;
    use crate::model::{CaptureContext, CaptureStoreResult};

    #[test]
    fn skip_initial_marks_first_change_as_seen_without_storing() -> Result<()> {
        let mut db = Database::open_in_memory()?;
        let mut state = WatchState::new();
        let snapshot = build_snapshot(
            CaptureContext::new(7).with_frontmost_app_name("Editor"),
            Vec::new(),
        );

        let result = process_watch_snapshot(&mut db, &snapshot, true, &mut state)?;

        assert!(result.is_none());
        assert!(db.recent(10, None)?.is_empty());
        Ok(())
    }

    #[test]
    fn repeated_change_counts_are_ignored_after_the_first_store() -> Result<()> {
        let mut db = Database::open_in_memory()?;
        let mut state = WatchState::new();
        let snapshot = build_snapshot(
            CaptureContext::new(7).with_frontmost_app_name("Editor"),
            Vec::new(),
        );

        let first = process_watch_snapshot(&mut db, &snapshot, false, &mut state)?;
        let second = process_watch_snapshot(&mut db, &snapshot, false, &mut state)?;

        assert!(first.is_some());
        assert!(second.is_none());
        assert_eq!(db.recent(10, None)?.len(), 1);
        Ok(())
    }

    #[test]
    fn changed_change_counts_store_new_events_for_existing_content() -> Result<()> {
        let mut db = Database::open_in_memory()?;
        let mut state = WatchState::new();
        let first = build_snapshot(
            CaptureContext::new(7).with_frontmost_app_name("Editor"),
            Vec::new(),
        );
        let second = build_snapshot(
            CaptureContext::new(8).with_frontmost_app_name("Editor"),
            Vec::new(),
        );

        let first_store = anyhow::Context::context(
            process_watch_snapshot(&mut db, &first, false, &mut state)?,
            "expected first watch snapshot to store",
        )?;
        let second_store = anyhow::Context::context(
            process_watch_snapshot(&mut db, &second, false, &mut state)?,
            "expected second watch snapshot to store",
        )?;
        let details = db
            .find_snapshot(first_store.snapshot_id, 10)?
            .ok_or_else(|| anyhow::anyhow!("expected stored snapshot details"))?;

        assert_eq!(first_store.snapshot_id, second_store.snapshot_id);
        assert!(!second_store.inserted_new_snapshot);
        assert_eq!(details.capture_count, 2);
        Ok(())
    }

    #[test]
    fn watch_log_line_includes_capture_context() {
        let snapshot = build_snapshot(
            CaptureContext::new(3)
                .with_frontmost_app_name("Editor")
                .with_frontmost_app_bundle_id("com.example.Editor"),
            vec![build_item(
                0,
                vec![build_representation(
                    "public.utf8-plain-text".to_string(),
                    None,
                    b"hello world".to_vec(),
                )],
            )],
        );
        let result = CaptureStoreResult {
            snapshot_id: 42,
            event_id: 7,
            inserted_new_snapshot: true,
        };

        let line = format_watch_capture_line(&snapshot, &result);

        assert!(line.contains("event=7"));
        assert!(line.contains("snapshot=42"));
        assert!(line.contains("new"));
        assert!(line.contains("source=Editor"));
        assert!(line.contains("preview=hello world"));
    }
}
