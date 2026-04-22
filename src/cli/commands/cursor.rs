use super::*;

pub(in crate::cli) fn parse_search_cursor(
    encoded: &str,
    query: &str,
    requested_mode: SearchMode,
    filters: &RetrievalFilters,
) -> Result<SearchCursorState> {
    let token: SearchCursorToken = decode_cursor(encoded)?;
    if token.command != "search" {
        return Err(anyhow!(
            "cursor is for command `{}` but was used with `search`",
            token.command
        ));
    }
    if token.query != query || token.requested_mode != requested_mode {
        return Err(anyhow!(
            "cursor does not match the active search query or mode"
        ));
    }
    if token.filters != *filters {
        return Err(anyhow!("cursor does not match the active search filters"));
    }
    if requested_mode != SearchMode::Auto && token.mode_used != requested_mode {
        return Err(anyhow!(
            "cursor mode `{}` does not match the requested search mode `{}`",
            token.mode_used.as_str(),
            requested_mode.as_str()
        ));
    }

    Ok(SearchCursorState::new(
        token.mode_used,
        token.score,
        token.last_seen_at,
        token.snapshot_id,
    ))
}

pub(in crate::cli) fn parse_recent_cursor(
    encoded: &str,
    filters: &RetrievalFilters,
) -> Result<RecentCursorState> {
    let token: RecentCursorToken = decode_cursor(encoded)?;
    if token.command != "recent" {
        return Err(anyhow!(
            "cursor is for command `{}` but was used with `recent`",
            token.command
        ));
    }
    if token.filters != *filters {
        return Err(anyhow!("cursor does not match the active recent filters"));
    }

    Ok(RecentCursorState::new(
        token.last_seen_at,
        token.snapshot_id,
    ))
}

pub(in crate::cli) fn parse_timeline_cursor(
    encoded: &str,
    filters: &RetrievalFilters,
    sort: TimelineSort,
) -> Result<TimelineCursorState> {
    let token: TimelineCursorToken = decode_cursor(encoded)?;
    if token.command != "timeline" {
        return Err(anyhow!(
            "cursor is for command `{}` but was used with `timeline`",
            token.command
        ));
    }
    if token.filters != *filters || token.sort != sort {
        return Err(anyhow!(
            "cursor does not match the active timeline filters or sort"
        ));
    }

    Ok(TimelineCursorState::new(token.observed_at, token.event_id))
}

pub(in crate::cli) fn encode_search_cursor(
    query: &str,
    requested_mode: SearchMode,
    filters: &RetrievalFilters,
    mode_used: SearchMode,
    hit: &crate::model::SearchHit,
) -> Result<String> {
    encode_cursor(&SearchCursorToken {
        command: "search".to_string(),
        query: query.to_string(),
        requested_mode,
        mode_used,
        filters: filters.clone(),
        last_seen_at: hit.last_observed_at().to_string(),
        snapshot_id: hit.snapshot_id(),
        score: hit.score(),
    })
}

pub(in crate::cli) fn encode_recent_cursor(
    filters: &RetrievalFilters,
    hit: &crate::model::SearchHit,
) -> Result<String> {
    encode_cursor(&RecentCursorToken {
        command: "recent".to_string(),
        filters: filters.clone(),
        last_seen_at: hit.last_observed_at().to_string(),
        snapshot_id: hit.snapshot_id(),
    })
}

pub(in crate::cli) fn encode_timeline_cursor(
    filters: &RetrievalFilters,
    sort: TimelineSort,
    event: &TimelineEvent,
) -> Result<String> {
    encode_cursor(&TimelineCursorToken {
        command: "timeline".to_string(),
        filters: filters.clone(),
        sort,
        observed_at: event.observed_at().to_string(),
        event_id: event.event_id(),
    })
}

pub(in crate::cli) fn encode_cursor<T: Serialize>(token: &T) -> Result<String> {
    let json = serde_json::to_vec(token)?;
    Ok(hex::encode(json))
}

pub(in crate::cli) fn decode_cursor<T: for<'de> Deserialize<'de>>(encoded: &str) -> Result<T> {
    let bytes =
        hex::decode(encoded).map_err(|error| anyhow!("invalid cursor encoding: {error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| anyhow!("invalid cursor payload: {error}"))
}
