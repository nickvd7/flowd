use chrono::{DateTime, Utc};
use flow_core::events::{ActionType, EventSource, NormalizedEvent, RawEvent};
use flow_patterns::{detect::detect_repeated_patterns, sessions::split_into_sessions};
use rusqlite::{params, Connection};
use serde_json::{json, Value};

const DEFAULT_SESSION_INACTIVITY_SECS: i64 = 300;

#[derive(Debug, Clone, PartialEq)]
pub struct StoredSuggestion {
    pub suggestion_id: i64,
    pub pattern_id: i64,
    pub signature: String,
    pub count: usize,
    pub avg_duration_ms: i64,
    pub canonical_summary: String,
    pub proposal_text: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct SuggestionDetails {
    pub suggestion_id: i64,
    pub pattern_id: i64,
    pub status: String,
    pub signature: String,
    pub canonical_summary: String,
    pub proposal_text: String,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPattern {
    pub pattern_id: i64,
    pub signature: String,
    pub count: usize,
    pub avg_duration_ms: i64,
    pub canonical_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSession {
    pub session_id: i64,
    pub event_count: usize,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAutomation {
    pub automation_id: i64,
    pub suggestion_id: Option<i64>,
    pub status: String,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub struct StoredAutomationSpec {
    pub automation_id: i64,
    pub suggestion_id: Option<i64>,
    pub status: String,
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

fn insert_normalized_event_row(conn: &Connection, event: &NormalizedEvent) -> rusqlite::Result<usize> {
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
    refresh_analysis_state(conn, DEFAULT_SESSION_INACTIVITY_SECS)?;
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
    let inserted = insert_normalized_event_for_raw_event_row(conn, raw_event_id, event)?;
    if inserted {
        refresh_analysis_state(conn, DEFAULT_SESSION_INACTIVITY_SECS)?;
    }
    Ok(inserted)
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

    if inserted_count > 0 {
        refresh_analysis_state(conn, DEFAULT_SESSION_INACTIVITY_SECS)?;
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
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO patterns (signature, count, avg_duration_ms, canonical_summary, confidence) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![signature, count as i64, avg_duration_ms, canonical_summary, count as f64],
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

pub fn refresh_analysis_state(conn: &mut Connection, inactivity_secs: i64) -> rusqlite::Result<()> {
    let stored_events = list_normalized_events(conn)?;
    let normalized_events: Vec<_> = stored_events
        .iter()
        .map(|stored| stored.event.clone())
        .collect();
    let sessions = split_into_sessions(&normalized_events, inactivity_secs);
    let patterns = detect_repeated_patterns(&sessions);
    let created_at = Utc::now().to_rfc3339();

    let tx = conn.transaction()?;
    clear_analysis_state(&tx)?;

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
        )?;
        offset = next_offset;
    }

    for pattern in patterns {
        let pattern_id = insert_pattern(
            &tx,
            &pattern.signature,
            pattern.count,
            pattern.avg_duration_ms,
            &pattern.canonical_summary,
        )?;
        insert_suggestion(&tx, pattern_id, &pattern.proposal_text, &created_at)?;
    }

    tx.commit()?;
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
            suggestions.id,
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
            created_at: row.get(7)?,
        })
    })?;

    rows.collect()
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
            suggestions.proposal_json
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
        })
    });

    match row {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(error),
    }
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

pub fn insert_automation(
    conn: &Connection,
    suggestion_id: i64,
    spec_yaml: &str,
    state: &str,
    summary: &str,
    accepted_at: &str,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO automations (suggestion_id, spec_yaml, state, summary, accepted_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![suggestion_id, spec_yaml, state, summary, accepted_at],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_automations(conn: &Connection) -> rusqlite::Result<Vec<StoredAutomation>> {
    let mut statement = conn.prepare(
        r#"
        SELECT id, suggestion_id, state, COALESCE(summary, '')
        FROM automations
        ORDER BY id ASC
        "#,
    )?;

    let rows = statement.query_map([], |row| {
        Ok(StoredAutomation {
            automation_id: row.get(0)?,
            suggestion_id: row.get(1)?,
            status: row.get(2)?,
            summary: row.get(3)?,
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
        SELECT id, suggestion_id, state, COALESCE(summary, ''), spec_yaml
        FROM automations
        WHERE id = ?1
        "#,
    )?;

    let row = statement.query_row([automation_id], |row| {
        Ok(StoredAutomationSpec {
            automation_id: row.get(0)?,
            suggestion_id: row.get(1)?,
            status: row.get(2)?,
            summary: row.get(3)?,
            spec_yaml: row.get(4)?,
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
            event_count: row.get::<_, i64>(1)? as usize,
            duration_ms: row.get(2)?,
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
    fn stores_and_reads_detected_suggestions() {
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

        let suggestions = list_suggestions(&conn).unwrap();
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].count, 2);
        assert!(suggestions[0].proposal_text.contains("invoice"));
    }

    #[test]
    fn refreshes_suggestions_automatically_without_duplicates() {
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

        for event in normalized.iter().take(3) {
            insert_normalized_event_record(&mut conn, event).unwrap();
        }

        assert!(list_suggestions(&conn).unwrap().is_empty());

        for event in normalized.iter().skip(3) {
            insert_normalized_event_record(&mut conn, event).unwrap();
        }

        let suggestions = list_suggestions(&conn).unwrap();
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].count, 2);

        let pattern_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM patterns", [], |row| row.get(0))
            .unwrap();
        let suggestion_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM suggestions", [], |row| row.get(0))
            .unwrap();

        assert_eq!(pattern_count, 1);
        assert_eq!(suggestion_count, 1);
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
}
