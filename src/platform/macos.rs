use anyhow::Result;
use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSWorkspace};

use crate::clipboard::{
    build_item, build_snapshot, classify_uti, decode_text_bytes_lossy, hash_bytes,
};
use crate::model::{ClipboardItem, ClipboardRepresentation, ClipboardSnapshot};

pub fn capture_snapshot() -> Result<ClipboardSnapshot> {
    autoreleasepool(|_| {
        let pasteboard = NSPasteboard::generalPasteboard();
        let change_count = pasteboard.changeCount() as i64;
        let (frontmost_app_name, frontmost_app_bundle_id) = current_frontmost_app();

        let mut items = Vec::new();
        if let Some(pasteboard_items) = pasteboard.pasteboardItems() {
            for idx in 0..pasteboard_items.len() {
                let item = pasteboard_items.objectAtIndex(idx);
                items.push(read_item(idx, &item));
            }
        }

        Ok(build_snapshot(
            change_count,
            frontmost_app_name,
            frontmost_app_bundle_id,
            items,
        ))
    })
}

fn current_frontmost_app() -> (Option<String>, Option<String>) {
    let workspace = NSWorkspace::sharedWorkspace();
    if let Some(app) = workspace.frontmostApplication() {
        let name = app.localizedName().map(|value| value.to_string());
        let bundle_id = app.bundleIdentifier().map(|value| value.to_string());
        (name, bundle_id)
    } else {
        (None, None)
    }
}

fn read_item(item_index: usize, item: &NSPasteboardItem) -> ClipboardItem {
    let types = item.types();
    let mut representations = Vec::new();

    for idx in 0..types.len() {
        let uti = types.objectAtIndex(idx);
        let uti_string = uti.to_string();

        let string_value = item.stringForType(&uti).map(|value| value.to_string());
        let raw_data = item.dataForType(&uti);
        let raw_bytes =
            representation_bytes(raw_data.map(|data| data.to_vec()), string_value.as_deref());

        representations.push(build_representation(uti_string, string_value, raw_bytes));
    }

    build_item(item_index, representations)
}

fn representation_bytes(raw_data: Option<Vec<u8>>, string_value: Option<&str>) -> Vec<u8> {
    match (raw_data, string_value) {
        (Some(data), _) => data,
        (None, Some(text)) => text.as_bytes().to_vec(),
        (None, None) => Vec::new(),
    }
}

fn build_representation(
    uti: String,
    string_value: Option<String>,
    raw_bytes: Vec<u8>,
) -> ClipboardRepresentation {
    let kind = classify_uti(&uti, string_value.is_some());
    let decoded_text = if let Some(text) = string_value {
        Some(text)
    } else if kind.is_textual() {
        decode_text_bytes_lossy(&raw_bytes)
    } else {
        None
    };

    ClipboardRepresentation::new(uti, kind, hash_bytes(&raw_bytes), decoded_text, raw_bytes)
}

#[cfg(test)]
mod tests {
    use crate::model::ClipboardKind;

    use super::{build_representation, representation_bytes};

    #[test]
    fn raw_string_bytes_fill_in_when_pasteboard_data_is_missing() {
        let bytes = representation_bytes(None, Some("hello"));

        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn textual_payloads_are_decoded_when_only_raw_bytes_exist() {
        let bytes = "hello"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let representation =
            build_representation("public.utf8-plain-text".to_string(), None, bytes);

        assert_eq!(representation.kind, ClipboardKind::PlainText);
        assert_eq!(representation.text_value.as_deref(), Some("hello"));
        assert!(representation.is_text());
    }

    #[test]
    fn binary_payloads_stay_binary_when_no_string_value_is_available() {
        let representation =
            build_representation("public.png".to_string(), None, vec![0x89, b'P', b'N', b'G']);

        assert_eq!(representation.kind, ClipboardKind::Image);
        assert_eq!(representation.text_value, None);
        assert!(!representation.is_text());
    }
}
