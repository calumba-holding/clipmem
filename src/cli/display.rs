use crate::db::StatsTimeBucketEntry;

pub(in crate::cli) fn format_duration_compact(seconds: u64) -> String {
    let day = 24 * 60 * 60;
    let hour = 60 * 60;
    let minute = 60;

    if seconds.is_multiple_of(day) {
        format!("{}d", seconds / day)
    } else if seconds.is_multiple_of(hour) {
        format!("{}h", seconds / hour)
    } else if seconds.is_multiple_of(minute) {
        format!("{}m", seconds / minute)
    } else {
        format!("{seconds}s")
    }
}

pub(in crate::cli) fn format_duration_seconds(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let remainder = seconds % 60;

    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {remainder}s")
    } else {
        format!("{remainder}s")
    }
}

pub(in crate::cli) fn peak_bucket(
    buckets: &[StatsTimeBucketEntry],
) -> Option<&StatsTimeBucketEntry> {
    buckets
        .iter()
        .max_by_key(|entry| {
            (
                entry.capture_event_count(),
                std::cmp::Reverse(entry.bucket()),
            )
        })
        .filter(|entry| entry.capture_event_count() > 0)
}

#[cfg(test)]
mod tests {
    use super::{format_duration_compact, format_duration_seconds};

    #[test]
    fn format_duration_compact_prefers_days_hours_minutes_then_seconds() {
        assert_eq!(format_duration_compact(172_800), "2d");
        assert_eq!(format_duration_compact(7_200), "2h");
        assert_eq!(format_duration_compact(300), "5m");
        assert_eq!(format_duration_compact(301), "301s");
    }

    #[test]
    fn format_duration_seconds_renders_largest_two_units() {
        assert_eq!(format_duration_seconds(176_400), "2d 1h");
        assert_eq!(format_duration_seconds(7_380), "2h 3m");
        assert_eq!(format_duration_seconds(185), "3m 5s");
        assert_eq!(format_duration_seconds(42), "42s");
    }
}
