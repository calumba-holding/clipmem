#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod unsupported;

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RestorePlanRepresentation {
    uti: String,
    bytes: Vec<u8>,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RestorePlanItem {
    item_index: usize,
    representations: Vec<RestorePlanRepresentation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RestoreReport {
    item_count: usize,
    representation_count: usize,
    total_bytes: usize,
    change_count: i64,
    rollback_available: bool,
}

#[derive(Debug)]
#[cfg(any(target_os = "macos", test))]
pub(crate) struct RestoreWriteFailure {
    write_error: anyhow::Error,
    rollback_attempted: bool,
    rollback_succeeded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(any(target_os = "macos", test))]
pub(crate) enum RestoreWritePhase {
    Destination,
    Rollback,
}

#[cfg(any(target_os = "macos", test))]
impl std::fmt::Display for RestoreWriteFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "restore write failed: {}; rollback_attempted={}; rollback_succeeded={}",
            self.write_error, self.rollback_attempted, self.rollback_succeeded
        )
    }
}

#[cfg(any(target_os = "macos", test))]
impl std::error::Error for RestoreWriteFailure {}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn execute_restore_protocol<T, Prepare, Rollback, CanRollback, Write>(
    prepare: Prepare,
    capture_rollback: Rollback,
    can_rollback: CanRollback,
    mut write: Write,
) -> anyhow::Result<(i64, bool)>
where
    Prepare: FnOnce() -> anyhow::Result<T>,
    Rollback: FnOnce() -> anyhow::Result<Option<T>>,
    CanRollback: Fn() -> bool,
    Write: FnMut(&T, RestoreWritePhase) -> anyhow::Result<i64>,
{
    let prepared = prepare()?;
    let rollback = capture_rollback().unwrap_or(None);
    match write(&prepared, RestoreWritePhase::Destination) {
        Ok(change_count) => Ok((change_count, rollback.is_some())),
        Err(write_error) => {
            let rollback_attempted = rollback.is_some() && can_rollback();
            let rollback_succeeded = rollback_attempted
                && rollback
                    .as_ref()
                    .is_some_and(|plan| write(plan, RestoreWritePhase::Rollback).is_ok());
            Err(RestoreWriteFailure {
                write_error,
                rollback_attempted,
                rollback_succeeded,
            }
            .into())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(any(target_os = "macos", test))]
pub(crate) struct TransientPasteboardChanged;

#[cfg(any(target_os = "macos", test))]
impl std::fmt::Display for TransientPasteboardChanged {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("clipboard changed during capture; retry the capture")
    }
}

#[cfg(any(target_os = "macos", test))]
impl std::error::Error for TransientPasteboardChanged {}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn stable_capture_with<F>(
    mut attempt: F,
) -> anyhow::Result<crate::model::ClipboardSnapshot>
where
    F: FnMut() -> anyhow::Result<(i64, crate::model::ClipboardSnapshot, i64)>,
{
    for _ in 0..3 {
        let (before, snapshot, after) = attempt()?;
        if before == after {
            return Ok(snapshot);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    Err(TransientPasteboardChanged.into())
}

#[cfg(target_os = "macos")]
pub use self::macos::capture_snapshot;
#[cfg(target_os = "macos")]
pub use self::macos::current_change_count;
#[cfg(target_os = "macos")]
#[cfg_attr(not(test), allow(unused_imports))]
pub(crate) use self::macos::{build_restore_plan, restore_items_registered};
#[cfg(not(target_os = "macos"))]
pub use self::unsupported::capture_snapshot;
#[cfg(not(target_os = "macos"))]
pub use self::unsupported::current_change_count;
#[cfg(not(target_os = "macos"))]
pub(crate) use self::unsupported::restore_items_registered;

#[cfg(target_os = "macos")]
impl RestorePlanRepresentation {
    #[must_use]
    pub(crate) fn new(uti: String, bytes: Vec<u8>) -> Self {
        Self { uti, bytes }
    }

    #[must_use]
    pub(crate) fn uti(&self) -> &str {
        &self.uti
    }

    #[must_use]
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[cfg(target_os = "macos")]
impl RestorePlanItem {
    #[must_use]
    pub(crate) fn new(item_index: usize, representations: Vec<RestorePlanRepresentation>) -> Self {
        Self {
            item_index,
            representations,
        }
    }

    #[must_use]
    pub(crate) fn item_index(&self) -> usize {
        self.item_index
    }

    #[must_use]
    pub(crate) fn representations(&self) -> &[RestorePlanRepresentation] {
        &self.representations
    }
}

impl RestoreReport {
    #[cfg(target_os = "macos")]
    #[must_use]
    pub(crate) fn new(
        item_count: usize,
        representation_count: usize,
        total_bytes: usize,
        change_count: i64,
        rollback_available: bool,
    ) -> Self {
        Self {
            item_count,
            representation_count,
            total_bytes,
            change_count,
            rollback_available,
        }
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
    pub(crate) fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    #[must_use]
    pub(crate) fn change_count(&self) -> i64 {
        self.change_count
    }

    #[must_use]
    pub(crate) fn rollback_available(&self) -> bool {
        self.rollback_available
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{build_snapshot, CaptureContext};

    use super::{
        execute_restore_protocol, stable_capture_with, RestoreWriteFailure,
        TransientPasteboardChanged,
    };

    #[test]
    fn stable_capture_discards_changed_attempt_and_returns_whole_retry() {
        let mut attempt = 0;
        let snapshot = stable_capture_with(|| {
            attempt += 1;
            let generation = if attempt == 1 { (1, 2) } else { (3, 3) };
            Ok((
                generation.0,
                build_snapshot(CaptureContext::new(generation.0), Vec::new()),
                generation.1,
            ))
        })
        .unwrap();
        assert_eq!(snapshot.change_count(), 3);
        assert_eq!(attempt, 2);
    }

    #[test]
    fn stable_capture_exhaustion_is_typed() {
        let error =
            stable_capture_with(|| Ok((1, build_snapshot(CaptureContext::new(1), Vec::new()), 2)))
                .unwrap_err();
        assert!(error.downcast_ref::<TransientPasteboardChanged>().is_some());
    }

    #[test]
    fn restore_preparation_failure_does_not_touch_destination() {
        let writes = std::cell::Cell::new(0);
        let error = execute_restore_protocol::<Vec<u8>, _, _, _, _>(
            || anyhow::bail!("bad representation"),
            || Ok(Some(vec![1])),
            || true,
            |_, _| {
                writes.set(writes.get() + 1);
                Ok(1)
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("bad representation"));
        assert_eq!(writes.get(), 0);
    }

    #[test]
    fn restore_write_failure_attempts_rollback_and_reports_result() {
        let writes = std::cell::Cell::new(0);
        let error = execute_restore_protocol(
            || Ok(vec![2]),
            || Ok(Some(vec![1])),
            || true,
            |plan: &Vec<u8>, _| {
                writes.set(writes.get() + 1);
                if plan == &vec![2] {
                    anyhow::bail!("destination write failed");
                }
                Ok(9)
            },
        )
        .unwrap_err();
        let failure = error.downcast_ref::<RestoreWriteFailure>().unwrap();
        assert!(failure.rollback_attempted);
        assert!(failure.rollback_succeeded);
        assert_eq!(writes.get(), 2);
    }

    #[test]
    fn restore_does_not_rollback_over_an_interleaved_external_generation() {
        let writes = std::cell::Cell::new(0);
        let error = execute_restore_protocol(
            || Ok(vec![2]),
            || Ok(Some(vec![1])),
            || false,
            |_plan: &Vec<u8>, _| {
                writes.set(writes.get() + 1);
                anyhow::bail!("generation ownership lost")
            },
        )
        .unwrap_err();
        let failure = error.downcast_ref::<RestoreWriteFailure>().unwrap();
        assert!(!failure.rollback_attempted);
        assert!(!failure.rollback_succeeded);
        assert_eq!(writes.get(), 1);
    }

    #[test]
    fn capture_snapshot_entrypoint_is_exposed() {
        let capture: fn() -> anyhow::Result<crate::model::ClipboardSnapshot> =
            super::capture_snapshot;

        let _ = capture;
    }

    #[test]
    fn current_change_count_entrypoint_is_exposed() {
        let current_change_count: fn() -> anyhow::Result<i64> = super::current_change_count;

        let _ = current_change_count;
    }

    #[test]
    fn restore_entrypoints_are_exposed() {
        #[cfg(target_os = "macos")]
        {
            let build_restore_plan: fn(
                &[crate::model::ClipboardItem],
            ) -> Vec<super::RestorePlanItem> = super::build_restore_plan;
            let _ = build_restore_plan;
        }
    }
}
