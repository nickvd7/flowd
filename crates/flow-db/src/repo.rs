use chrono::{DateTime, Utc};
use flow_core::events::{ActionType, EventSource, NormalizedEvent, RawEvent};
use rusqlite::{params, Connection};
use serde_json::{json, Value};

pub const AUTOMATION_STATUS_ACTIVE: &str = "active";
pub const AUTOMATION_STATUS_DISABLED: &str = "disabled";
pub const AUTOMATION_STATUS_FAILED: &str = "failed";

#[derive(Debug, Clone, PartialEq)]
pub struct StoredSuggestion {
    pub suggestion_id: i64,
    pub pattern_id: i64,
    pub signature: String,
    pub count: usize,
    pub avg_duration_ms: i64,
    pub canonical_summary: String,
    pub proposal_text: String,
    pub usefulness_score: f64,
    pub freshness: String,
    pub last_seen_at: String,
    pub created_at: String,
    pub shown_count: u32,
    pub accepted_count: u32,
    pub rejected_count: u32,
    pub snoozed_count: u32,
    pub last_shown_ts: Option<String>,
    pub last_accepted_ts: Option<String>,
    pub last_rejected_ts: Option<String>,
    pub last_snoozed_ts: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSuggestionHistory {
    pub signature: String,
    pub shown_count: u32,
    pub accepted_count: u32,
    pub rejected_count: u32,
    pub snoozed_count: u32,
    pub last_shown_ts: Option<String>,
    pub last_accepted_ts: Option<String>,
    pub last_rejected_ts: Option<String>,
    pub last_snoozed_ts: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSuggestionRecord {
    pub suggestion_id: i64,
    pub pattern_id: i64,
    pub status: String,
    pub signature: String,
    pub canonical_summary: String,
    pub proposal_text: String,
    pub shown_count: u32,
    pub accepted_count: u32,
    pub rejected_count: u32,
    pub snoozed_count: u32,
    pub last_shown_ts: Option<String>,
    pub last_accepted_ts: Option<String>,
    pub last_rejected_ts: Option<String>,
    pub last_snoozed_ts: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SuggestionDetails {
    pub suggestion_id: i64,
    pub pattern_id: i64,
    pub status: String,
    pub signature: String,
    pub canonical_summary: String,
    pub proposal_text: String,
    pub shown_count: u32,
    pub accepted_count: u32,
    pub rejected_count: u32,
    pub snoozed_count: u32,
    pub last_shown_ts: Option<String>,
    pub last_accepted_ts: Option<String>,
    pub last_rejected_ts: Option<String>,
    pub last_snoozed_ts: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StoredRawEvent {
    pub id: i64,
    pub event: RawEvent,
}

#[derive(Debug, Clone)]
pub struct StoredNormalizedEvent {
    pub id: i64,
    pub event: NormalizedEvent,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredPattern {
    pub pattern_id: i64,
    pub signature: String,
    pub count: usize,
    pub avg_duration_ms: i64,
    pub canonical_summary: String,
    pub usefulness_score: f64,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSession {
    pub session_id: i64,
    pub start_ts: String,
    pub end_ts: String,
    pub event_count: usize,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAutomation {
    pub automation_id: i64,
    pub suggestion_id: Option<i64>,
    pub status: String,
    pub accepted_at: Option<String>,
    pub run_count: usize,
    pub last_run_result: Option<String>,
    pub last_run_finished_at: Option<String>,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub struct StoredAutomationSpec {
    pub automation_id: i64,
    pub suggestion_id: Option<i64>,
    pub status: String,
    pub accepted_at: Option<String>,
    pub summary: String,
    pub spec_yaml: String,
}

#[derive(Debug, Clone)]
pub struct AutomationRunRecord<'a> {
    pub automation_id: i64,
    pub started_at: &'a str,
    pub finished_at: &'a str,
    pub result: &'a str,
    pub undo_payload_json: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAutomationRun {
    pub run_id: i64,
    pub automation_id: i64,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub result: String,
    pub undo_payload_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalUsageStats {
    pub pattern_count: usize,
    pub suggestion_count: usize,
    pub approved_automation_count: usize,
    pub automation_run_count: usize,
    pub undo_run_count: usize,
    pub estimated_time_saved_ms: i64,
}

pub fn insert_raw_event(conn: &Connection, event: &RawEvent) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO raw_events (ts, source, payload_json) VALUES (?1, ?2, ?3)",
        params![
            event.ts.to_rfc3339(),
            format!("{:?}", event.source),
            serde_json::to_string(&event.payload).unwrap()
        ],
    )
}

fn insert_normalized_event_row(
    conn: &Connection,
    event: &NormalizedEvent,
) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO normalized_events (ts, action_type, app, target, metadata_json, raw_event_id) VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
        params![
            event.ts.to_rfc3339(),
            format!("{:?}", event.action_type),
            event.app,
            event.target,
            serde_json::to_string(&event.metadata).unwrap()
        ],
    )
}

pub fn insert_normalized_event_record(
    conn: &mut Connection,
    event: &NormalizedEvent,
) -> rusqlite::Result<i64> {
    insert_normalized_event_row(conn, event)?;
    Ok(conn.last_insert_rowid())
}

fn insert_normalized_event_for_raw_event_row(
    conn: &Connection,
    raw_event_id: i64,
    event: &NormalizedEvent,
) -> rusqlite::Result<bool> {
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO normalized_events (ts, action_type, app, target, metadata_json, raw_event_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            event.ts.to_rfc3339(),
            format!("{:?}", event.action_type),
            event.app,
            event.target,
            serde_json::to_string(&event.metadata).unwrap(),
            raw_event_id,
        ],
    )?;

    Ok(inserted == 1)
}

pub fn insert_normalized_event_for_raw_event(
    conn: &mut Connection,
    raw_event_id: i64,
    event: &NormalizedEvent,
) -> rusqlite::Result<bool> {
    insert_normalized_event_for_raw_event_row(conn, raw_event_id, event)
}

pub fn insert_normalized_events_for_raw_events(
    conn: &mut Connection,
    records: &[(i64, NormalizedEvent)],
) -> rusqlite::Result<usize> {
    let mut inserted_count = 0usize;

    for (raw_event_id, event) in records {
        if insert_normalized_event_for_raw_event_row(conn, *raw_event_id, event)? {
            inserted_count += 1;
        }
    }

    Ok(inserted_count)
}

pub fn list_pending_file_raw_events(conn: &Connection) -> rusqlite::Result<Vec<StoredRawEvent>> {
    let mut statement = conn.prepare(
        r#"
        SELECT raw_events.id, raw_events.ts, raw_events.source, raw_events.payload_json
        FROM raw_events
        LEFT JOIN normalized_events ON normalized_events.raw_event_id = raw_events.id
        WHERE raw_events.source = ?1
            AND normalized_events.raw_event_id IS NULL
            AND (
                raw_events.payload_json LIKE '%"kind":"create"%'
                OR raw_events.payload_json LIKE '%"kind":"rename"%'
                OR raw_events.payload_json LIKE '%"kind":"move"%'
            )
        ORDER BY raw_events.id ASC
        "#,
    )?;

    let rows = statement.query_map([format!("{:?}", EventSource::FileWatcher)], |row| {
        let ts: String = row.get(1)?;
        let source: String = row.get(2)?;
        let payload_json: String = row.get(3)?;

        Ok(StoredRawEvent {
            id: row.get(0)?,
            event: RawEvent {
                ts: parse_timestamp(&ts)?,
                source: parse_event_source(&source)?,
                payload: parse_json_value(&payload_json)?,
            },
        })
    })?;

    rows.collect()
}

pub fn list_pending_observation_raw_events(
    conn: &Connection,
) -> rusqlite::Result<Vec<StoredRawEvent>> {
    let mut statement = conn.prepare(
        r#"
        SELECT raw_events.id, raw_events.ts, raw_events.source, raw_events.payload_json
        FROM raw_events
        LEFT JOIN normalized_events ON normalized_events.raw_event_id = raw_events.id
        WHERE normalized_events.raw_event_id IS NULL
            AND (
                (
                    raw_events.source = ?1
                    AND (
                        raw_events.payload_json LIKE '%"kind":"create"%'
                        OR raw_events.payload_json LIKE '%"kind":"rename"%'
                        OR raw_events.payload_json LIKE '%"kind":"move"%'
                    )
                )
                OR raw_events.source = ?2
            )
        ORDER BY raw_events.id ASC
        "#,
    )?;

    let rows = statement.query_map(
        [
            format!("{:?}", EventSource::FileWatcher),
            format!("{:?}", EventSource::Terminal),
        ],
        |row| {
            let ts: String = row.get(1)?;
            let source: String = row.get(2)?;
            let payload_json: String = row.get(3)?;

            Ok(StoredRawEvent {
                id: row.get(0)?,
                event: RawEvent {
                    ts: parse_timestamp(&ts)?,
                    source: parse_event_source(&source)?,
                    payload: parse_json_value(&payload_json)?,
                },
            })
        },
    )?;

    rows.collect()
}

pub fn list_normalized_events(conn: &Connection) -> rusqlite::Result<Vec<StoredNormalizedEvent>> {
    let mut statement = conn.prepare(
        r#"
        SELECT id, ts, action_type, app, target, metadata_json
        FROM normalized_events
        ORDER BY ts ASC, id ASC
        "#,
    )?;

    let rows = statement.query_map([], |row| {
        let ts: String = row.get(1)?;
        let action_type: String = row.get(2)?;
        let metadata_json: String = row.get(5)?;

        Ok(StoredNormalizedEvent {
            id: row.get(0)?,
            event: NormalizedEvent {
                ts: parse_timestamp(&ts)?,
                action_type: parse_action_type(&action_type)?,
                app: row.get(3)?,
                target: row.get(4)?,
                metadata: parse_json_value(&metadata_json)?,
            },
        })
    })?;

    rows.collect()
}

pub fn insert_session(
    conn: &Connection,
    start_ts: &str,
    end_ts: &str,
    event_ids: &[i64],
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO sessions (start_ts, end_ts, session_type) VALUES (?1, ?2, ?3)",
        params![start_ts, end_ts, "file_workflow"],
    )?;
    let session_id = conn.last_insert_rowid();

    for (ord, event_id) in event_ids.iter().enumerate() {
        conn.execute(
            "INSERT INTO session_events (session_id, event_id, ord) VALUES (?1, ?2, ?3)",
            params![session_id, event_id, ord as i64],
        )?;
    }

    Ok(session_id)
}

pub fn insert_pattern(
    conn: &Connection,
    signature: &str,
    count: usize,
    avg_duration_ms: i64,
    canonical_summary: &str,
    last_seen_at: &str,
    safety_score: f64,
    usefulness_score: f64,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO patterns (signature, count, avg_duration_ms, canonical_summary, confidence, last_seen_at, safety_score, is_active) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)",
        params![
            signature,
            count as i64,
            avg_duration_ms,
            canonical_summary,
            usefulness_score,
            last_seen_at,
            safety_score
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn clear_analysis_state(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM suggestions", [])?;
    conn.execute("DELETE FROM patterns", [])?;
    conn.execute("DELETE FROM session_events", [])?;
    conn.execute("DELETE FROM sessions", [])?;
    Ok(())
}

pub fn clear_session_state(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM session_events", [])?;
    conn.execute("DELETE FROM sessions", [])?;
    Ok(())
}

pub fn insert_suggestion(
    conn: &Connection,
    pattern_id: i64,
    proposal_text: &str,
    created_at: &str,
    usefulness_score: f64,
) -> rusqlite::Result<i64> {
    let proposal_json = json!({
        "kind": "file_workflow",
        "message": proposal_text,
    });
    conn.execute(
        "INSERT INTO suggestions (pattern_id, status, proposal_json, created_at, usefulness_score, freshness) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            pattern_id,
            "pending",
            proposal_json.to_string(),
            created_at,
            usefulness_score,
            "current"
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn upsert_pattern(
    conn: &Connection,
    signature: &str,
    count: usize,
    avg_duration_ms: i64,
    canonical_summary: &str,
    last_seen_at: &str,
    safety_score: f64,
    usefulness_score: f64,
) -> rusqlite::Result<i64> {
    let existing = conn.query_row(
        "SELECT id FROM patterns WHERE signature = ?1",
        [signature],
        |row| row.get::<_, i64>(0),
    );

    match existing {
        Ok(pattern_id) => {
            conn.execute(
                "UPDATE patterns SET count = ?2, avg_duration_ms = ?3, canonical_summary = ?4, confidence = ?5, last_seen_at = ?6, safety_score = ?7, is_active = 1 WHERE id = ?1",
                params![
                    pattern_id,
                    count as i64,
                    avg_duration_ms,
                    canonical_summary,
                    usefulness_score,
                    last_seen_at,
                    safety_score
                ],
            )?;
            Ok(pattern_id)
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => insert_pattern(
            conn,
            signature,
            count,
            avg_duration_ms,
            canonical_summary,
            last_seen_at,
            safety_score,
            usefulness_score,
        ),
        Err(error) => Err(error),
    }
}

pub fn sync_suggestion_for_pattern(
    conn: &Connection,
    pattern_id: i64,
    proposal_text: &str,
    created_at: &str,
    usefulness_score: f64,
) -> rusqlite::Result<()> {
    let approved_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM suggestions WHERE pattern_id = ?1 AND status = 'approved'",
        [pattern_id],
        |row| row.get(0),
    )?;
    let proposal_json = json!({
        "kind": "file_workflow",
        "message": proposal_text,
    })
    .to_string();

    if approved_count > 0 {
        conn.execute(
            "UPDATE suggestions SET freshness = 'stale', usefulness_score = ?2 WHERE pattern_id = ?1 AND status != 'approved'",
            params![pattern_id, usefulness_score],
        )?;
        return Ok(());
    }

    let mut statement = conn.prepare(
        "SELECT id FROM suggestions WHERE pattern_id = ?1 AND status != 'approved' ORDER BY id ASC",
    )?;
    let suggestion_ids = statement
        .query_map([pattern_id], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    if let Some(primary_id) = suggestion_ids.first().copied() {
        conn.execute(
            "UPDATE suggestions SET status = 'pending', proposal_json = ?2, usefulness_score = ?3, freshness = 'current' WHERE id = ?1",
            params![primary_id, proposal_json, usefulness_score],
        )?;
        for duplicate_id in suggestion_ids.into_iter().skip(1) {
            conn.execute(
                "UPDATE suggestions SET freshness = 'stale', usefulness_score = ?2 WHERE id = ?1",
                params![duplicate_id, usefulness_score],
            )?;
        }
        return Ok(());
    }

    insert_suggestion(
        conn,
        pattern_id,
        proposal_text,
        created_at,
        usefulness_score,
    )?;
    Ok(())
}

pub fn suppress_suggestions_for_pattern(
    conn: &Connection,
    pattern_id: i64,
    usefulness_score: f64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE suggestions SET freshness = 'stale', usefulness_score = ?2 WHERE pattern_id = ?1 AND status != 'approved'",
        params![pattern_id, usefulness_score],
    )?;
    Ok(())
}

pub fn mark_stale_patterns_and_suggestions(
    conn: &Connection,
    active_pattern_ids: &[i64],
) -> rusqlite::Result<()> {
    if active_pattern_ids.is_empty() {
        conn.execute("UPDATE patterns SET is_active = 0", [])?;
        conn.execute(
            "UPDATE suggestions SET freshness = 'stale' WHERE status = 'pending'",
            [],
        )?;
        return Ok(());
    }

    let placeholders = std::iter::repeat_n("?", active_pattern_ids.len())
        .collect::<Vec<_>>()
        .join(", ");

    conn.execute(
        &format!("UPDATE patterns SET is_active = 0 WHERE id NOT IN ({placeholders})"),
        rusqlite::params_from_iter(active_pattern_ids.iter()),
    )?;
    conn.execute(
        &format!("UPDATE patterns SET is_active = 1 WHERE id IN ({placeholders})"),
        rusqlite::params_from_iter(active_pattern_ids.iter()),
    )?;
    conn.execute(
        &format!(
            "UPDATE suggestions SET freshness = 'stale' WHERE status = 'pending' AND pattern_id NOT IN ({placeholders})"
        ),
        rusqlite::params_from_iter(active_pattern_ids.iter()),
    )?;
    Ok(())
}

pub fn list_suggestions(conn: &Connection) -> rusqlite::Result<Vec<StoredSuggestion>> {
    let mut statement = conn.prepare(
        r#"
        SELECT
            suggestions.id,
            patterns.id,
            patterns.signature,
            patterns.count,
            patterns.avg_duration_ms,
            COALESCE(patterns.canonical_summary, ''),
            suggestions.proposal_json,
            suggestions.usefulness_score,
            suggestions.freshness,
            COALESCE(patterns.last_seen_at, ''),
            suggestions.created_at,
            suggestions.shown_count,
            suggestions.accepted_count,
            suggestions.rejected_count,
            suggestions.snoozed_count,
            suggestions.last_shown_ts,
            suggestions.last_accepted_ts,
            suggestions.last_rejected_ts,
            suggestions.last_snoozed_ts
        FROM suggestions
        INNER JOIN patterns ON patterns.id = suggestions.pattern_id
        WHERE suggestions.status = 'pending'
            AND suggestions.freshness = 'current'
            AND patterns.is_active = 1
        ORDER BY suggestions.usefulness_score DESC, patterns.count DESC, patterns.signature ASC, suggestions.created_at ASC
        "#,
    )?;

    let rows = statement.query_map([], |row| {
        let proposal_json: String = row.get(6)?;
        let proposal: Value = serde_json::from_str(&proposal_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                proposal_json.len(),
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;

        Ok(StoredSuggestion {
            suggestion_id: row.get(0)?,
            pattern_id: row.get(1)?,
            signature: row.get(2)?,
            count: row.get::<_, i64>(3)? as usize,
            avg_duration_ms: row.get(4)?,
            canonical_summary: row.get(5)?,
            proposal_text: proposal
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string(),
            usefulness_score: row.get(7)?,
            freshness: row.get(8)?,
            last_seen_at: row.get(9)?,
            created_at: row.get(10)?,
            shown_count: row.get::<_, i64>(11)? as u32,
            accepted_count: row.get::<_, i64>(12)? as u32,
            rejected_count: row.get::<_, i64>(13)? as u32,
            snoozed_count: row.get::<_, i64>(14)? as u32,
            last_shown_ts: row.get(15)?,
            last_accepted_ts: row.get(16)?,
            last_rejected_ts: row.get(17)?,
            last_snoozed_ts: row.get(18)?,
        })
    })?;

    rows.collect()
}

pub fn list_suggestion_histories(
    conn: &Connection,
) -> rusqlite::Result<Vec<StoredSuggestionHistory>> {
    let mut statement = conn.prepare(
        r#"
        SELECT
            patterns.signature,
            suggestions.shown_count,
            suggestions.accepted_count,
            suggestions.rejected_count,
            suggestions.snoozed_count,
            suggestions.last_shown_ts,
            suggestions.last_accepted_ts,
            suggestions.last_rejected_ts,
            suggestions.last_snoozed_ts
        FROM suggestions
        INNER JOIN patterns ON patterns.id = suggestions.pattern_id
        ORDER BY patterns.signature ASC, suggestions.id ASC
        "#,
    )?;

    let rows = statement.query_map([], |row| {
        Ok(StoredSuggestionHistory {
            signature: row.get(0)?,
            shown_count: row.get::<_, i64>(1)? as u32,
            accepted_count: row.get::<_, i64>(2)? as u32,
            rejected_count: row.get::<_, i64>(3)? as u32,
            snoozed_count: row.get::<_, i64>(4)? as u32,
            last_shown_ts: row.get(5)?,
            last_accepted_ts: row.get(6)?,
            last_rejected_ts: row.get(7)?,
            last_snoozed_ts: row.get(8)?,
        })
    })?;

    let mut aggregated: Vec<StoredSuggestionHistory> = Vec::new();

    for history in rows {
        let history = history?;
        if let Some(current) = aggregated.last_mut() {
            if current.signature == history.signature {
                current.shown_count += history.shown_count;
                current.accepted_count += history.accepted_count;
                current.rejected_count += history.rejected_count;
                current.snoozed_count += history.snoozed_count;
                current.last_shown_ts =
                    latest_timestamp(current.last_shown_ts.take(), history.last_shown_ts);
                current.last_accepted_ts =
                    latest_timestamp(current.last_accepted_ts.take(), history.last_accepted_ts);
                current.last_rejected_ts =
                    latest_timestamp(current.last_rejected_ts.take(), history.last_rejected_ts);
                current.last_snoozed_ts =
                    latest_timestamp(current.last_snoozed_ts.take(), history.last_snoozed_ts);
                continue;
            }
        }

        aggregated.push(history);
    }

    Ok(aggregated)
}

pub fn get_suggestion(
    conn: &Connection,
    suggestion_id: i64,
) -> rusqlite::Result<Option<SuggestionDetails>> {
    let mut statement = conn.prepare(
        r#"
        SELECT
            suggestions.id,
            suggestions.pattern_id,
            suggestions.status,
            patterns.signature,
            COALESCE(patterns.canonical_summary, ''),
            suggestions.proposal_json,
            suggestions.shown_count,
            suggestions.accepted_count,
            suggestions.rejected_count,
            suggestions.snoozed_count,
            suggestions.last_shown_ts,
            suggestions.last_accepted_ts,
            suggestions.last_rejected_ts,
            suggestions.last_snoozed_ts
        FROM suggestions
        INNER JOIN patterns ON patterns.id = suggestions.pattern_id
        WHERE suggestions.id = ?1
        "#,
    )?;

    let row = statement.query_row([suggestion_id], |row| {
        let proposal_json: String = row.get(5)?;
        let proposal: Value = serde_json::from_str(&proposal_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                proposal_json.len(),
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;

        Ok(SuggestionDetails {
            suggestion_id: row.get(0)?,
            pattern_id: row.get(1)?,
            status: row.get(2)?,
            signature: row.get(3)?,
            canonical_summary: row.get(4)?,
            proposal_text: proposal
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string(),
            shown_count: row.get::<_, i64>(6)? as u32,
            accepted_count: row.get::<_, i64>(7)? as u32,
            rejected_count: row.get::<_, i64>(8)? as u32,
            snoozed_count: row.get::<_, i64>(9)? as u32,
            last_shown_ts: row.get(10)?,
            last_accepted_ts: row.get(11)?,
            last_rejected_ts: row.get(12)?,
            last_snoozed_ts: row.get(13)?,
        })
    });

    match row {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(error),
    }
}

pub fn list_all_suggestion_records(
    conn: &Connection,
) -> rusqlite::Result<Vec<StoredSuggestionRecord>> {
    let mut statement = conn.prepare(
        r#"
        SELECT
            suggestions.id,
            suggestions.pattern_id,
            suggestions.status,
            patterns.signature,
            COALESCE(patterns.canonical_summary, ''),
            suggestions.proposal_json,
            suggestions.shown_count,
            suggestions.accepted_count,
            suggestions.rejected_count,
            suggestions.snoozed_count,
            suggestions.last_shown_ts,
            suggestions.last_accepted_ts,
            suggestions.last_rejected_ts,
            suggestions.last_snoozed_ts
        FROM suggestions
        INNER JOIN patterns ON patterns.id = suggestions.pattern_id
        ORDER BY suggestions.id ASC
        "#,
    )?;

    let rows = statement.query_map([], |row| {
        let proposal_json: String = row.get(5)?;
        let proposal: Value = serde_json::from_str(&proposal_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                proposal_json.len(),
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;

        Ok(StoredSuggestionRecord {
            suggestion_id: row.get(0)?,
            pattern_id: row.get(1)?,
            status: row.get(2)?,
            signature: row.get(3)?,
            canonical_summary: row.get(4)?,
            proposal_text: proposal
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string(),
            shown_count: row.get::<_, i64>(6)? as u32,
            accepted_count: row.get::<_, i64>(7)? as u32,
            rejected_count: row.get::<_, i64>(8)? as u32,
            snoozed_count: row.get::<_, i64>(9)? as u32,
            last_shown_ts: row.get(10)?,
            last_accepted_ts: row.get(11)?,
            last_rejected_ts: row.get(12)?,
            last_snoozed_ts: row.get(13)?,
        })
    })?;

    rows.collect()
}

pub fn set_suggestion_status(
    conn: &Connection,
    suggestion_id: i64,
    status: &str,
) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE suggestions SET status = ?2 WHERE id = ?1",
        params![suggestion_id, status],
    )
}

pub fn increment_shown(conn: &Connection, suggestion_id: i64) -> rusqlite::Result<usize> {
    increment_feedback_counter(
        conn,
        suggestion_id,
        "shown_count",
        "last_shown_ts",
        &Utc::now().to_rfc3339(),
    )
}

pub fn increment_accepted(conn: &Connection, suggestion_id: i64) -> rusqlite::Result<usize> {
    increment_feedback_counter(
        conn,
        suggestion_id,
        "accepted_count",
        "last_accepted_ts",
        &Utc::now().to_rfc3339(),
    )
}

pub fn increment_rejected(conn: &Connection, suggestion_id: i64) -> rusqlite::Result<usize> {
    increment_feedback_counter(
        conn,
        suggestion_id,
        "rejected_count",
        "last_rejected_ts",
        &Utc::now().to_rfc3339(),
    )
}

pub fn increment_snoozed(conn: &Connection, suggestion_id: i64) -> rusqlite::Result<usize> {
    increment_feedback_counter(
        conn,
        suggestion_id,
        "snoozed_count",
        "last_snoozed_ts",
        &Utc::now().to_rfc3339(),
    )
}

pub fn insert_automation(
    conn: &Connection,
    suggestion_id: i64,
    spec_yaml: &str,
    status: &str,
    summary: &str,
    accepted_at: &str,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO automations (suggestion_id, spec_yaml, state, summary, accepted_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![suggestion_id, spec_yaml, status, summary, accepted_at],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn set_automation_status(
    conn: &Connection,
    automation_id: i64,
    status: &str,
) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE automations SET state = ?2 WHERE id = ?1",
        params![automation_id, status],
    )
}

pub fn list_automations(conn: &Connection) -> rusqlite::Result<Vec<StoredAutomation>> {
    let mut statement = conn.prepare(
        r#"
        SELECT
            automations.id,
            automations.suggestion_id,
            automations.state,
            automations.accepted_at,
            COALESCE(run_stats.run_count, 0),
            run_stats.last_run_result,
            run_stats.last_run_finished_at,
            COALESCE(automations.summary, '')
        FROM automations
        LEFT JOIN (
            SELECT
                automation_runs.automation_id,
                COUNT(*) AS run_count,
                (
                    SELECT result
                    FROM automation_runs AS latest
                    WHERE latest.automation_id = automation_runs.automation_id
                    ORDER BY latest.id DESC
                    LIMIT 1
                ) AS last_run_result,
                (
                    SELECT finished_at
                    FROM automation_runs AS latest
                    WHERE latest.automation_id = automation_runs.automation_id
                    ORDER BY latest.id DESC
                    LIMIT 1
                ) AS last_run_finished_at
            FROM automation_runs
            GROUP BY automation_runs.automation_id
        ) AS run_stats ON run_stats.automation_id = automations.id
        ORDER BY id ASC
        "#,
    )?;

    let rows = statement.query_map([], |row| {
        Ok(StoredAutomation {
            automation_id: row.get(0)?,
            suggestion_id: row.get(1)?,
            status: row.get(2)?,
            accepted_at: row.get(3)?,
            run_count: row.get::<_, i64>(4)? as usize,
            last_run_result: row.get(5)?,
            last_run_finished_at: row.get(6)?,
            summary: row.get(7)?,
        })
    })?;

    rows.collect()
}

pub fn get_automation(
    conn: &Connection,
    automation_id: i64,
) -> rusqlite::Result<Option<StoredAutomationSpec>> {
    let mut statement = conn.prepare(
        r#"
        SELECT id, suggestion_id, state, accepted_at, COALESCE(summary, ''), spec_yaml
        FROM automations
        WHERE id = ?1
        "#,
    )?;

    let row = statement.query_row([automation_id], |row| {
        Ok(StoredAutomationSpec {
            automation_id: row.get(0)?,
            suggestion_id: row.get(1)?,
            status: row.get(2)?,
            accepted_at: row.get(3)?,
            summary: row.get(4)?,
            spec_yaml: row.get(5)?,
        })
    });

    match row {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(error),
    }
}

pub fn insert_automation_run(
    conn: &Connection,
    record: &AutomationRunRecord<'_>,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO automation_runs (automation_id, started_at, finished_at, result, undo_payload_json) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            record.automation_id,
            record.started_at,
            record.finished_at,
            record.result,
            record.undo_payload_json
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_automation_runs(conn: &Connection) -> rusqlite::Result<Vec<StoredAutomationRun>> {
    let mut statement = conn.prepare(
        r#"
        SELECT id, automation_id, started_at, finished_at, result, undo_payload_json
        FROM automation_runs
        ORDER BY id DESC
        "#,
    )?;

    let rows = statement.query_map([], |row| {
        Ok(StoredAutomationRun {
            run_id: row.get(0)?,
            automation_id: row.get(1)?,
            started_at: row.get(2)?,
            finished_at: row.get(3)?,
            result: row.get(4)?,
            undo_payload_json: row.get(5)?,
        })
    })?;

    rows.collect()
}

pub fn load_local_usage_stats(conn: &Connection) -> rusqlite::Result<LocalUsageStats> {
    conn.query_row(
        r#"
        SELECT
            (SELECT COUNT(*) FROM patterns),
            (SELECT COUNT(*) FROM suggestions),
            (SELECT COUNT(*) FROM automations),
            (SELECT COUNT(*) FROM automation_runs WHERE result = 'completed'),
            (SELECT COUNT(*) FROM automation_runs WHERE result = 'undone'),
            COALESCE((
                SELECT SUM(patterns.avg_duration_ms)
                FROM automation_runs
                INNER JOIN automations ON automations.id = automation_runs.automation_id
                INNER JOIN suggestions ON suggestions.id = automations.suggestion_id
                INNER JOIN patterns ON patterns.id = suggestions.pattern_id
                WHERE automation_runs.result = 'completed'
            ), 0)
        "#,
        [],
        |row| {
            Ok(LocalUsageStats {
                pattern_count: row.get::<_, i64>(0)? as usize,
                suggestion_count: row.get::<_, i64>(1)? as usize,
                approved_automation_count: row.get::<_, i64>(2)? as usize,
                automation_run_count: row.get::<_, i64>(3)? as usize,
                undo_run_count: row.get::<_, i64>(4)? as usize,
                estimated_time_saved_ms: row.get(5)?,
            })
        },
    )
}

pub fn load_automation_run(
    conn: &Connection,
    run_id: i64,
) -> rusqlite::Result<Option<StoredAutomationRun>> {
    let mut statement = conn.prepare(
        r#"
        SELECT id, automation_id, started_at, finished_at, result, undo_payload_json
        FROM automation_runs
        WHERE id = ?1
        "#,
    )?;

    let row = statement.query_row([run_id], |row| {
        Ok(StoredAutomationRun {
            run_id: row.get(0)?,
            automation_id: row.get(1)?,
            started_at: row.get(2)?,
            finished_at: row.get(3)?,
            result: row.get(4)?,
            undo_payload_json: row.get(5)?,
        })
    });

    match row {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(error),
    }
}

pub fn load_example_events_for_pattern(
    conn: &Connection,
    pattern_id: i64,
) -> rusqlite::Result<Vec<NormalizedEvent>> {
    let signature: String = conn.query_row(
        "SELECT signature FROM patterns WHERE id = ?1",
        [pattern_id],
        |row| row.get(0),
    )?;

    let mut sessions = conn.prepare("SELECT id FROM sessions ORDER BY id ASC")?;
    let session_ids = sessions.query_map([], |row| row.get::<_, i64>(0))?;

    for session_id in session_ids {
        let events = load_session_events(conn, session_id?)?;
        if session_signature(&events) == signature {
            return Ok(events);
        }
    }

    Ok(Vec::new())
}

pub fn list_patterns(conn: &Connection) -> rusqlite::Result<Vec<StoredPattern>> {
    let mut statement = conn.prepare(
        r#"
        SELECT id, signature, count, avg_duration_ms, COALESCE(canonical_summary, ''), confidence, COALESCE(last_seen_at, '')
        FROM patterns
        WHERE is_active = 1
        ORDER BY confidence DESC, count DESC, signature ASC, id ASC
        "#,
    )?;

    let rows = statement.query_map([], |row| {
        Ok(StoredPattern {
            pattern_id: row.get(0)?,
            signature: row.get(1)?,
            count: row.get::<_, i64>(2)? as usize,
            avg_duration_ms: row.get(3)?,
            canonical_summary: row.get(4)?,
            usefulness_score: row.get(5)?,
            last_seen_at: row.get(6)?,
        })
    })?;

    rows.collect()
}

pub fn list_recent_sessions(
    conn: &Connection,
    limit: usize,
) -> rusqlite::Result<Vec<StoredSession>> {
    let limit = i64::try_from(limit)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let mut statement = conn.prepare(
        r#"
        SELECT
            sessions.id,
            sessions.start_ts,
            sessions.end_ts,
            COUNT(session_events.event_id) AS event_count,
            ((strftime('%s', sessions.end_ts) - strftime('%s', sessions.start_ts)) * 1000) AS duration_ms
        FROM sessions
        LEFT JOIN session_events ON session_events.session_id = sessions.id
        GROUP BY sessions.id, sessions.start_ts, sessions.end_ts
        ORDER BY sessions.id DESC
        LIMIT ?1
        "#,
    )?;

    let rows = statement.query_map([limit], |row| {
        Ok(StoredSession {
            session_id: row.get(0)?,
            start_ts: row.get(1)?,
            end_ts: row.get(2)?,
            event_count: row.get::<_, i64>(3)? as usize,
            duration_ms: row.get(4)?,
        })
    })?;

    rows.collect()
}

fn parse_timestamp(value: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                value.len(),
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })
}

fn latest_timestamp(current: Option<String>, next: Option<String>) -> Option<String> {
    match (current, next) {
        (Some(current), Some(next)) => Some(if next > current { next } else { current }),
        (Some(current), None) => Some(current),
        (None, Some(next)) => Some(next),
        (None, None) => None,
    }
}

fn increment_feedback_counter(
    conn: &Connection,
    suggestion_id: i64,
    counter_column: &str,
    timestamp_column: &str,
    timestamp: &str,
) -> rusqlite::Result<usize> {
    conn.execute(
        &format!(
            "UPDATE suggestions SET {counter_column} = {counter_column} + 1, {timestamp_column} = ?2 WHERE id = ?1"
        ),
        params![suggestion_id, timestamp],
    )
}

fn parse_event_source(value: &str) -> rusqlite::Result<EventSource> {
    match value {
        "FileWatcher" => Ok(EventSource::FileWatcher),
        "Clipboard" => Ok(EventSource::Clipboard),
        "Terminal" => Ok(EventSource::Terminal),
        "ActiveWindow" => Ok(EventSource::ActiveWindow),
        "Browser" => Ok(EventSource::Browser),
        _ => Err(rusqlite::Error::FromSqlConversionFailure(
            value.len(),
            rusqlite::types::Type::Text,
            format!("unsupported event source: {value}").into(),
        )),
    }
}

fn parse_action_type(value: &str) -> rusqlite::Result<ActionType> {
    match value {
        "OpenApp" => Ok(ActionType::OpenApp),
        "SwitchApp" => Ok(ActionType::SwitchApp),
        "CopyText" => Ok(ActionType::CopyText),
        "PasteText" => Ok(ActionType::PasteText),
        "RunCommand" => Ok(ActionType::RunCommand),
        "CreateFile" => Ok(ActionType::CreateFile),
        "RenameFile" => Ok(ActionType::RenameFile),
        "MoveFile" => Ok(ActionType::MoveFile),
        "VisitUrl" => Ok(ActionType::VisitUrl),
        "DownloadFile" => Ok(ActionType::DownloadFile),
        _ => Err(rusqlite::Error::FromSqlConversionFailure(
            value.len(),
            rusqlite::types::Type::Text,
            format!("unsupported action type: {value}").into(),
        )),
    }
}

fn parse_json_value(value: &str) -> rusqlite::Result<Value> {
    serde_json::from_str(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            value.len(),
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn load_session_events(
    conn: &Connection,
    session_id: i64,
) -> rusqlite::Result<Vec<NormalizedEvent>> {
    let mut statement = conn.prepare(
        r#"
        SELECT
            normalized_events.ts,
            normalized_events.action_type,
            normalized_events.app,
            normalized_events.target,
            normalized_events.metadata_json
        FROM session_events
        INNER JOIN normalized_events ON normalized_events.id = session_events.event_id
        WHERE session_events.session_id = ?1
        ORDER BY session_events.ord ASC
        "#,
    )?;

    let rows = statement.query_map([session_id], |row| {
        let ts: String = row.get(0)?;
        let action_type: String = row.get(1)?;
        let metadata_json: String = row.get(4)?;

        Ok(NormalizedEvent {
            ts: parse_timestamp(&ts)?,
            action_type: parse_action_type(&action_type)?,
            app: row.get(2)?,
            target: row.get(3)?,
            metadata: parse_json_value(&metadata_json)?,
        })
    })?;

    rows.collect()
}

fn session_signature(events: &[NormalizedEvent]) -> String {
    events
        .iter()
        .map(event_signature_part)
        .collect::<Vec<_>>()
        .join("->")
}

fn event_signature_part(event: &NormalizedEvent) -> String {
    let group = event
        .metadata
        .get("file_group")
        .and_then(|value| value.as_str())
        .unwrap_or("file");
    format!("{:?}:{group}", event.action_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::run_migrations;
    use chrono::Utc;
    use flow_adapters::file_watcher::{synthetic_file_event, FileEvent, FileEventKind};
    use flow_adapters::terminal::synthetic_terminal_history_event;
    use flow_core::events::EventSource;
    use flow_patterns::normalize::normalize;
    use tempfile::tempdir;

    #[test]
    fn inserts_raw_event_records() {
        let dir = tempdir().unwrap();
        let conn = crate::open_database(dir.path().join("flowd.db")).unwrap();
        let raw_event = synthetic_file_event(
            Utc::now(),
            FileEventKind::Create,
            dir.path().join("report.txt").display().to_string(),
            None,
        );

        let inserted = insert_raw_event(&conn, &raw_event).unwrap();
        assert_eq!(inserted, 1);

        let (source, payload_json): (String, String) = conn
            .query_row(
                "SELECT source, payload_json FROM raw_events ORDER BY id DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(source, format!("{:?}", EventSource::FileWatcher));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&payload_json).unwrap(),
            raw_event.payload
        );
    }

    #[test]
    fn automation_status_updates_are_persisted_and_queryable() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let automation_id = insert_automation(
            &conn,
            1,
            "id: test\ntrigger: {}\nactions: []\n",
            AUTOMATION_STATUS_ACTIVE,
            "Test automation",
            "2026-03-11T10:00:00Z",
        )
        .unwrap();

        set_automation_status(&conn, automation_id, AUTOMATION_STATUS_DISABLED).unwrap();
        let disabled = get_automation(&conn, automation_id).unwrap().unwrap();
        assert_eq!(disabled.status, AUTOMATION_STATUS_DISABLED);

        let listed = list_automations(&conn).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].status, AUTOMATION_STATUS_DISABLED);

        set_automation_status(&conn, automation_id, AUTOMATION_STATUS_FAILED).unwrap();
        let failed = get_automation(&conn, automation_id).unwrap().unwrap();
        assert_eq!(failed.status, AUTOMATION_STATUS_FAILED);
    }

    #[test]
    fn lists_pending_file_raw_events_without_normalized_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let pending_raw =
            synthetic_file_event(Utc::now(), FileEventKind::Create, "/tmp/report.txt", None);
        let completed_raw = synthetic_file_event(
            Utc::now(),
            FileEventKind::Move,
            "/tmp/archive/report.txt",
            Some("/tmp/report.txt".to_string()),
        );

        insert_raw_event(&conn, &pending_raw).unwrap();
        insert_raw_event(&conn, &completed_raw).unwrap();
        let completed_raw_id = conn.last_insert_rowid();
        let normalized = normalize(&completed_raw).unwrap();
        insert_normalized_event_for_raw_event(&mut conn, completed_raw_id, &normalized).unwrap();

        let pending = list_pending_file_raw_events(&conn).unwrap();

        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].event.ts, pending_raw.ts);
        assert_eq!(pending[0].event.source, pending_raw.source);
        assert_eq!(pending[0].event.payload, pending_raw.payload);
    }

    #[test]
    fn lists_pending_terminal_observation_events_without_normalized_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let terminal_raw = synthetic_terminal_history_event(
            Utc::now(),
            "/tmp/workspace",
            "mv inbox/report.txt archive/report.txt",
            Some(0),
        );
        insert_raw_event(&conn, &terminal_raw).unwrap();

        let pending = list_pending_observation_raw_events(&conn).unwrap();

        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].event.source, EventSource::Terminal);
        assert_eq!(pending[0].event.payload, terminal_raw.payload);

        let normalized = normalize(&terminal_raw).unwrap();
        let terminal_raw_id = conn.last_insert_rowid();
        insert_normalized_event_for_raw_event(&mut conn, terminal_raw_id, &normalized).unwrap();

        assert!(list_pending_observation_raw_events(&conn)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn ignores_duplicate_normalized_rows_for_the_same_raw_event() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let raw_event = synthetic_file_event(
            Utc::now(),
            FileEventKind::Rename,
            "/tmp/report-final.txt",
            Some("/tmp/report.txt".to_string()),
        );

        insert_raw_event(&conn, &raw_event).unwrap();
        let raw_event_id = conn.last_insert_rowid();
        let normalized = normalize(&raw_event).unwrap();

        let first_insert =
            insert_normalized_event_for_raw_event(&mut conn, raw_event_id, &normalized).unwrap();
        let second_insert =
            insert_normalized_event_for_raw_event(&mut conn, raw_event_id, &normalized).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM normalized_events", [], |row| {
                row.get(0)
            })
            .unwrap();

