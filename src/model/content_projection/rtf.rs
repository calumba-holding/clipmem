use super::{ProjectionDiagnostic, TextProjectionResult};

const MAX_INPUT_BYTES: usize = 1_000_000;
const MAX_OUTPUT_CHARS: usize = 200_000;
const MAX_GROUP_DEPTH: usize = 512;

#[derive(Clone, Copy)]
struct State {
    skip: bool,
    ignorable: bool,
    uc: usize,
}

pub fn project_rtf(input: &str) -> TextProjectionResult {
    let mut diagnostics = Vec::new();
    let input = if input.len() > MAX_INPUT_BYTES {
        diagnostics.push(ProjectionDiagnostic::InputTruncated);
        let mut end = MAX_INPUT_BYTES;
        while !input.is_char_boundary(end) {
            end -= 1;
        }
        &input[..end]
    } else {
        input
    };
    let bytes = input.as_bytes();
    let mut stack = vec![State {
        skip: false,
        ignorable: false,
        uc: 1,
    }];
    let mut out = String::new();
    let mut i = 0usize;
    let mut fallback = 0usize;
    while i < bytes.len() && out.chars().count() < MAX_OUTPUT_CHARS {
        if fallback > 0 {
            i = skip_fallback(bytes, i);
            fallback -= 1;
            continue;
        }
        match bytes[i] {
            b'{' => {
                if stack.len() < MAX_GROUP_DEPTH {
                    stack.push(*stack.last().unwrap());
                } else {
                    diagnostics.push(ProjectionDiagnostic::InputTruncated);
                }
                i += 1;
            }
            b'}' => {
                if stack.len() > 1 {
                    stack.pop();
                } else {
                    diagnostics.push(ProjectionDiagnostic::MalformedMarkup);
                }
                i += 1;
            }
            b'\\' => {
                let (next, control) = parse_control(bytes, i + 1);
                i = next;
                let state = stack.last_mut().unwrap();
                match control {
                    Control::Symbol(b'\\' | b'{' | b'}') if !state.skip => {
                        out.push(control.symbol().unwrap() as char)
                    }
                    Control::Symbol(b'*') => state.ignorable = true,
                    Control::Hex(value) if !state.skip => out.push(char::from(value)),
                    Control::Word(word, parameter) => {
                        if is_destination(word) || (state.ignorable && !is_known_control(word)) {
                            state.skip = true;
                        }
                        if word == "uc" {
                            if let Some(n) = parameter {
                                state.uc = n.max(0) as usize;
                            }
                        }
                        if word == "u" && !state.skip {
                            if let Some(n) = parameter {
                                let scalar = (n as i16) as u16;
                                out.push(
                                    char::decode_utf16([scalar])
                                        .next()
                                        .and_then(Result::ok)
                                        .unwrap_or('\u{fffd}'),
                                );
                                fallback = state.uc;
                            }
                        }
                        if !state.skip {
                            match word {
                                "par" | "line" => out.push('\n'),
                                "tab" => out.push('\t'),
                                "emdash" => out.push('—'),
                                "endash" => out.push('–'),
                                "bullet" => out.push('•'),
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            byte => {
                if !stack.last().unwrap().skip && byte >= 0x20 {
                    out.push(byte as char);
                }
                i += 1;
            }
        }
    }
    if stack.len() != 1 {
        diagnostics.push(ProjectionDiagnostic::MalformedMarkup);
    }
    if i < bytes.len() {
        diagnostics.push(ProjectionDiagnostic::OutputTruncated);
    }
    TextProjectionResult {
        text: normalize(&out),
        urls: Vec::new(),
        diagnostics,
    }
}

enum Control<'a> {
    Symbol(u8),
    Hex(u8),
    Word(&'a str, Option<i32>),
}
impl Control<'_> {
    fn symbol(&self) -> Option<u8> {
        if let Self::Symbol(v) = self {
            Some(*v)
        } else {
            None
        }
    }
}

fn parse_control(bytes: &[u8], start: usize) -> (usize, Control<'_>) {
    if start >= bytes.len() {
        return (start, Control::Symbol(0));
    }
    if bytes[start] == b'\'' && start + 2 < bytes.len() {
        if let Ok(hex) = std::str::from_utf8(&bytes[start + 1..start + 3]) {
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                return (start + 3, Control::Hex(v));
            }
        }
    }
    if !bytes[start].is_ascii_alphabetic() {
        return (start + 1, Control::Symbol(bytes[start]));
    }
    let mut i = start;
    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
        i += 1;
    }
    let word = std::str::from_utf8(&bytes[start..i]).unwrap_or("");
    let number_start = i;
    if i < bytes.len() && bytes[i] == b'-' {
        i += 1;
    }
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    let parameter = (i > number_start)
        .then(|| {
            std::str::from_utf8(&bytes[number_start..i])
                .ok()?
                .parse()
                .ok()
        })
        .flatten();
    if i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    }
    (i, Control::Word(word, parameter))
}

fn skip_fallback(bytes: &[u8], i: usize) -> usize {
    if bytes.get(i) == Some(&b'\\') {
        parse_control(bytes, i + 1).0
    } else {
        i + 1
    }
}

fn is_destination(word: &str) -> bool {
    matches!(
        word,
        "fonttbl"
            | "colortbl"
            | "stylesheet"
            | "info"
            | "pict"
            | "object"
            | "annotation"
            | "header"
            | "footer"
            | "fldinst"
            | "datastore"
            | "themedata"
    )
}
fn is_known_control(word: &str) -> bool {
    matches!(
        word,
        "rtf"
            | "ansi"
            | "mac"
            | "deff"
            | "par"
            | "line"
            | "tab"
            | "u"
            | "uc"
            | "emdash"
            | "endash"
            | "bullet"
    )
}
fn normalize(text: &str) -> String {
    text.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::project_rtf;

    #[test]
    fn unicode_fallback_destinations_and_escapes() {
        let p = project_rtf(
            r"{\rtf1\ansi{\fonttbl{\f0 ignored;}}Hello \u8212? world\par escaped \{brace\} \'e9{\pict deadbeef}}",
        );
        assert_eq!(p.text, "Hello — world\nescaped {brace} é");
    }

    #[test]
    fn ignorable_destination_is_skipped() {
        assert_eq!(
            project_rtf(r"{\rtf1 before {\*\unknown hidden} after}").text,
            "before after"
        );
    }
}
