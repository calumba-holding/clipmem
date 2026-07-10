use crate::db::types::TimelineCursorState;

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
