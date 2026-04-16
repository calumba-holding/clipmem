use std::collections::{BTreeSet, HashSet};
use std::env;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::model::{ClipboardItem, ClipboardRepresentation};

pub fn default_db_path() -> PathBuf {
    if cfg!(target_os = "macos") {
        expand_tilde("~/Library/Application Support/clipmem/clipmem.sqlite3")
    } else {
        expand_tilde("~/.local/state/clipmem/clipmem.sqlite3")
    }
}

pub fn expand_tilde<S: AsRef<str>>(path: S) -> PathBuf {
    let path = path.as_ref();
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return home.join(stripped);
        }
    }
    PathBuf::from(path)
}

pub fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn fingerprint_snapshot(items: &[ClipboardItem]) -> String {
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

pub fn snapshot_kind(items: &[ClipboardItem]) -> String {
    let kinds = items
        .iter()
        .map(|item| item.primary_kind.clone())
        .collect::<BTreeSet<_>>();

    if kinds.is_empty() {
        "empty".to_string()
    } else if kinds.len() == 1 {
        kinds.into_iter().next().unwrap_or_else(|| "empty".to_string())
    } else {
        "mixed".to_string()
    }
}

pub fn truncate_chars<S: AsRef<str>>(text: S, max_chars: usize) -> String {
    let text = text.as_ref();
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

pub fn normalise_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn dedupe_text_fragments<I>(fragments: I) -> Vec<String>
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

pub fn kind_priority(kind: &str) -> usize {
    match kind {
        "plain_text" => 0,
        "url" => 1,
        "file_url" => 2,
        "html" => 3,
        "json" => 4,
        "xml" => 5,
        "rtf" => 6,
        "pdf" => 7,
        "image" => 8,
        "binary" => 9,
        "empty" => 10,
        _ => 11,
    }
}

pub fn classify_uti(uti: &str, text_value_present: bool) -> &'static str {
    let lower = uti.to_ascii_lowercase();

    if lower.contains("file-url") || lower.contains("nsfilenamespboardtype") {
        "file_url"
    } else if lower.contains("html") {
        "html"
    } else if lower.contains("rtf") {
        "rtf"
    } else if lower.contains("json") {
        "json"
    } else if lower.contains("xml") || lower.contains("plist") {
        "xml"
    } else if lower.contains("pdf") {
        "pdf"
    } else if lower.contains("png")
        || lower.contains("jpeg")
        || lower.contains("jpg")
        || lower.contains("tiff")
        || lower.contains("gif")
        || lower.contains("bmp")
        || lower.contains("webp")
        || lower.contains("image")
    {
        "image"
    } else if lower.contains("url") || lower.contains("nsurlpboardtype") {
        "url"
    } else if lower.contains("text")
        || lower.contains("string")
        || lower.contains("utf8")
        || lower.contains("utf-8")
        || lower.contains("plain-text")
        || lower.contains("nsstringpboardtype")
        || text_value_present
    {
        "plain_text"
    } else {
        "binary"
    }
}

pub fn likely_textual_kind(kind: &str) -> bool {
    matches!(kind, "plain_text" | "url" | "file_url" | "html" | "rtf" | "json" | "xml")
}

pub fn decode_textish_bytes(bytes: &[u8]) -> Option<String> {
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

    if printable_ratio(bytes) >= 0.85 {
        return Some(String::from_utf8_lossy(bytes).into_owned());
    }

    None
}

fn detect_utf16_endianness(bytes: &[u8]) -> Option<bool> {
    if bytes.len() < 2 || bytes.len() % 2 != 0 {
        return None;
    }

    if bytes.starts_with(&[0xff, 0xfe]) {
        return Some(true);
    }
    if bytes.starts_with(&[0xfe, 0xff]) {
        return Some(false);
    }

    let lane_len = bytes.len() / 2;
    let even_nul_ratio = nul_ratio_for_lane(bytes, 0, lane_len);
    let odd_nul_ratio = nul_ratio_for_lane(bytes, 1, lane_len);

    if even_nul_ratio >= 0.6 && odd_nul_ratio <= 0.2 {
        Some(false)
    } else if odd_nul_ratio >= 0.6 && even_nul_ratio <= 0.2 {
        Some(true)
    } else {
        None
    }
}

fn nul_ratio_for_lane(bytes: &[u8], lane: usize, lane_len: usize) -> f32 {
    let nul_count = bytes
        .iter()
        .skip(lane)
        .step_by(2)
        .filter(|&&byte| byte == 0)
        .count();

    nul_count as f32 / lane_len as f32
}

fn decode_utf16(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 2 || bytes.len() % 2 != 0 {
        return None;
    }

    let le = decode_utf16_with_endian(bytes, true)?;
    let be = decode_utf16_with_endian(bytes, false)?;

    let le_score = readable_text_score(&le);
    let be_score = readable_text_score(&be);

    if le_score >= be_score {
        Some(le)
    } else {
        Some(be)
    }
}

fn decode_utf16_with_endian(bytes: &[u8], little_endian: bool) -> Option<String> {
    let mut words = Vec::with_capacity(bytes.len() / 2);
    let mut iter = bytes.chunks_exact(2);

    while let Some(chunk) = iter.next() {
        let word = if little_endian {
            u16::from_le_bytes([chunk[0], chunk[1]])
        } else {
            u16::from_be_bytes([chunk[0], chunk[1]])
        };
        words.push(word);
    }

    if words.first().copied() == Some(0xfeff) {
        words.remove(0);
    } else if words.first().copied() == Some(0xfffe) {
        words.remove(0);
    }

    String::from_utf16(&words).ok()
}

fn printable_ratio(bytes: &[u8]) -> f32 {
    let printable = bytes
        .iter()
        .filter(|&&b| {
            b == b'\n' || b == b'\r' || b == b'\t' || (0x20..=0x7e).contains(&b)
        })
        .count();

    printable as f32 / bytes.len() as f32
}

fn readable_text_score(text: &str) -> usize {
    text.chars()
        .filter(|c| {
            c.is_ascii_graphic()
                || c.is_whitespace()
                || (*c as u32) > 0x7f && !c.is_control()
        })
        .count()
}

pub fn html_to_text_lossy(html: &str) -> String {
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
                    "nbsp" => " ",
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

pub fn rtf_to_text_lossy(rtf: &str) -> String {
    let mut out = String::with_capacity(rtf.len());
    let chars = rtf.chars().collect::<Vec<_>>();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            '{' | '}' => {
                i += 1;
            }
            '\\' => {
                i += 1;
                if i >= chars.len() {
                    break;
                }

                match chars[i] {
                    '\\' | '{' | '}' => {
                        out.push(chars[i]);
                        i += 1;
                    }
                    '\'' => {
                        if i + 2 < chars.len() {
                            let hex = format!("{}{}", chars[i + 1], chars[i + 2]);
                            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                                out.push(byte as char);
                            }
                            i += 3;
                        } else {
                            i += 1;
                        }
                    }
                    c if c.is_ascii_alphabetic() => {
                        let start = i;
                        while i < chars.len() && chars[i].is_ascii_alphabetic() {
                            i += 1;
                        }
                        let word = chars[start..i].iter().collect::<String>();

                        if i < chars.len() && (chars[i] == '-' || chars[i].is_ascii_digit()) {
                            i += 1;
                            while i < chars.len() && chars[i].is_ascii_digit() {
                                i += 1;
                            }
                        }

                        if i < chars.len() && chars[i] == ' ' {
                            i += 1;
                        }

                        match word.as_str() {
                            "par" | "line" => out.push('\n'),
                            "tab" => out.push('\t'),
                            _ => {}
                        }
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
            ch => {
                out.push(ch);
                i += 1;
            }
        }
    }

    normalise_whitespace(&out)
}

pub fn pick_primary_representation<'a>(
    reps: &'a [ClipboardRepresentation],
) -> Option<&'a ClipboardRepresentation> {
    reps.iter()
        .min_by_key(|rep| (kind_priority(&rep.classification), !rep.is_text, rep.uti.as_str()))
}

