#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod unsupported;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RestorePlanRepresentation {
    uti: String,
    bytes: Vec<u8>,
}

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
}

#[cfg(target_os = "macos")]
pub use self::macos::capture_snapshot;
#[cfg(target_os = "macos")]
pub use self::macos::current_change_count;
#[cfg(target_os = "macos")]
#[cfg_attr(not(test), allow(unused_imports))]
pub(crate) use self::macos::{build_restore_plan, restore_items};
#[cfg(not(target_os = "macos"))]
pub use self::unsupported::capture_snapshot;
#[cfg(not(target_os = "macos"))]
pub use self::unsupported::current_change_count;
#[cfg(not(target_os = "macos"))]
#[cfg_attr(not(test), allow(unused_imports))]
pub(crate) use self::unsupported::{build_restore_plan, restore_items};

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
    #[must_use]
    pub(crate) fn new(item_count: usize, representation_count: usize, total_bytes: usize) -> Self {
        Self {
            item_count,
            representation_count,
            total_bytes,
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
}

#[cfg(test)]
mod tests {
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
        let build_restore_plan: fn(&[crate::model::ClipboardItem]) -> Vec<super::RestorePlanItem> =
            super::build_restore_plan;
        let restore_items: fn(
            &[crate::model::ClipboardItem],
        ) -> anyhow::Result<super::RestoreReport> = super::restore_items;

        let _ = build_restore_plan;
        let _ = restore_items;
    }
}
