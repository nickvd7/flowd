use flow_adapters::file_watcher::FileEvent;
use flow_db::{migrations::run_migrations, repo::insert_normalized_event_record};
use flow_patterns::normalize::normalize;
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

#[test]
fn approve_creates_automation_and_lists_it() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);

    let approve = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["approve", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(approve.status.success());
    let approve_stdout = String::from_utf8(approve.stdout).unwrap();
    assert!(approve_stdout.contains("Approved suggestion 1 as automation 1"));

    let automations = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .arg("automations")
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(automations.status.success());
    let stdout = String::from_utf8(automations.stdout).unwrap();
    assert!(stdout.contains("automation_id"));
    assert!(stdout.contains("suggestion_id"));
    assert!(stdout.contains("approved"));
    assert!(stdout.contains("Repeated invoice file workflow detected"));
}

#[test]
fn dry_run_previews_actions_and_records_a_run() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);
    approve_suggestion(&db_path);

    std::fs::create_dir_all(temp_dir.path().join("inbox")).unwrap();
    std::fs::write(temp_dir.path().join("inbox/invoice-1003.pdf"), "invoice").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["dry-run", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("rename:"));
    assert!(stdout.contains("move:"));
    assert!(temp_dir.path().join("inbox/invoice-1003.pdf").exists());
    assert!(!temp_dir
        .path()
        .join("archive/invoice-1003-reviewed.pdf")
        .exists());

    let conn = Connection::open(&db_path).unwrap();
    let run_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM automation_runs WHERE result = 'dry_run'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(run_count, 1);
}

#[test]
fn run_executes_safe_file_automation_and_records_result() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);
    approve_suggestion(&db_path);

    std::fs::create_dir_all(temp_dir.path().join("inbox")).unwrap();
    std::fs::write(temp_dir.path().join("inbox/invoice-1004.pdf"), "invoice").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["run", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("rename:"));
    assert!(stdout.contains("move:"));
    assert!(!temp_dir.path().join("inbox/invoice-1004.pdf").exists());
    assert!(temp_dir
        .path()
        .join("archive/invoice-1004-reviewed.pdf")
        .exists());

    let conn = Connection::open(&db_path).unwrap();
    let run_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM automation_runs WHERE result = 'completed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(run_count, 1);
}

fn seed_database(db_path: &Path) {
    let mut conn = Connection::open(db_path).unwrap();
    run_migrations(&conn).unwrap();

    let fixture_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/invoice_file_events.json"
    );
    let raw_fixture = std::fs::read_to_string(fixture_path).unwrap();
    let mut file_events: Vec<FileEvent> = serde_json::from_str(&raw_fixture).unwrap();
    let inbox = db_path.parent().unwrap().join("inbox");
    let archive = db_path.parent().unwrap().join("archive");
    let inbox_text = inbox.display().to_string();
    let archive_text = archive.display().to_string();
    for event in &mut file_events {
        event.path = event
            .path
            .replace("/tmp/inbox", &inbox_text)
            .replace("/tmp/archive", &archive_text);
        event.from_path = event.from_path.as_ref().map(|value| {
            value
                .replace("/tmp/inbox", &inbox_text)
                .replace("/tmp/archive", &archive_text)
        });
    }
    let raw_events: Vec<_> = file_events
        .into_iter()
        .map(FileEvent::into_raw_event)
        .collect();
    let normalized: Vec<_> = raw_events.iter().filter_map(normalize).collect();

    for event in &normalized {
        insert_normalized_event_record(&mut conn, event).unwrap();
    }
}

fn approve_suggestion(db_path: &Path) {
    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["approve", "1"])
        .env("FLOWD_DB_PATH", db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
}
