pub use crate::model::{
    build_item, build_representation, build_snapshot, CaptureContext, ClipboardItem, ClipboardKind,
    ClipboardRepresentation, ClipboardSnapshot, ParseDomainValueError, SnapshotKind,
};

#[cfg(test)]
mod tests {
    use super::{build_item, build_representation, build_snapshot, CaptureContext, ClipboardKind};

    #[test]
    fn capture_facade_builds_normalized_plain_text_snapshots() {
        let snapshot = build_snapshot(
            CaptureContext::new(4)
                .with_frontmost_app_name("Terminal")
                .with_frontmost_app_bundle_id("com.apple.Terminal"),
            vec![build_item(
                0,
                vec![build_representation(
                    "public.utf8-plain-text".to_string(),
                    None,
                    b"git status".to_vec(),
                )],
            )],
        );

        assert_eq!(snapshot.snapshot_kind().as_str(), "plain_text");
        assert_eq!(snapshot.items()[0].primary_kind(), ClipboardKind::PlainText);
        assert_eq!(
            snapshot.items()[0].representations()[0].text_value(),
            Some("git status")
        );
    }
}
