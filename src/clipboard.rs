use std::collections::{BTreeSet, HashSet};

use crate::model::{
    CaptureContext, ClipboardItem, ClipboardKind, ClipboardRepresentation, ClipboardSnapshot,
    SnapshotKind,
};

/// Build a normalized snapshot from captured clipboard items and capture metadata.
#[must_use]
pub fn build_snapshot(capture: CaptureContext, items: Vec<ClipboardItem>) -> ClipboardSnapshot {
    let (change_count, frontmost_app_bundle_id, frontmost_app_name) = capture.into_parts();
    let item_count = items.len();
    let total_bytes = items.iter().map(|item| item.total_bytes).sum();

    ClipboardSnapshot {
        change_count,
        frontmost_app_bundle_id,
        frontmost_app_name,
        fingerprint: fingerprint_snapshot(&items),
        snapshot_kind: snapshot_kind(&items),
        preview_text: join_item_previews(&items),
        search_text: joined_search_text(&items),
        item_count,
        total_bytes,
        items,
    }
}

/// Build a normalized item from its captured representations.
#[must_use]
pub fn build_item(
    item_index: usize,
    representations: Vec<ClipboardRepresentation>,
) -> ClipboardItem {
    let primary = pick_primary_representation(&representations);
    let primary_kind = primary.map_or(ClipboardKind::Empty, |rep| rep.kind);
    let primary_uti = primary.map(|rep| rep.uti.clone());

    let search_text = item_search_text(&representations);
    let preview_text = preview_for_item(&search_text, &representations);
    ClipboardItem::new(
        item_index,
        primary_kind,
        primary_uti,
        preview_text,
        search_text,
        representations,
    )
}

/// Build a normalized representation from the raw clipboard payload captured for a single UTI.
#[must_use]
pub fn build_representation(
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

pub(crate) fn hash_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub(crate) fn classify_uti(uti: &str, text_value_present: bool) -> ClipboardKind {
    let lower = uti.to_ascii_lowercase();

    if lower.contains("file-url") || lower.contains("nsfilenamespboardtype") {
        ClipboardKind::FileUrl
    } else if lower.contains("html") {
        ClipboardKind::Html
    } else if lower.contains("rtf") {
        ClipboardKind::Rtf
    } else if lower.contains("json") {
        ClipboardKind::Json
    } else if lower.contains("xml") || lower.contains("plist") {
        ClipboardKind::Xml
    } else if lower.contains("pdf") {
        ClipboardKind::Pdf
    } else if lower.contains("png")
        || lower.contains("jpeg")
        || lower.contains("jpg")
        || lower.contains("tiff")
        || lower.contains("gif")
        || lower.contains("bmp")
        || lower.contains("webp")
        || lower.contains("image")
    {
        ClipboardKind::Image
    } else if lower.contains("url") || lower.contains("nsurlpboardtype") {
        ClipboardKind::Url
    } else if lower.contains("text")
        || lower.contains("string")
        || lower.contains("utf8")
        || lower.contains("utf-8")
        || lower.contains("plain-text")
        || lower.contains("nsstringpboardtype")
        || text_value_present
    {
        ClipboardKind::PlainText
    } else {
        ClipboardKind::Binary
    }
}

pub(crate) fn decode_text_bytes_lossy(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return Some(String::new());
    }

    if let Some(little_endian) = detect_utf16_endianness(bytes) {
        if let Some(text) = decode_utf16_with_endian(bytes, little_endian) {
            return Some(text);
        }
    }

    if let Ok(text) = std::str::from_utf8(bytes) {
        return Some(text.to_string());
    }

    if let Some(text) = decode_utf16(bytes) {
        return Some(text);
    }

    if is_mostly_printable(bytes) {
        return Some(String::from_utf8_lossy(bytes).into_owned());
    }

    None
}

fn fingerprint_snapshot(items: &[ClipboardItem]) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(b"clipmem/v1");

    for item in items {
        hasher.update((item.item_index as u64).to_be_bytes());
        let mut ordered = item.representations.iter().collect::<Vec<_>>();
        ordered.sort_by(|a, b| a.uti.cmp(&b.uti));

        for rep in ordered {
            hasher.update((rep.uti.len() as u64).to_be_bytes());
            hasher.update(rep.uti.as_bytes());
            hasher.update((rep.raw_bytes.len() as u64).to_be_bytes());
            hasher.update(&rep.raw_bytes);
        }
    }

    hex::encode(hasher.finalize())
}

