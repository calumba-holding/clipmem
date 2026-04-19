use crate::model::ClipboardSnapshot;

const CONTEXT_CUES: &[&str] = &[
    "api key",
    "api-key",
    "apikey",
    "access token",
    "access_token",
    "auth token",
    "authorization",
    "bearer",
    "client secret",
    "client_secret",
    "secret key",
    "secret_key",
    "token",
    "x-api-key",
];

pub(crate) fn should_skip_snapshot_for_api_key_filter(snapshot: &ClipboardSnapshot) -> bool {
    let search_text = snapshot.search_text();
    if search_text.trim().is_empty() {
        return false;
    }

    text_looks_like_api_key(search_text)
}

fn text_looks_like_api_key(text: &str) -> bool {
    has_strong_api_key_signature(text) || has_contextual_token_match(text)
}

fn has_strong_api_key_signature(text: &str) -> bool {
    token_candidates(text).any(matches_strong_api_key_signature)
}

fn has_contextual_token_match(text: &str) -> bool {
    let lines = text.lines().map(str::trim).collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }

        let cue_nearby = [
            index.saturating_sub(1),
            index,
            (index + 1).min(lines.len() - 1),
        ]
        .into_iter()
        .any(|candidate| contains_context_cue(lines[candidate]));
        if !cue_nearby {
            continue;
        }

        if token_candidates(line).any(is_contextual_secret_candidate) {
            return true;
        }
    }

    false
}

fn contains_context_cue(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    CONTEXT_CUES.iter().any(|cue| lower.contains(cue))
}

fn matches_strong_api_key_signature(token: &str) -> bool {
    matches_openai_key(token)
        || matches_anthropic_key(token)
        || matches_github_key(token)
        || matches_google_api_key(token)
        || matches_aws_access_key_id(token)
        || matches_slack_token(token)
        || matches_stripe_key(token)
}

fn matches_openai_key(token: &str) -> bool {
    token.len() >= 36
        && (token.starts_with("sk-proj-")
            || token.starts_with("sk-live-")
            || token.starts_with("sk-svcacct-")
            || (token.starts_with("sk-") && !token.starts_with("sk_test_")))
        && token_chars_are_safe(token)
        && class_count(token) >= 2
}

fn matches_anthropic_key(token: &str) -> bool {
    token.len() >= 24
        && token.starts_with("sk-ant-")
        && token_chars_are_safe(token)
        && class_count(token) >= 2
}

fn matches_github_key(token: &str) -> bool {
    const SHORT_PREFIXES: &[&str] = &["ghp_", "gho_", "ghu_", "ghs_", "ghr_"];

    (SHORT_PREFIXES
        .iter()
        .any(|prefix| token.starts_with(prefix) && token.len() >= 24)
        || (token.starts_with("github_pat_") && token.len() >= 40))
        && token_chars_are_safe(token)
}

fn matches_google_api_key(token: &str) -> bool {
    token.len() >= 35
        && token.len() <= 45
        && token.starts_with("AIza")
        && token_chars_are_safe(token)
}

fn matches_aws_access_key_id(token: &str) -> bool {
    token.len() == 20
        && (token.starts_with("AKIA") || token.starts_with("ASIA"))
        && token
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
}

fn matches_slack_token(token: &str) -> bool {
    token.len() >= 24
        && token.starts_with("xox")
        && token.contains('-')
        && token_chars_are_safe(token)
}

fn matches_stripe_key(token: &str) -> bool {
    token.len() >= 24
        && (token.starts_with("sk_live_")
            || token.starts_with("sk_test_")
            || token.starts_with("rk_live_")
            || token.starts_with("rk_test_"))
        && token_chars_are_safe(token)
}

fn is_contextual_secret_candidate(token: &str) -> bool {
    let len = token.len();
    if !(20..=200).contains(&len) {
        return false;
    }
    if !token_chars_are_safe(token) {
        return false;
    }
    if token.starts_with("http") {
        return false;
    }
    if class_count(token) < 3 {
        return false;
    }

    let lower = token.to_ascii_lowercase();
    !matches!(
        lower.as_str(),
        "authorization" | "bearer" | "token" | "secret" | "password" | "apikey"
    )
}

fn token_candidates(text: &str) -> impl Iterator<Item = &str> {
    text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'))
        .filter(|token| !token.is_empty())
}

