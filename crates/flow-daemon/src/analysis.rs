use anyhow::{Context, Result};
use flow_db::repo::{insert_normalized_events_for_raw_events, list_pending_file_raw_events};
use flow_patterns::normalize::normalize;
use rusqlite::Connection;

#[cfg(test)]
use flow_db::repo::refresh_analysis_state;

/// The analysis layer converts persisted raw events into normalized workflow
/// data, then rebuilds sessions, patterns, and suggestions deterministically.
pub fn catch_up_analysis(conn: &mut Connection, _inactivity_secs: i64) -> Result<()> {
    normalize_pending_raw_events(conn)?;
    Ok(())
}

pub fn normalize_pending_raw_events(conn: &mut Connection) -> Result<()> {
    let mut normalized_events = Vec::new();

    for raw_event in
        list_pending_file_raw_events(conn).context("failed to load pending raw file events")?
    {
        let Some(normalized_event) = normalize(&raw_event.event) else {
            continue;
        };

        normalized_events.push((raw_event.id, normalized_event));
    }

    insert_normalized_events_for_raw_events(conn, &normalized_events)
        .context("failed to insert normalized events")?;

    Ok(())
}

#[cfg(test)]
pub fn rebuild_analysis_state(conn: &mut Connection, inactivity_secs: i64) -> Result<()> {
    refresh_analysis_state(conn, inactivity_secs).context("failed to refresh analysis state")?;
    Ok(())
}