pub fn preview_for_item(item: &ClipboardItem) -> String {
    if !item.search_text.is_empty() {
        return truncate_chars(item.search_text.replace('\n', " "), 200);
    }

    if let Some(rep) = pick_primary_representation(&item.representations) {
        return truncate_chars(
            format!("[{} · {} bytes · {}]", rep.classification, rep.byte_len, rep.uti),
            200,
        );
    }

    "[empty clipboard item]".to_string()
}

pub fn join_item_previews(items: &[ClipboardItem]) -> String {
    let parts = items
        .iter()
        .map(|item| item.preview_text.clone())
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>();

    if parts.is_empty() {
        "[empty clipboard]".to_string()
    } else {
        truncate_chars(parts.join(" | "), 280)
    }
}

pub fn joined_search_text(items: &[ClipboardItem]) -> String {
    items.iter()
        .map(|item| item.search_text.clone())
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn total_bytes(items: &[ClipboardItem]) -> usize {
    items.iter().map(|item| item.total_bytes).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

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
            .flat_map(|w| w.to_le_bytes())
            .collect::<Vec<_>>();
        assert_eq!(decode_textish_bytes(&bytes).as_deref(), Some("hello"));
    }

    #[test]
    fn utf16le_bom_decoding_works() {
        let mut bytes = vec![0xff, 0xfe];
        bytes.extend("hello".encode_utf16().flat_map(|w| w.to_le_bytes()));
        assert_eq!(decode_textish_bytes(&bytes).as_deref(), Some("hello"));
    }

    #[test]
    fn utf16be_decoding_works() {
        let bytes = "hello"
            .encode_utf16()
            .flat_map(|w| w.to_be_bytes())
            .collect::<Vec<_>>();
        assert_eq!(decode_textish_bytes(&bytes).as_deref(), Some("hello"));
    }
}
