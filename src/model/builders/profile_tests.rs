use super::{
    build_representation, classify_uti, decode_text_bytes_lossy, decode_text_bytes_strict,
    hash_bytes, normalize_whitespace, rtf_to_text_lossy,
};
use crate::model::ClipboardRepresentation;
use std::time::{Duration, Instant};

#[test]
#[ignore = "profiling harness for model whitespace normalization"]
fn profile_normalize_whitespace() {
    let text = large_whitespace_heavy_text(20_000);

    let before = median_duration(15, || {
        let normalized = normalize_whitespace_collect_join_for_profile(&text);
        assert!(normalized.contains("clipmem token 19999"));
    });
    let after = median_duration(15, || {
        let normalized = normalize_whitespace(&text);
        assert!(normalized.contains("clipmem token 19999"));
    });

    eprintln!("normalize_whitespace_collect_before={before:?} normalize_whitespace_stream_after={after:?}");
}

#[test]
#[ignore = "profiling harness for model RTF text extraction"]
fn profile_rtf_to_text_lossy() {
    let rtf = large_rtf_text(20_000);

    let before = median_duration(15, || {
        let text = rtf_to_text_lossy_vec_chars_for_profile(&rtf);
        assert!(text.contains("clipmem token 19999"));
    });
    let after = median_duration(15, || {
        let text = rtf_to_text_lossy(&rtf);
        assert!(text.contains("clipmem token 19999"));
    });

    eprintln!("rtf_to_text_vec_chars_before={before:?} rtf_to_text_stream_after={after:?}");
}

#[test]
#[ignore = "profiling harness for representation text-value normalization"]
fn profile_build_representation_with_string_value() {
    let text = large_plain_text(256 * 1024);
    let raw_bytes = text.as_bytes().to_vec();

    let expected_total = text.len() * 80;

    let before = median_duration_with_total(11, expected_total, || {
        (0..80)
            .map(|_| {
                build_representation_decode_raw_first_for_profile(
                    "public.utf8-plain-text".to_string(),
                    Some(text.clone()),
                    raw_bytes.clone(),
                )
                .text_value()
                .map(str::len)
                .unwrap_or(0)
            })
            .sum()
    });
    let after = median_duration_with_total(11, expected_total, || {
        (0..80)
            .map(|_| {
                build_representation(
                    "public.utf8-plain-text".to_string(),
                    Some(text.clone()),
                    raw_bytes.clone(),
                )
                .text_value()
                .map(str::len)
                .unwrap_or(0)
            })
            .sum()
    });

    eprintln!(
        "build_representation_decode_raw_first_before={before:?} build_representation_string_value_first_after={after:?}"
    );
}

fn median_duration(runs: usize, mut f: impl FnMut()) -> Duration {
    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        let started = Instant::now();
        f();
        samples.push(started.elapsed());
    }
    samples.sort();
    samples[samples.len() / 2]
}

fn median_duration_with_total(
    runs: usize,
    expected_total: usize,
    mut f: impl FnMut() -> usize,
) -> Duration {
    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        let started = Instant::now();
        let total = f();
        assert_eq!(total, expected_total);
        samples.push(started.elapsed());
    }
    samples.sort();
    samples[samples.len() / 2]
}

fn large_whitespace_heavy_text(token_count: usize) -> String {
    let mut out = String::with_capacity(token_count * 32);
    for index in 0..token_count {
        out.push_str(" \t clipmem\n token ");
        out.push_str(&index.to_string());
        out.push_str("\r\n");
    }
    out
}

fn large_plain_text(byte_count: usize) -> String {
    let mut out = String::with_capacity(byte_count);
    while out.len() < byte_count {
        out.push_str("clipmem captured text payload with enough content to benchmark decoding\n");
    }
    out.truncate(byte_count);
    out
}

fn normalize_whitespace_collect_join_for_profile(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn build_representation_decode_raw_first_for_profile(
    uti: String,
    string_value: Option<String>,
    raw_bytes: Vec<u8>,
) -> ClipboardRepresentation {
    let decoded_raw_text = decode_text_bytes_strict(&raw_bytes);
    let clean_string_value = string_value
        .as_deref()
        .filter(|text| is_searchable_text_fragment_before_for_profile(text))
        .map(ToOwned::to_owned);
    let kind = classify_uti(&uti, clean_string_value.is_some());
    let decoded_text = clean_string_value
        .or_else(|| {
            decoded_raw_text
                .as_deref()
                .filter(|text| is_searchable_text_fragment_before_for_profile(text))
                .map(ToOwned::to_owned)
        })
        .or_else(|| {
            if kind.is_textual() {
                decode_text_bytes_lossy(&raw_bytes)
                    .filter(|text| is_searchable_text_fragment_before_for_profile(text))
            } else {
                decoded_raw_text
            }
        });

    ClipboardRepresentation::new(uti, kind, hash_bytes(&raw_bytes), decoded_text, raw_bytes)
}

fn is_searchable_text_fragment_before_for_profile(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.contains('\0') {
        return false;
    }

    let total = trimmed.chars().count();
    if total == 0 {
        return false;
    }

    let disruptive_controls = trimmed
        .chars()
        .filter(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
        .count();
    super::ratio_at_most(disruptive_controls, total, 1, 20)
}

fn large_rtf_text(token_count: usize) -> String {
    let mut out = String::with_capacity(token_count * 48);
    out.push_str(r"{\rtf1\ansi ");
    for index in 0..token_count {
        out.push_str(r"\b clipmem\b0  token ");
        out.push_str(&index.to_string());
        out.push_str(r"\tab value\par ");
    }
    out.push('}');
    out
}

fn rtf_to_text_lossy_vec_chars_for_profile(rtf: &str) -> String {
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

    normalize_whitespace_collect_join_for_profile(&out)
}
