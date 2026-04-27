pub(in crate::db) fn normalize_file_path(value: &str) -> String {
    decode_file_url_path(value).unwrap_or_else(|| value.to_string())
}

pub(in crate::db) fn decode_file_url_path(value: &str) -> Option<String> {
    const PREFIX: &str = "file://";
    if !has_ascii_prefix(value, PREFIX) {
        return None;
    }

    let rest = strip_localhost_authority(&value[PREFIX.len()..]);

    if rest.is_empty() {
        None
    } else if rest.as_bytes().contains(&b'%') {
        Some(percent_decode(rest))
    } else {
        Some(rest.to_string())
    }
}

fn strip_localhost_authority(value: &str) -> &str {
    const LOCALHOST: &str = "localhost";
    if has_ascii_prefix(value, LOCALHOST)
        && (value.len() == LOCALHOST.len() || value.as_bytes()[LOCALHOST.len()] == b'/')
    {
        &value[LOCALHOST.len()..]
    } else {
        value
    }
}

fn has_ascii_prefix(value: &str, prefix: &str) -> bool {
    value
        .as_bytes()
        .get(..prefix.len())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(prefix.as_bytes()))
}

pub(in crate::db) fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    if !bytes.contains(&b'%') {
        return value.to_string();
    }

    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(first), Some(second)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                out.push((first << 4) | second);
                index += 3;
                continue;
            }
        }

        out.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&out).into_owned()
}

pub(in crate::db::read) fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(in crate::db) fn file_url_cache_path_fragment(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .bytes()
        .map(|byte| match byte {
            b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02x}"),
        })
        .collect()
}