fn token_chars_are_safe(token: &str) -> bool {
    token
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

fn class_count(token: &str) -> usize {
    let mut seen_lower = false;
    let mut seen_upper = false;
    let mut seen_digit = false;
    let mut seen_symbol = false;

    for ch in token.chars() {
        if ch.is_ascii_lowercase() {
            seen_lower = true;
        } else if ch.is_ascii_uppercase() {
            seen_upper = true;
        } else if ch.is_ascii_digit() {
            seen_digit = true;
        } else if ch == '_' || ch == '-' {
            seen_symbol = true;
        }
    }

    [seen_lower, seen_upper, seen_digit, seen_symbol]
        .into_iter()
        .filter(|seen| *seen)
        .count()
}

#[cfg(test)]
mod tests {
    use super::should_skip_snapshot_for_api_key_filter;
    use crate::model::{build_item, build_representation, build_snapshot, CaptureContext};

    fn text_snapshot(text: &str) -> crate::model::ClipboardSnapshot {
        build_snapshot(
            CaptureContext::new(1)
                .with_frontmost_app_name("Terminal")
                .with_frontmost_app_bundle_id("com.apple.Terminal"),
            vec![build_item(
                0,
                vec![build_representation(
                    "public.utf8-plain-text".to_string(),
                    Some(text.to_string()),
                    text.as_bytes().to_vec(),
                )],
            )],
        )
    }

    #[test]
    fn detects_openai_style_keys() {
        let snapshot = text_snapshot("sk-proj-abcDEF1234567890abcdefghijklmnopqrstuv");
        assert!(should_skip_snapshot_for_api_key_filter(&snapshot));
    }

    #[test]
    fn detects_anthropic_style_keys() {
        let snapshot = text_snapshot("sk-ant-api03-AbCdEf1234567890abcdefghijklmnop");
        assert!(should_skip_snapshot_for_api_key_filter(&snapshot));
    }

    #[test]
    fn detects_github_pat_tokens() {
        let snapshot = text_snapshot("github_pat_11AA22bb33CC44dd55EE66ff77GG88hh99II00jj");
        assert!(should_skip_snapshot_for_api_key_filter(&snapshot));
    }

    #[test]
    fn detects_google_api_keys() {
        let snapshot = text_snapshot("AIzaSyD3XAMPLE1234567890abcDEFghiJKLmn");
        assert!(should_skip_snapshot_for_api_key_filter(&snapshot));
    }

    #[test]
    fn detects_aws_access_key_ids() {
        let snapshot = text_snapshot("AKIAIOSFODNN7EXAMPLE");
        assert!(should_skip_snapshot_for_api_key_filter(&snapshot));
    }

    #[test]
    fn detects_slack_tokens() {
        let token = [
            "xoxb",
            "123456789012",
            "123456789012",
            "abcdefghijklmnopqrstuvwx",
        ]
        .join("-");
        let snapshot = text_snapshot(&token);
        assert!(should_skip_snapshot_for_api_key_filter(&snapshot));
    }

    #[test]
    fn detects_stripe_keys() {
        let token = ["sk", "live", "51PFakeAbCdEfGhIjKlMnOpQrStUvWxYz123456789"].join("_");
        let snapshot = text_snapshot(&token);
        assert!(should_skip_snapshot_for_api_key_filter(&snapshot));
    }

    #[test]
    fn detects_contextual_generic_tokens() {
        let snapshot = text_snapshot("Authorization: Bearer 8JfA-2mQpV_4tLz9XnR6cH0wKdS7yBu3");
        assert!(should_skip_snapshot_for_api_key_filter(&snapshot));
    }

    #[test]
    fn ignores_random_base64_without_context() {
        let snapshot = text_snapshot("QWxhZGRpbjpvcGVuIHNlc2FtZQ");
        assert!(!should_skip_snapshot_for_api_key_filter(&snapshot));
    }

    #[test]
    fn ignores_git_shas_and_uuids() {
        let snapshot = text_snapshot(
            "commit 0123456789abcdef0123456789abcdef01234567\nrequest 123e4567-e89b-12d3-a456-426614174000",
        );
        assert!(!should_skip_snapshot_for_api_key_filter(&snapshot));
    }

    #[test]
    fn ignores_normal_shell_commands() {
        let snapshot = text_snapshot("git status && cargo test -q");
        assert!(!should_skip_snapshot_for_api_key_filter(&snapshot));
    }

    #[test]
    fn ignores_empty_search_text() {
        let snapshot = build_snapshot(CaptureContext::new(1), Vec::new());
        assert!(!should_skip_snapshot_for_api_key_filter(&snapshot));
    }
}