fn snapshot_kind(items: &[ClipboardItem]) -> SnapshotKind {
    let kinds = items
        .iter()
        .map(|item| item.primary_kind)
        .collect::<BTreeSet<_>>();

    if kinds.is_empty() {
        SnapshotKind::Empty
    } else if kinds.len() == 1 {
        kinds
            .into_iter()
            .next()
            .map_or(SnapshotKind::Empty, SnapshotKind::from)
    } else {
        SnapshotKind::Mixed
    }
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let mut out = String::new();
    for (idx, ch) in text.chars().enumerate() {
        if idx >= max_chars.saturating_sub(1) {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

fn normalise_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn dedupe_text_fragments<I>(fragments: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for fragment in fragments {
        let collapsed = normalise_whitespace(&fragment);
        let trimmed = collapsed.trim();
        if trimmed.is_empty() {
            continue;
        }

        let key = trimmed.to_lowercase();
        if seen.insert(key) {
            out.push(trimmed.to_string());
        }
    }

    out
}

fn detect_utf16_endianness(bytes: &[u8]) -> Option<bool> {
    if bytes.len() < 2 || !bytes.len().is_multiple_of(2) {
        return None;
    }

    if bytes.starts_with(&[0xff, 0xfe]) {
        return Some(true);
    }
    if bytes.starts_with(&[0xfe, 0xff]) {
        return Some(false);
    }

    let lane_len = bytes.len() / 2;
    let even_nul_count = nul_count_for_lane(bytes, 0);
    let odd_nul_count = nul_count_for_lane(bytes, 1);

    if ratio_at_least(even_nul_count, lane_len, 6, 10)
        && ratio_at_most(odd_nul_count, lane_len, 2, 10)
    {
        Some(false)
    } else if ratio_at_least(odd_nul_count, lane_len, 6, 10)
        && ratio_at_most(even_nul_count, lane_len, 2, 10)
    {
        Some(true)
    } else {
        None
    }
}

fn nul_count_for_lane(bytes: &[u8], lane: usize) -> usize {
    bytes
        .iter()
        .skip(lane)
        .step_by(2)
        .filter(|&&byte| byte == 0)
        .count()
}

fn ratio_at_least(count: usize, total: usize, numerator: usize, denominator: usize) -> bool {
    count.saturating_mul(denominator) >= total.saturating_mul(numerator)
}

fn ratio_at_most(count: usize, total: usize, numerator: usize, denominator: usize) -> bool {
    count.saturating_mul(denominator) <= total.saturating_mul(numerator)
}

fn decode_utf16(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 2 || !bytes.len().is_multiple_of(2) {
        return None;
    }

    let little_endian = decode_utf16_with_endian(bytes, true)?;
    let big_endian = decode_utf16_with_endian(bytes, false)?;

    let little_endian_score = readable_text_score(&little_endian);
    let big_endian_score = readable_text_score(&big_endian);

    if little_endian_score >= big_endian_score {
        Some(little_endian)
    } else {
        Some(big_endian)
    }
}

fn decode_utf16_with_endian(bytes: &[u8], little_endian: bool) -> Option<String> {
    let mut words = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks_exact(2) {
        let word = if little_endian {
            u16::from_le_bytes([chunk[0], chunk[1]])
        } else {
            u16::from_be_bytes([chunk[0], chunk[1]])
        };
        words.push(word);
    }

    if matches!(words.first().copied(), Some(0xfeff | 0xfffe)) {
        words.remove(0);
    }

    String::from_utf16(&words).ok()
}

fn is_mostly_printable(bytes: &[u8]) -> bool {
    let printable = bytes
        .iter()
        .filter(|&&byte| {
            byte == b'\n' || byte == b'\r' || byte == b'\t' || (0x20..=0x7e).contains(&byte)
        })
        .count();

    ratio_at_least(printable, bytes.len(), 85, 100)
}

fn readable_text_score(text: &str) -> usize {
    text.chars()
        .filter(|character| {
            character.is_ascii_graphic()
                || character.is_whitespace()
                || (*character as u32) > 0x7f && !character.is_control()
        })
        .count()
}

fn html_to_text_lossy(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut entity = String::new();
    let mut in_entity = false;

    for ch in html.chars() {
        if in_tag {
            if ch == '>' {
                in_tag = false;
                out.push(' ');
            }
            continue;
        }

        if in_entity {
            if ch == ';' {
                out.push_str(match entity.as_str() {
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    "apos" => "'",
                    _ => " ",
                });
                entity.clear();
                in_entity = false;
            } else if entity.len() < 12 {
                entity.push(ch);
            } else {
                entity.clear();
                in_entity = false;
            }
            continue;
        }

        match ch {
            '<' => in_tag = true,
            '&' => {
                in_entity = true;
                entity.clear();
            }
            _ => out.push(ch),
        }
    }

    normalise_whitespace(&out)
}

fn rtf_to_text_lossy(rtf: &str) -> String {
    let mut out = String::with_capacity(rtf.len());
    let chars = rtf.chars().collect::<Vec<_>>();
    let mut index = 0;

    while index < chars.len() {
        match chars[index] {
            '{' | '}' => {
                index += 1;
            }
            '\\' => {
                index += 1;
                if index >= chars.len() {
                    break;
                }

                match chars[index] {
                    '\\' | '{' | '}' => {
                        out.push(chars[index]);
                        index += 1;
                    }
                    '\'' => {
                        if index + 2 < chars.len() {
                            let hex = format!("{}{}", chars[index + 1], chars[index + 2]);
                            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                                out.push(byte as char);
                            }
                            index += 3;
                        } else {
                            index += 1;
                        }
                    }
                    c if c.is_ascii_alphabetic() => {
                        let start = index;
                        while index < chars.len() && chars[index].is_ascii_alphabetic() {
                            index += 1;
                        }
                        let word = chars[start..index].iter().collect::<String>();

                        if index < chars.len()
                            && (chars[index] == '-' || chars[index].is_ascii_digit())
                        {
                            index += 1;
                            while index < chars.len() && chars[index].is_ascii_digit() {
                                index += 1;
                            }
                        }

                        if index < chars.len() && chars[index] == ' ' {
                            index += 1;
                        }

                        match word.as_str() {
                            "par" | "line" => out.push('\n'),
                            "tab" => out.push('\t'),
                            _ => {}
                        }
                    }
                    _ => {
                        index += 1;
                    }
                }
            }
            ch => {
                out.push(ch);
                index += 1;
            }
        }
    }

    normalise_whitespace(&out)
}

fn pick_primary_representation(
    reps: &[ClipboardRepresentation],
) -> Option<&ClipboardRepresentation> {
    reps.iter()
        .min_by_key(|rep| (rep.kind.priority(), !rep.is_text(), rep.uti.as_str()))
}

fn item_search_text(representations: &[ClipboardRepresentation]) -> String {
    let fragments = representations
        .iter()
        .filter_map(search_fragment_for_representation);
    dedupe_text_fragments(fragments).join("\n\n")
}

fn search_fragment_for_representation(rep: &ClipboardRepresentation) -> Option<String> {
    let text = rep.text_value.as_deref()?;

    match rep.kind {
        ClipboardKind::Html => Some(html_to_text_lossy(text)),
        ClipboardKind::Rtf => Some(rtf_to_text_lossy(text)),
        kind if kind.is_textual() => Some(text.to_string()),
        _ => None,
    }
}

fn preview_for_item(search_text: &str, representations: &[ClipboardRepresentation]) -> String {
    if !search_text.is_empty() {
        return truncate_chars(&search_text.replace('\n', " "), 200);
    }

    if let Some(rep) = pick_primary_representation(representations) {
        return truncate_chars(
            &format!("[{} · {} bytes · {}]", rep.kind, rep.byte_len, rep.uti),
            200,
        );
    }

    "[empty clipboard item]".to_string()
}

fn join_item_previews(items: &[ClipboardItem]) -> String {
    let parts = items
        .iter()
        .map(|item| item.preview_text.clone())
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>();

    if parts.is_empty() {
        "[empty clipboard]".to_string()
    } else {
        truncate_chars(&parts.join(" | "), 280)
    }
}

fn joined_search_text(items: &[ClipboardItem]) -> String {
    items
        .iter()
        .map(|item| item.search_text.clone())
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use crate::model::{CaptureContext, SnapshotKind};

    use super::{build_snapshot, decode_text_bytes_lossy, html_to_text_lossy, rtf_to_text_lossy};

    #[test]
    fn html_is_stripped_reasonably() {
        let html = "<p>Hello <strong>world</strong> &amp; friends</p>";
        assert_eq!(html_to_text_lossy(html), "Hello world & friends");
    }

    #[test]
    fn rtf_is_stripped_reasonably() {
        let rtf = r"{\rtf1\ansi hello\par world}";
        assert_eq!(rtf_to_text_lossy(rtf), "hello world");
    }

    #[test]
    fn utf16_decoding_works() {
        let bytes = "hello"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        assert_eq!(decode_text_bytes_lossy(&bytes).as_deref(), Some("hello"));
    }

    #[test]
    fn utf16le_bom_decoding_works() {
        let mut bytes = vec![0xff, 0xfe];
        bytes.extend("hello".encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(decode_text_bytes_lossy(&bytes).as_deref(), Some("hello"));
    }

    #[test]
    fn utf16be_decoding_works() {
        let bytes = "hello"
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect::<Vec<_>>();
        assert_eq!(decode_text_bytes_lossy(&bytes).as_deref(), Some("hello"));
    }

    #[test]
    fn empty_snapshots_stay_structurally_empty() {
        let snapshot = build_snapshot(CaptureContext::new(7), Vec::new());

        assert_eq!(snapshot.snapshot_kind(), SnapshotKind::Empty);
        assert_eq!(snapshot.item_count(), 0);
        assert!(snapshot.items().is_empty());
        assert_eq!(snapshot.preview_text(), "[empty clipboard]");
    }
}
