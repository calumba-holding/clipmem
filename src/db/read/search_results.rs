use std::collections::HashSet;

use crate::db::types::{Page, SearchMode, SearchResults};
use crate::model::{SearchHit, TimelineEvent};

pub(in crate::db) fn paginate_rows(mut rows: Vec<SearchHit>, limit: usize) -> Page<SearchHit> {
    let has_more = rows.len() > limit;
    if has_more {
        rows.truncate(limit);
    }

    Page::new(rows, has_more)
}

pub(in crate::db) fn merge_scored_search_results(
    mode: SearchMode,
    native: SearchResults,
    ocr: SearchResults,
    limit: usize,
    lower_score_is_better: bool,
) -> SearchResults {
    let mut rows = native.hits().to_vec();
    let mut seen = rows
        .iter()
        .map(SearchHit::snapshot_id)
        .collect::<HashSet<_>>();
    for hit in ocr.hits() {
        if seen.insert(hit.snapshot_id()) {
            rows.push(hit.clone());
        }
    }

    rows.sort_by(|left, right| {
        let left_score = left.score().unwrap_or(if lower_score_is_better {
            f64::INFINITY
        } else {
            f64::NEG_INFINITY
        });
        let right_score = right.score().unwrap_or(if lower_score_is_better {
            f64::INFINITY
        } else {
            f64::NEG_INFINITY
        });
        let score_order = if lower_score_is_better {
            left_score
                .partial_cmp(&right_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        } else {
            right_score
                .partial_cmp(&left_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        };
        score_order
            .then_with(|| right.last_observed_at().cmp(left.last_observed_at()))
            .then_with(|| right.snapshot_id().cmp(&left.snapshot_id()))
    });

    let has_more = native.has_more() || ocr.has_more() || rows.len() > limit;
    if rows.len() > limit {
        rows.truncate(limit);
    }
    SearchResults::new(mode, rows, has_more)
}

pub(in crate::db) fn paginate_timeline_rows(
    mut rows: Vec<TimelineEvent>,
    limit: usize,
) -> Page<TimelineEvent> {
    let has_more = rows.len() > limit;
    if has_more {
        rows.truncate(limit);
    }

    Page::new(rows, has_more)
}
