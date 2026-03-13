use flow_db::{
    migrations::run_migrations,
    repo::{
        insert_automation, insert_automation_run, insert_pattern, insert_suggestion,
        AutomationRunRecord,
    },
};
use rusqlite::Connection;
use std::{path::Path, process::Command};

#[test]
fn stats_renders_zero_counts_for_empty_database() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    let conn = Connection::open(&db_path).unwrap();
    run_migrations(&conn).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .arg("stats")
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout,
        concat!(
            "Local usage stats\n",
            "patterns_detected: 0\n",
            "suggestions_created: 0\n",
            "automations_approved: 0\n",
            "automation_runs: 0\n",
            "undo_runs: 0\n",
            "estimated_time_saved: 0s\n",
            "\n",
            "Next steps:\n",
            "1. Inspect pending suggestions: flowctl suggestions\n",
            "2. Inspect approved automations: flowctl automations\n",
            "3. Inspect config values: flowctl config show\n",
        )
    );
}

#[test]
fn stats_renders_local_usage_summary() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .arg("stats")
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout,
        concat!(
            "Local usage stats\n",
            "patterns_detected: 2\n",
            "suggestions_created: 2\n",
            "automations_approved: 1\n",
            "automation_runs: 2\n",
            "undo_runs: 1\n",
            "estimated_time_saved: 1m\n",
            "\n",
            "Next steps:\n",
            "1. Inspect pending suggestions: flowctl suggestions\n",
            "2. Inspect approved automations: flowctl automations\n",
            "3. Inspect config values: flowctl config show\n",
        )
    );
}

fn seed_database(db_path: &Path) {
    let conn = Connection::open(db_path).unwrap();
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
        40_000,
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
        "active",
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
            finished_at: "2026-03-11T12:20:05Z",
            result: "completed",
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
}
