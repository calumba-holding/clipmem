use super::event_filter_clause;

pub(super) fn matching_events_cte(include_matching_events: bool) -> String {
    if include_matching_events {
        format!(
            "WITH matching_events AS (
                 SELECT DISTINCT ce.snapshot_id
                 FROM capture_events ce
                 WHERE {}
             )",
            event_filter_clause("ce")
        )
    } else {
        String::new()
    }
}

pub(super) fn matching_events_join(include_matching_events: bool) -> &'static str {
    if include_matching_events {
        "JOIN matching_events me ON me.snapshot_id = s.id"
    } else {
        ""
    }
}
