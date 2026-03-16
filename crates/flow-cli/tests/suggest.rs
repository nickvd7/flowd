use chrono::{DateTime, Utc};
use flow_adapters::file_watcher::FileEvent;
use flow_analysis::refresh_analysis_state;
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
    assert_eq!(suggestion.shown_count, 1);
    assert!(suggestion.last_shown_ts.is_some());
    let expected = format!(
        "[{}] {}\n  pattern: {} | runs: {} | avg: {} | score: {:.3} | freshness: {} | last seen: {}\n\nNext steps:\n1. Inspect one suggestion: flowctl suggestions explain {}\n2. Review suggestion history: flowctl suggestions history\n3. Approve a suggestion: flowctl approve {}\n",
        suggestion.suggestion_id,
        suggestion.proposal_text,
        suggestion.canonical_summary,
        suggestion.count,
        format_duration(suggestion.avg_duration_ms),
        suggestion.usefulness_score,
        suggestion.freshness,
        format_timestamp(&suggestion.last_seen_at),
        suggestion.suggestion_id,
        suggestion.suggestion_id,
    );
    assert_eq!(stdout, expected);
}

#[test]
fn suggest_explain_renders_baseline_fallback_details() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["suggest", "--explain"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("explain: baseline fallback"));
    assert!(stdout.contains("score: baseline_score="));
    assert!(stdout.contains("factors: fallback=No intelligence decision was applied."));
    assert!(stdout.contains("Next steps:"));
    assert!(stdout.contains("flowctl suggestions history"));
    assert!(stdout.contains("flowctl approve 1"));
}

#[test]
fn suggestions_explain_renders_deterministic_fallback_report_without_marking_display() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["suggestions", "explain", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Suggestion: Repeated invoice file workflow detected"));
    assert!(stdout.contains("Why this suggestion appeared:"));
    assert!(stdout.contains("pattern repetitions: 2"));
    assert!(stdout.contains("confidence:"));
    assert!(stdout.contains("estimated time saved:"));
    assert!(stdout.contains("Observed workflow:"));
    assert!(stdout.contains("rename"));
    assert!(stdout.contains("move"));
    assert!(stdout.contains("Stored metadata:"));
    assert!(stdout.contains("feedback: shown=0, accepted=0, rejected=0, snoozed=0"));
    assert!(stdout.contains("Automation preview"));
    assert!(stdout.contains("Examples:"));
    assert!(stdout.contains("invoice-1001.pdf -> invoice-1001-reviewed.pdf"));
    assert!(stdout.contains("Risk:"));

    let conn = Connection::open(&db_path).unwrap();
    let suggestion = list_suggestions(&conn).unwrap().remove(0);
    assert_eq!(suggestion.shown_count, 0);
    assert!(suggestion.last_shown_ts.is_none());
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

    refresh_analysis_state(&mut conn, 300).unwrap();
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
