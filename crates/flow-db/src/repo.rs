use chrono::{DateTime, Utc};
use flow_core::events::{EventSource, NormalizedEvent, RawEvent};
use rusqlite::{params, Connection};
use serde_json::{json, Value};

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

#[derive(Debug, Clone)]
pub struct StoredRawEvent {
    pub id: i64,
    pub event: RawEvent,
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
    conn: &Connection,
    event: &NormalizedEvent,
) -> rusqlite::Result<i64> {
    insert_normalized_event(conn, event)?;
    Ok(conn.last_insert_rowid())
}

pub fn insert_normalized_event_for_raw_event(
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

fn parse_json_value(value: &str) -> rusqlite::Result<Value> {
    serde_json::from_str(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            value.len(),
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::run_migrations;
    use chrono::Utc;
    use flow_adapters::file_watcher::{synthetic_file_event, FileEvent, FileEventKind};
    use flow_core::events::EventSource;
    use flow_patterns::{
        detect::detect_repeated_patterns, normalize::normalize, sessions::split_into_sessions,
    };
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
        let conn = Connection::open_in_memory().unwrap();
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
        let sessions = split_into_sessions(&normalized, 300);

        for session in &sessions {
            let event_ids: Vec<_> = session
                .events
                .iter()
                .map(|event| insert_normalized_event_record(&conn, event).unwrap())
                .collect();
            insert_session(
                &conn,
                &session.start_ts.to_rfc3339(),
                &session.end_ts.to_rfc3339(),
                &event_ids,
            )
            .unwrap();
        }

        for pattern in detect_repeated_patterns(&sessions) {
            let pattern_id = insert_pattern(
                &conn,
                &pattern.signature,
                pattern.count,
                pattern.avg_duration_ms,
                &pattern.canonical_summary,
            )
            .unwrap();
            insert_suggestion(
                &conn,
                pattern_id,
                &pattern.proposal_text,
                &Utc::now().to_rfc3339(),
            )
            .unwrap();
        }

        let suggestions = list_suggestions(&conn).unwrap();
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].count, 2);
        assert!(suggestions[0].proposal_text.contains("invoice"));
    }

    #[test]
    fn lists_pending_file_raw_events_without_normalized_rows() {
        let conn = Connection::open_in_memory().unwrap();
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
        insert_normalized_event_for_raw_event(&conn, completed_raw_id, &normalized).unwrap();

        let pending = list_pending_file_raw_events(&conn).unwrap();

        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].event.ts, pending_raw.ts);
        assert_eq!(pending[0].event.source, pending_raw.source);
        assert_eq!(pending[0].event.payload, pending_raw.payload);
    }

    #[test]
    fn ignores_duplicate_normalized_rows_for_the_same_raw_event() {
        let conn = Connection::open_in_memory().unwrap();
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
            insert_normalized_event_for_raw_event(&conn, raw_event_id, &normalized).unwrap();
        let second_insert =
            insert_normalized_event_for_raw_event(&conn, raw_event_id, &normalized).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM normalized_events", [], |row| {
                row.get(0)
            })
            .unwrap();

        assert!(first_insert);
        assert!(!second_insert);
        assert_eq!(count, 1);
    }
}