        assert!(first_insert);
        assert!(!second_insert);
        assert_eq!(count, 1);
    }

    #[test]
    fn clear_analysis_state_removes_sessions_patterns_and_suggestions() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let fixture_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/invoice_file_events.json"
        );
        let raw_fixture = std::fs::read_to_string(fixture_path).unwrap();
        let file_events: Vec<FileEvent> = serde_json::from_str(&raw_fixture).unwrap();
        let raw_events: Vec<_> = file_events
            .into_iter()
            .map(FileEvent::into_raw_event)
            .collect();
        let normalized: Vec<_> = raw_events.iter().filter_map(normalize).collect();

        for event in &normalized {
            insert_normalized_event_record(&mut conn, event).unwrap();
        }

        clear_analysis_state(&conn).unwrap();

        let session_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
            .unwrap();
        let session_event_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM session_events", [], |row| row.get(0))
            .unwrap();
        let pattern_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM patterns", [], |row| row.get(0))
            .unwrap();
        let suggestion_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM suggestions", [], |row| row.get(0))
            .unwrap();
        let normalized_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM normalized_events", [], |row| {
                row.get(0)
            })
            .unwrap();

        assert_eq!(session_count, 0);
        assert_eq!(session_event_count, 0);
        assert_eq!(pattern_count, 0);
        assert_eq!(suggestion_count, 0);
        assert_eq!(normalized_count, normalized.len() as i64);
    }

    #[test]
    fn feedback_history_updates_counts_and_timestamps() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        let pattern_id = insert_pattern(
            &conn,
            "CreateFile:invoice",
            2,
            30_000,
            "CreateFile -> RenameFile",
            "2026-03-11T09:00:00Z",
            1.0,
            0.8,
        )
        .unwrap();
        let suggestion_id = insert_suggestion(
            &conn,
            pattern_id,
            "Repeated invoice file workflow detected",
            "2026-03-11T10:00:00Z",
            0.8,
        )
        .unwrap();

        increment_shown(&conn, suggestion_id).unwrap();
        increment_accepted(&conn, suggestion_id).unwrap();
        increment_rejected(&conn, suggestion_id).unwrap();
        increment_snoozed(&conn, suggestion_id).unwrap();

        let stored = get_suggestion(&conn, suggestion_id).unwrap().unwrap();
        assert_eq!(stored.shown_count, 1);
        assert_eq!(stored.accepted_count, 1);
        assert_eq!(stored.rejected_count, 1);
        assert_eq!(stored.snoozed_count, 1);
        assert!(stored.last_shown_ts.is_some());
        assert!(stored.last_accepted_ts.is_some());
        assert!(stored.last_rejected_ts.is_some());
        assert!(stored.last_snoozed_ts.is_some());

        let listed = list_suggestions(&conn).unwrap().remove(0);
        assert_eq!(listed.shown_count, 1);
        assert_eq!(listed.accepted_count, 1);
        assert_eq!(listed.rejected_count, 1);
        assert_eq!(listed.snoozed_count, 1);
        assert_eq!(listed.last_shown_ts, stored.last_shown_ts);
        assert_eq!(listed.last_accepted_ts, stored.last_accepted_ts);
        assert_eq!(listed.last_rejected_ts, stored.last_rejected_ts);
        assert_eq!(listed.last_snoozed_ts, stored.last_snoozed_ts);
    }

    #[test]
    fn list_suggestion_histories_aggregates_feedback_by_signature() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        let pattern_id = insert_pattern(
            &conn,
            "CreateFile:invoice",
            2,
            30_000,
            "CreateFile -> RenameFile",
            "2026-03-11T09:00:00Z",
            1.0,
            0.8,
        )
        .unwrap();
        let first_id = insert_suggestion(
            &conn,
            pattern_id,
            "Repeated invoice file workflow detected",
            "2026-03-11T10:00:00Z",
            0.8,
        )
        .unwrap();
        conn.execute(
            "UPDATE suggestions SET freshness = 'stale' WHERE id = ?1",
            [first_id],
        )
        .unwrap();
        let second_id = insert_suggestion(
            &conn,
            pattern_id,
            "Repeated invoice file workflow detected again",
            "2026-03-11T11:00:00Z",
            0.7,
        )
        .unwrap();

        conn.execute(
            "UPDATE suggestions
             SET shown_count = 2,
                 accepted_count = 1,
                 rejected_count = 0,
                 snoozed_count = 1,
                 last_shown_ts = '2026-03-11T10:10:00+00:00',
                 last_accepted_ts = '2026-03-11T10:20:00+00:00',
                 last_snoozed_ts = '2026-03-11T10:30:00+00:00'
             WHERE id = ?1",
            [first_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE suggestions
             SET shown_count = 3,
                 accepted_count = 0,
                 rejected_count = 4,
                 snoozed_count = 2,
                 last_shown_ts = '2026-03-11T11:10:00+00:00',
                 last_rejected_ts = '2026-03-11T11:20:00+00:00',
                 last_snoozed_ts = '2026-03-11T11:30:00+00:00'
             WHERE id = ?1",
            [second_id],
        )
        .unwrap();

        let histories = list_suggestion_histories(&conn).unwrap();

        assert_eq!(
            histories,
            vec![StoredSuggestionHistory {
                signature: "CreateFile:invoice".to_string(),
                shown_count: 5,
                accepted_count: 1,
                rejected_count: 4,
                snoozed_count: 3,
                last_shown_ts: Some("2026-03-11T11:10:00+00:00".to_string()),
                last_accepted_ts: Some("2026-03-11T10:20:00+00:00".to_string()),
                last_rejected_ts: Some("2026-03-11T11:20:00+00:00".to_string()),
                last_snoozed_ts: Some("2026-03-11T11:30:00+00:00".to_string()),
            }]
        );
    }

    #[test]
    fn local_usage_stats_are_zero_for_an_empty_database() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let stats = load_local_usage_stats(&conn).unwrap();

        assert_eq!(
            stats,
            LocalUsageStats {
                pattern_count: 0,
                suggestion_count: 0,
                approved_automation_count: 0,
                automation_run_count: 0,
                undo_run_count: 0,
                estimated_time_saved_ms: 0,
            }
        );
    }

    #[test]
    fn local_usage_stats_aggregate_patterns_suggestions_approvals_and_runs() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let invoice_pattern_id = insert_pattern(
            &conn,
            "CreateFile:invoice",
            3,
            30_000,
            "CreateFile -> RenameFile",
            "2026-03-11T09:00:00Z",
            1.0,
            0.8,
        )
        .unwrap();
        let report_pattern_id = insert_pattern(
            &conn,
            "CreateFile:report",
            2,
            45_000,
            "CreateFile -> MoveFile",
            "2026-03-11T10:00:00Z",
            1.0,
            0.7,
        )
        .unwrap();

        let invoice_suggestion_id = insert_suggestion(
            &conn,
            invoice_pattern_id,
            "Repeated invoice file workflow detected",
            "2026-03-11T10:00:00Z",
            0.8,
        )
        .unwrap();
        insert_suggestion(
            &conn,
            report_pattern_id,
            "Repeated report file workflow detected",
            "2026-03-11T11:00:00Z",
            0.7,
        )
        .unwrap();

        let automation_id = insert_automation(
            &conn,
            invoice_suggestion_id,
            "id: invoice\ntrigger: {}\nactions: []\n",
            AUTOMATION_STATUS_ACTIVE,
            "Invoice automation",
            "2026-03-11T12:00:00Z",
        )
        .unwrap();

        insert_automation_run(
            &conn,
            &AutomationRunRecord {
                automation_id,
                started_at: "2026-03-11T12:10:00Z",
                finished_at: "2026-03-11T12:10:05Z",
                result: "completed",
                undo_payload_json: Some("{\"operations\":[]}"),
            },
        )
        .unwrap();
        insert_automation_run(
            &conn,
            &AutomationRunRecord {
                automation_id,
                started_at: "2026-03-11T12:20:00Z",
                finished_at: "2026-03-11T12:20:01Z",
                result: "dry_run",
                undo_payload_json: Some("{\"operations\":[]}"),
            },
        )
        .unwrap();
        insert_automation_run(
            &conn,
            &AutomationRunRecord {
                automation_id,
                started_at: "2026-03-11T12:30:00Z",
                finished_at: "2026-03-11T12:30:02Z",
                result: "undone",
                undo_payload_json: Some("{\"kind\":\"undo\"}"),
            },
        )
        .unwrap();

        let stats = load_local_usage_stats(&conn).unwrap();

        assert_eq!(
            stats,
            LocalUsageStats {
                pattern_count: 2,
                suggestion_count: 2,
                approved_automation_count: 1,
                automation_run_count: 1,
                undo_run_count: 1,
                estimated_time_saved_ms: 30_000,
            }
        );
    }
}
