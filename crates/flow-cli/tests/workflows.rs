use chrono::Utc;
use flow_adapters::file_watcher::FileEvent;
use flow_db::{
    migrations::run_migrations,
    repo::{insert_normalized_event_record, insert_pattern, insert_session, insert_suggestion},
};
use flow_patterns::{
    detect::detect_repeated_patterns, normalize::normalize, sessions::split_into_sessions,
};
use rusqlite::Connection;
use std::{path::Path, process::Command};

#[test]
fn patterns_renders_detected_workflows_table() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .arg("patterns")
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("pattern_id"));
    assert!(stdout.contains("runs"));
    assert!(stdout.contains("example"));
    assert!(stdout.contains("invoice_invoice_reviewed_workflow"));
    assert!(stdout.contains("create -> rename -> move"));
}

#[test]
fn suggestions_renders_detected_suggestions_table() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .arg("suggestions")
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("suggestion_id"));
    assert!(stdout.contains("pattern"));
    assert!(stdout.contains("description"));
    assert!(stdout.contains("Repeated invoice file workflow detected"));
}

#[test]
fn sessions_renders_recent_sessions_table() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .arg("sessions")
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("session_id"));
    assert!(stdout.contains("events"));
    assert!(stdout.contains("duration"));
    assert!(stdout.contains("40s"));
    assert!(stdout.contains("2"));
    assert!(stdout.contains("1"));
}

fn seed_database(db_path: &Path) {
    let conn = Connection::open(db_path).unwrap();
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
}
