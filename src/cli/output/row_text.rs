use crate::model::SearchHit;

pub(in crate::cli) fn best_text_from_hit(hit: &SearchHit) -> String {
    if hit.preview_text().trim().is_empty() {
        hit.why_matched()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(hit.preview_text())
            .to_string()
    } else {
        hit.preview_text().to_string()
    }
}

pub(in crate::cli) fn truncate_for_display(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        value.to_string()
    } else {
        value
            .chars()
            .take(limit.saturating_sub(1))
            .collect::<String>()
            + "…"
    }
}
