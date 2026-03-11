use anyhow::{Context, Result};
use chrono::Utc;
use flow_db::repo::{
    clear_analysis_state, insert_normalized_event_for_raw_event, insert_pattern, insert_session,
    insert_suggestion, list_normalized_events, list_pending_file_raw_events,
};
use flow_patterns::{
    detect::detect_repeated_patterns, normalize::normalize, sessions::split_into_sessions,
};
use rusqlite::Connection;

/// The analysis layer converts persisted raw events into normalized workflow
/// data, then rebuilds sessions, patterns, and suggestions deterministically.
pub fn catch_up_analysis(conn: &mut Connection, inactivity_secs: i64) -> Result<()> {
    normalize_pending_raw_events(conn)?;
    rebuild_analysis_state(conn, inactivity_secs)?;
    Ok(())
}

pub fn normalize_pending_raw_events(conn: &Connection) -> Result<()> {
    for raw_event in
        list_pending_file_raw_events(conn).context("failed to load pending raw file events")?
    {
        let Some(normalized_event) = normalize(&raw_event.event) else {
            continue;
        };

        insert_normalized_event_for_raw_event(conn, raw_event.id, &normalized_event)
            .context("failed to insert normalized event")?;
    }

    Ok(())
}

pub fn rebuild_analysis_state(conn: &mut Connection, inactivity_secs: i64) -> Result<()> {
    let stored_events = list_normalized_events(conn).context("failed to load normalized events")?;
    let normalized_events: Vec<_> = stored_events
        .iter()
        .map(|stored| stored.event.clone())
        .collect();
    let sessions = split_into_sessions(&normalized_events, inactivity_secs);
    let patterns = detect_repeated_patterns(&sessions);
    let created_at = Utc::now().to_rfc3339();

    let tx = conn
        .transaction()
        .context("failed to start analysis rebuild transaction")?;
    clear_analysis_state(&tx).context("failed to clear analysis state")?;

    let mut offset = 0usize;
    for session in &sessions {
        let next_offset = offset + session.events.len();
        let event_ids: Vec<_> = stored_events[offset..next_offset]
            .iter()
            .map(|stored| stored.id)
            .collect();
        insert_session(
            &tx,
            &session.start_ts.to_rfc3339(),
            &session.end_ts.to_rfc3339(),
            &event_ids,
        )
        .context("failed to persist rebuilt session")?;
        offset = next_offset;
    }

    for pattern in patterns {
        let pattern_id = insert_pattern(
            &tx,
            &pattern.signature,
            pattern.count,
            pattern.avg_duration_ms,
            &pattern.canonical_summary,
        )
        .context("failed to persist rebuilt pattern")?;
        insert_suggestion(&tx, pattern_id, &pattern.proposal_text, &created_at)
            .context("failed to persist rebuilt suggestion")?;
    }

    tx.commit()
        .context("failed to commit analysis rebuild transaction")?;
    Ok(())
}
