use super::{normalise_whitespace, rtf_to_text_lossy};
use std::time::{Duration, Instant};

#[test]
#[ignore = "profiling harness for model whitespace normalization"]
fn profile_normalise_whitespace() {
    let text = large_whitespace_heavy_text(20_000);

    let before = median_duration(15, || {
        let normalized = normalise_whitespace_collect_join_for_profile(&text);
        assert!(normalized.contains("clipmem token 19999"));
    });
    let after = median_duration(15, || {
        let normalized = normalise_whitespace(&text);
        assert!(normalized.contains("clipmem token 19999"));
    });

    eprintln!("normalise_whitespace_collect_before={before:?} normalise_whitespace_stream_after={after:?}");
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

fn large_whitespace_heavy_text(token_count: usize) -> String {
    let mut out = String::with_capacity(token_count * 32);
    for index in 0..token_count {
        out.push_str(" \t clipmem\n token ");
        out.push_str(&index.to_string());
        out.push_str("\r\n");
    }
    out
}

fn normalise_whitespace_collect_join_for_profile(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
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

    normalise_whitespace_collect_join_for_profile(&out)
}
