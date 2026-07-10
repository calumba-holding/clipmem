use crate::db::types::Page;
use crate::model::{SearchHit, TimelineEvent};

pub(in crate::db) fn paginate_rows(mut rows: Vec<SearchHit>, limit: usize) -> Page<SearchHit> {
    let has_more = rows.len() > limit;
    if has_more {
        rows.truncate(limit);
    }

    Page::new(rows, has_more)
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
