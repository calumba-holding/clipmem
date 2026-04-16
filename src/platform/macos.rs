use anyhow::Result;
use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSWorkspace};
use crate::model::{ClipboardItem, ClipboardRepresentation, ClipboardSnapshot};
use crate::util::{
    classify_uti, decode_textish_bytes, dedupe_text_fragments, fingerprint_snapshot, hash_bytes,
    html_to_text_lossy, join_item_previews, joined_search_text, likely_textual_kind,
    pick_primary_representation, preview_for_item, rtf_to_text_lossy, snapshot_kind, total_bytes,
};

pub fn capture_snapshot() -> Result<ClipboardSnapshot> {
    autoreleasepool(|_| {
        let pasteboard = NSPasteboard::generalPasteboard();
        let change_count = pasteboard.changeCount() as i64;
        let (frontmost_app_name, frontmost_app_bundle_id) = current_frontmost_app();

        let mut items = Vec::new();
        if let Some(pasteboard_items) = pasteboard.pasteboardItems() {
            for idx in 0..pasteboard_items.len() {
                let item = pasteboard_items.objectAtIndex(idx);
                items.push(read_item(idx, &item)?);
            }
        }

        if items.is_empty() {
            items.push(ClipboardItem {
                item_index: 0,
                primary_kind: "empty".to_string(),
                primary_uti: None,
                preview_text: "[empty clipboard]".to_string(),
                search_text: String::new(),
                total_bytes: 0,
                representations: Vec::new(),
            });
        }

        let snapshot = ClipboardSnapshot {
            change_count,
            frontmost_app_bundle_id,
            frontmost_app_name,
            fingerprint: fingerprint_snapshot(&items),
            snapshot_kind: snapshot_kind(&items),
            preview_text: join_item_previews(&items),
            search_text: joined_search_text(&items),
            item_count: items.len(),
            total_bytes: total_bytes(&items),
            items,
        };

        Ok(snapshot)
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

fn read_item(item_index: usize, item: &NSPasteboardItem) -> Result<ClipboardItem> {
    let types = item.types();
    let mut representations = Vec::new();

    for idx in 0..types.len() {
        let uti = types.objectAtIndex(idx);
        let uti_string = uti.to_string();

        let string_value = item.stringForType(&uti).map(|value| value.to_string());
        let raw_data = item.dataForType(&uti);
        let raw_bytes = match (raw_data, string_value.as_ref()) {
            (Some(data), _) => data.to_vec(),
            (None, Some(text)) => text.as_bytes().to_vec(),
            (None, None) => Vec::new(),
        };

        let classification = classify_uti(&uti_string, string_value.is_some()).to_string();
        let decoded_text = if let Some(text) = string_value {
            Some(text)
        } else if likely_textual_kind(&classification) {
            decode_textish_bytes(&raw_bytes)
        } else {
            None
        };

        representations.push(ClipboardRepresentation {
            uti: uti_string,
            classification,
            is_text: decoded_text.is_some(),
            byte_len: raw_bytes.len(),
            raw_sha256: hash_bytes(&raw_bytes),
            text_value: decoded_text,
            raw_bytes,
        });
    }

    let primary = pick_primary_representation(&representations);
    let primary_kind = primary
        .map(|rep| rep.classification.clone())
        .unwrap_or_else(|| "empty".to_string());
    let primary_uti = primary.map(|rep| rep.uti.clone());

    let search_text = item_search_text(&representations);
    let mut item = ClipboardItem {
        item_index,
        primary_kind,
        primary_uti,
        preview_text: String::new(),
        search_text,
        total_bytes: representations.iter().map(|rep| rep.byte_len).sum(),
        representations,
    };
    item.preview_text = preview_for_item(&item);

    Ok(item)
}

fn item_search_text(representations: &[ClipboardRepresentation]) -> String {
    let fragments = representations.iter().filter_map(search_fragment_for_representation);
    dedupe_text_fragments(fragments).join("\n\n")
}

fn search_fragment_for_representation(rep: &ClipboardRepresentation) -> Option<String> {
    let text = rep.text_value.as_deref()?;

    match rep.classification.as_str() {
        "html" => Some(html_to_text_lossy(text)),
        "rtf" => Some(rtf_to_text_lossy(text)),
        "plain_text" | "url" | "file_url" | "json" | "xml" => Some(text.to_string()),
        _ => None,
    }
}

