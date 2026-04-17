pub use crate::db::{
    Database, RetrievalFilters, RetrievalKind, SearchMode, SearchResults, TimelineSort,
};
pub use crate::model::{
    CaptureEvent, CaptureStoreResult, DoctorReport, SearchHit, SnapshotDetails, TimelineEvent,
};

#[cfg(test)]
mod tests {
    use super::{CaptureEvent, DoctorReport, SearchHit, SnapshotDetails};
    use crate::model::{build_item, build_representation, SnapshotKind};

    #[test]
    fn archive_dtos_preserve_constructor_data() {
        let mut snapshot = SnapshotDetails::new(
            7,
            "abc123".to_string(),
            SnapshotKind::PlainText,
            "git push".to_string(),
            "git push".to_string(),
            1,
            8,
            "2026-04-16 10:00:00".to_string(),
            2,
            "2026-04-16 10:00:00".to_string(),
            "2026-04-16 11:00:00".to_string(),
            Some("Terminal".to_string()),
            Some("com.apple.Terminal".to_string()),
            Vec::new(),
            Vec::new(),
        );
        snapshot.set_recent_events(vec![CaptureEvent::new(
            21,
            "2026-04-16 11:00:00".to_string(),
            3,
            Some("Terminal".to_string()),
            Some("com.apple.Terminal".to_string()),
        )]);
        snapshot.set_items(vec![build_item(
            0,
            vec![build_representation(
                "public.utf8-plain-text".to_string(),
                None,
                b"git push".to_vec(),
            )],
        )]);
        let hit = SearchHit::new(
            7,
            21,
            "abc123".to_string(),
            SnapshotKind::PlainText,
            "git push".to_string(),
            Some("git push".to_string()),
            2,
            "2026-04-16 10:00:00".to_string(),
            "2026-04-16 11:00:00".to_string(),
            Some("Terminal".to_string()),
            Some("com.apple.Terminal".to_string()),
            Vec::new(),
            Vec::new(),
            8,
            1,
            Some(0.25),
        );
        let report = DoctorReport::new(
            "/tmp/clipmem.sqlite3".to_string(),
            "3.46.0".to_string(),
            "wal".to_string(),
            true,
            true,
            vec!["ENABLE_FTS5".to_string()],
        );

        assert_eq!(snapshot.recent_events()[0].event_id(), 21);
        assert_eq!(snapshot.items()[0].preview_text(), "git push");
        assert_eq!(hit.event_id(), 21);
        assert_eq!(hit.score(), Some(0.25));
        assert!(report.fts5_compile_option_present());
    }
}
