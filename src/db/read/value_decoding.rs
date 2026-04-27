use crate::file_url::normalize_file_path;

const LIST_VALUE_SEPARATOR: char = '\u{1f}';
const MATCHED_FIELDS_SEPARATOR: char = '\u{1e}';

pub(in crate::db) fn split_aggregated_values(value: &str) -> Vec<String> {
    if value.is_empty() {
        Vec::new()
    } else {
        value
            .split(LIST_VALUE_SEPARATOR)
            .filter(|entry| !entry.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }
}

pub(in crate::db) fn split_aggregated_file_paths(value: &str) -> Vec<String> {
    if value.is_empty() {
        Vec::new()
    } else {
        value
            .split(LIST_VALUE_SEPARATOR)
            .filter(|entry| !entry.is_empty())
            .map(normalize_file_path)
            .collect()
    }
}

pub(in crate::db) fn split_match_fields(value: &str) -> Vec<String> {
    if value.is_empty() {
        Vec::new()
    } else {
        value
            .split(MATCHED_FIELDS_SEPARATOR)
            .filter(|entry| !entry.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }
}

#[cfg(test)]
mod profile_tests {
    use std::time::{Duration, Instant};

    use super::{split_aggregated_file_paths, split_aggregated_values, LIST_VALUE_SEPARATOR};

    #[test]
    #[ignore = "profiling harness for aggregated file path row mapping"]
    fn profile_aggregated_file_path_normalization() {
        let aggregated = large_file_url_list(20_000);

        let before = median_duration(11, 20_000, || {
            let paths = split_aggregated_file_paths_before_for_profile(&aggregated);
            assert_eq!(
                paths.last().map(String::as_str),
                Some("/Users/test/project-19999/Cargo.toml")
            );
            paths.len()
        });
        let after = median_duration(11, 20_000, || {
            let paths = split_aggregated_file_paths(&aggregated);
            assert_eq!(
                paths.last().map(String::as_str),
                Some("/Users/test/project-19999/Cargo.toml")
            );
            paths.len()
        });

        eprintln!(
            "aggregated_file_paths_allocate_then_decode_before={before:?} aggregated_file_paths_stream_after={after:?}"
        );
    }

    fn median_duration(
        runs: usize,
        expected_count: usize,
        mut f: impl FnMut() -> usize,
    ) -> Duration {
        let mut samples = Vec::with_capacity(runs);
        for _ in 0..runs {
            let started = Instant::now();
            let count = f();
            assert_eq!(count, expected_count);
            samples.push(started.elapsed());
        }
        samples.sort();
        samples[samples.len() / 2]
    }

    fn large_file_url_list(path_count: usize) -> String {
        let mut out = String::with_capacity(path_count * 48);
        for index in 0..path_count {
            if index > 0 {
                out.push(LIST_VALUE_SEPARATOR);
            }
            out.push_str("file:///Users/test/project-");
            out.push_str(&index.to_string());
            out.push_str("/Cargo.toml");
        }
        out
    }

    fn split_aggregated_file_paths_before_for_profile(value: &str) -> Vec<String> {
        split_aggregated_values(value)
            .into_iter()
            .map(|value| normalize_file_path_before_for_profile(&value))
            .collect()
    }

    fn normalize_file_path_before_for_profile(value: &str) -> String {
        decode_file_url_path_before_for_profile(value).unwrap_or_else(|| value.to_string())
    }

    fn decode_file_url_path_before_for_profile(value: &str) -> Option<String> {
        let rest = value.strip_prefix("file://")?;
        let path = rest
            .strip_prefix("localhost/")
            .or_else(|| rest.strip_prefix('/'))
            .map(|path| format!("/{path}"))?;
        let mut out = Vec::with_capacity(path.len());
        let bytes = path.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'%' && index + 2 < bytes.len() {
                if let (Some(high), Some(low)) =
                    (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
                {
                    out.push((high << 4) | low);
                    index += 3;
                    continue;
                }
            }
            out.push(bytes[index]);
            index += 1;
        }
        String::from_utf8(out).ok()
    }

    fn hex_value(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }
}
