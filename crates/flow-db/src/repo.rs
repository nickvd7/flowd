use flow_core::events::{NormalizedEvent, RawEvent};
use flow_patterns::{
    detect::{detect_repeated_patterns, PatternCandidate},
    normalize::normalize,
    sessions::{split_into_sessions, EventSession},
};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct StoredRawEvent {
    pub ts: chrono::DateTime<chrono::Utc>,
    pub source: String,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredPattern {
    pub pattern_id: i64,
    pub signature: String,
    pub count: usize,
    pub avg_duration_ms: i64,
    pub canonical_summary: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredSuggestion {
    pub pattern_id: i64,
    pub signature: String,
    pub count: usize,
    pub avg_duration_ms: i64,
    pub canonical_summary: String,
    pub proposal_text: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredNormalizedEvent {
    pub id: i64,
    pub event: NormalizedEvent,
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

pub fn insert_normalized_event(
    conn: &Connection,
    event: &NormalizedEvent,
) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO normalized_events (ts, action_type, app, target, metadata_json) VALUES (?1, ?2, ?3, ?4, ?5)",
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
    conn: &Connection,
    event: &NormalizedEvent,
) -> rusqlite::Result<i64> {
    insert_normalized_event(conn, event)?;
    Ok(conn.last_insert_rowid())
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
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO patterns (signature, count, avg_duration_ms, canonical_summary, confidence) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![signature, count as i64, avg_duration_ms, canonical_summary, count as f64],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn ingest_raw_event(conn: &Connection, event: &RawEvent) -> rusqlite::Result<()> {
    insert_raw_event(conn, event)?;

    if let Some(normalized_event) = normalize(event) {
        insert_normalized_event(conn, &normalized_event)?;
        refresh_suggestions(conn)?;
    }

    Ok(())
}

pub fn insert_suggestion(
    conn: &Connection,
    pattern_id: i64,
    proposal_text: &str,
    created_at: &str,
) -> rusqlite::Result<i64> {
    let proposal_json = json!({
        "kind": "file_workflow",
        "message": proposal_text,
    });
    conn.execute(
        "INSERT INTO suggestions (pattern_id, status, proposal_json, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![pattern_id, "pending", proposal_json.to_string(), created_at],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_suggestions(conn: &Connection) -> rusqlite::Result<Vec<StoredSuggestion>> {
    let mut statement = conn.prepare(
        r#"
        SELECT
            patterns.id,
            patterns.signature,
            patterns.count,
            patterns.avg_duration_ms,
            COALESCE(patterns.canonical_summary, ''),
            suggestions.proposal_json,
            suggestions.created_at
        FROM suggestions
        INNER JOIN patterns ON patterns.id = suggestions.pattern_id
        WHERE suggestions.status = 'pending'
        ORDER BY patterns.count DESC, patterns.signature ASC, suggestions.created_at ASC
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

        Ok(StoredSuggestion {
            pattern_id: row.get(0)?,
            signature: row.get(1)?,
            count: row.get::<_, i64>(2)? as usize,
            avg_duration_ms: row.get(3)?,
            canonical_summary: row.get(4)?,
            proposal_text: proposal
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string(),
            created_at: row.get(6)?,
        })
    })?;

    rows.collect()
}

pub fn list_patterns(conn: &Connection) -> rusqlite::Result<Vec<StoredPattern>> {
    let mut statement = conn.prepare(
        r#"
        SELECT id, signature, count, avg_duration_ms, COALESCE(canonical_summary, '')
        FROM patterns
        ORDER BY count DESC, signature ASC, id ASC
        "#,
    )?;

    let rows = statement.query_map([], |row| {
        Ok(StoredPattern {
            pattern_id: row.get(0)?,
            signature: row.get(1)?,
            count: row.get::<_, i64>(2)? as usize,
            avg_duration_ms: row.get(3)?,
            canonical_summary: row.get(4)?,
        })
    })?;

    rows.collect()
}

pub fn list_recent_raw_events(
    conn: &Connection,
    limit: usize,
) -> rusqlite::Result<Vec<StoredRawEvent>> {
    let mut statement = conn.prepare(
        r#"
        SELECT ts, source, payload_json
        FROM raw_events
        ORDER BY ts DESC, id DESC
        LIMIT ?1
        "#,
    )?;

    let rows = statement.query_map(params![limit as i64], |row| {
        let ts: String = row.get(0)?;
        let payload_json: String = row.get(2)?;
        let payload: Value = serde_json::from_str(&payload_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                payload_json.len(),
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;

        Ok(StoredRawEvent {
            ts: chrono::DateTime::parse_from_rfc3339(&ts)
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        ts.len(),
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?
                .with_timezone(&chrono::Utc),
            source: row.get(1)?,
            payload,
        })
    })?;

    let mut events: Vec<_> = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    events.reverse();
    Ok(events)
}

pub fn list_normalized_events(conn: &Connection) -> rusqlite::Result<Vec<NormalizedEvent>> {
    Ok(list_normalized_event_records(conn)?
        .into_iter()
        .map(|record| record.event)
        .collect())
}

pub fn list_normalized_event_records(
    conn: &Connection,
) -> rusqlite::Result<Vec<StoredNormalizedEvent>> {
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
        let metadata: Value = serde_json::from_str(&metadata_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                metadata_json.len(),
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;

        Ok(StoredNormalizedEvent {
            id: row.get(0)?,
            event: NormalizedEvent {
                ts: chrono::DateTime::parse_from_rfc3339(&ts)
                    .map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            ts.len(),
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?
                    .with_timezone(&chrono::Utc),
                action_type: parse_action_type(&action_type).ok_or_else(|| {
                    rusqlite::Error::FromSqlConversionFailure(
                        action_type.len(),
                        rusqlite::types::Type::Text,
                        "unknown action type".into(),
                    )
                })?,
                app: row.get(3)?,
                target: row.get(4)?,
                metadata,
            },
        })
    })?;

    rows.collect()
}

pub fn refresh_suggestions(conn: &Connection) -> rusqlite::Result<()> {
    let records = list_normalized_event_records(conn)?;

    conn.execute_batch(
        r#"
        DELETE FROM session_events;
        DELETE FROM sessions;
        DELETE FROM suggestions;
        DELETE FROM patterns;
        "#,
    )?;

    if records.is_empty() {
        return Ok(());
    }

    let normalized_events: Vec<_> = records.iter().map(|record| record.event.clone()).collect();
    let sessions = split_into_sessions(&normalized_events, 300);
    let session_event_ids = split_session_event_ids(&records, 300);

    for (session, event_ids) in sessions.iter().zip(session_event_ids.iter()) {
        insert_session(
            conn,
            &session.start_ts.to_rfc3339(),
            &session.end_ts.to_rfc3339(),
            event_ids,
        )?;
    }

    let session_end_times = session_end_times_by_signature(&sessions);
    for pattern in detect_repeated_patterns(&sessions) {
        let pattern_id = insert_pattern(
            conn,
            &pattern.signature,
            pattern.count,
            pattern.avg_duration_ms,
            &pattern.canonical_summary,
        )?;
        insert_suggestion(
            conn,
            pattern_id,
            &pattern.proposal_text,
            &deterministic_created_at(&pattern, &session_end_times),
        )?;
    }

    Ok(())
}

fn parse_action_type(action_type: &str) -> Option<flow_core::events::ActionType> {
    use flow_core::events::ActionType;

    Some(match action_type {
        "OpenApp" => ActionType::OpenApp,
        "SwitchApp" => ActionType::SwitchApp,
        "CopyText" => ActionType::CopyText,
        "PasteText" => ActionType::PasteText,
        "RunCommand" => ActionType::RunCommand,
        "CreateFile" => ActionType::CreateFile,
        "RenameFile" => ActionType::RenameFile,
        "MoveFile" => ActionType::MoveFile,
        "VisitUrl" => ActionType::VisitUrl,
        "DownloadFile" => ActionType::DownloadFile,
        _ => return None,
    })
}

fn split_session_event_ids(
    records: &[StoredNormalizedEvent],
    inactivity_secs: i64,
) -> Vec<Vec<i64>> {
    if records.is_empty() || inactivity_secs < 0 {
        return Vec::new();
    }

    let mut sessions: Vec<Vec<i64>> = Vec::new();
    let mut current = vec![records[0].id];

    for pair in records.windows(2) {
        let prev = &pair[0].event;
        let next_record = &pair[1];
        let gap = next_record
            .event
            .ts
            .signed_duration_since(prev.ts)
            .num_seconds();

        if gap > inactivity_secs {
            sessions.push(current);
            current = vec![next_record.id];
        } else {
            current.push(next_record.id);
        }
    }

    sessions.push(current);
    sessions
}

fn session_end_times_by_signature(
    sessions: &[EventSession],
) -> HashMap<String, chrono::DateTime<chrono::Utc>> {
    let mut end_times = HashMap::new();

    for session in sessions {
        let signature = session
            .events
            .iter()
            .map(|event| {
                let group = event
                    .metadata
                    .get("file_group")
                    .and_then(|value| value.as_str())
                    .unwrap_or("file");
                format!("{:?}:{group}", event.action_type)
            })
            .collect::<Vec<_>>()
            .join("->");

        end_times
            .entry(signature)
            .and_modify(|current: &mut chrono::DateTime<chrono::Utc>| {
                if session.end_ts > *current {
                    *current = session.end_ts;
                }
            })
            .or_insert(session.end_ts);
    }

    end_times
}

fn deterministic_created_at(
    pattern: &PatternCandidate,
    session_end_times: &HashMap<String, chrono::DateTime<chrono::Utc>>,
) -> String {
    session_end_times
        .get(&pattern.signature)
        .copied()
        .unwrap_or_else(|| chrono::DateTime::from_timestamp(0, 0).unwrap())
        .to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::run_migrations;
    use chrono::Utc;
    use flow_adapters::file_watcher::FileEvent;

    #[test]
    fn stores_and_reads_detected_suggestions() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let fixture_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/invoice_file_events.json"
        );
        let raw_fixture = std::fs::read_to_string(fixture_path).unwrap();
        let file_events: Vec<FileEvent> = serde_json::from_str(&raw_fixture).unwrap();
        for raw_event in file_events.into_iter().map(FileEvent::into_raw_event) {
            ingest_raw_event(&conn, &raw_event).unwrap();
        }

        let suggestions = list_suggestions(&conn).unwrap();
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].count, 2);
        assert!(suggestions[0].proposal_text.contains("invoice"));
    }

    #[test]
    fn lists_recent_raw_events_in_chronological_order() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let first = RawEvent {
            ts: Utc::now(),
            source: flow_core::events::EventSource::Terminal,
            payload: json!({ "command": "ls" }),
        };
        let second = RawEvent {
            ts: first.ts + chrono::TimeDelta::seconds(5),
            source: flow_core::events::EventSource::Browser,
            payload: json!({ "url": "https://example.com" }),
        };

        insert_raw_event(&conn, &first).unwrap();
        insert_raw_event(&conn, &second).unwrap();

        let events = list_recent_raw_events(&conn, 10).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].source, "Terminal");
        assert_eq!(events[1].source, "Browser");
    }

    #[test]
    fn lists_patterns_with_summary_fields() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        insert_pattern(
            &conn,
            "CreateFile:invoice->RenameFile:invoice_reviewed",
            3,
            4200,
            "CreateFile -> RenameFile",
        )
        .unwrap();

        let patterns = list_patterns(&conn).unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].count, 3);
        assert_eq!(patterns[0].avg_duration_ms, 4200);
        assert_eq!(patterns[0].canonical_summary, "CreateFile -> RenameFile");
    }
}
