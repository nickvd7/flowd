use flow_core::events::{NormalizedEvent, RawEvent};
use rusqlite::{params, Connection};

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

pub fn insert_normalized_event(conn: &Connection, event: &NormalizedEvent) -> rusqlite::Result<usize> {
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
