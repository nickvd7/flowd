use chrono::{DateTime, Utc};
use flow_adapters::file_watcher::FileEvent;
use flow_db::{
    migrations::run_migrations,
    repo::{insert_normalized_event_record, list_suggestions},
};
use flow_patterns::normalize::normalize;
use rusqlite::Connection;
use std::{path::Path, process::Command};

#[test]
fn suggest_renders_detected_file_workflow() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .arg("suggest")
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let conn = Connection::open(&db_path).unwrap();
    let suggestion = list_suggestions(&conn).unwrap().remove(0);
    let expected = format!(
        "[{}] {}\n  pattern: {} | runs: {} | avg: {} | score: {:.3} | freshness: {} | last seen: {}\n",
        suggestion.suggestion_id,
        suggestion.proposal_text,
        suggestion.canonical_summary,
        suggestion.count,
        format_duration(suggestion.avg_duration_ms),
        suggestion.usefulness_score,
        suggestion.freshness,
        format_timestamp(&suggestion.last_seen_at),
    );
    assert_eq!(stdout, expected);
}

fn seed_database(db_path: &Path) {
    let mut conn = Connection::open(db_path).unwrap();
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
}

fn format_duration(duration_ms: i64) -> String {
    let total_seconds = duration_ms / 1000;

    if duration_ms >= 60_000 && duration_ms % 1000 == 0 {
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;
        if seconds == 0 {
            return format!("{minutes}m");
        }
        return format!("{minutes}m {seconds}s");
    }

    if duration_ms % 1000 == 0 {
        return format!("{total_seconds}s");
    }

    format!("{duration_ms}ms")
}

fn format_timestamp(value: &str) -> String {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| {
            timestamp
                .with_timezone(&Utc)
                .format("%Y-%m-%d %H:%M:%SZ")
                .to_string()
        })
        .unwrap_or_else(|_| value.to_string())
}
