use flow_adapters::file_watcher::{synthetic_file_event, FileEventKind};
use flow_db::{
    migrations::run_migrations,
    repo::{insert_automation, insert_pattern, insert_raw_event, insert_suggestion},
};
use rusqlite::Connection;
use std::{path::Path, process::Command};

#[test]
fn doctor_reports_healthy_local_state() {
    let temp_dir = tempfile::tempdir().unwrap();
    let watch_path = temp_dir.path().join("Inbox");
    std::fs::create_dir(&watch_path).unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    let config_path = temp_dir.path().join("flowd.toml");
    write_config(
        &config_path,
        &format!(
            "database_path = \"{}\"\nobserved_folders = [\"{}\"]\nintelligence_enabled = true\n",
            db_path.display(),
            watch_path.display(),
        ),
    );
    seed_database(&db_path);

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["--config", config_path.to_str().unwrap(), "doctor"])
        .env("FLOWD_DOCTOR_DAEMON_RUNNING", "1")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout,
        concat!(
            "daemon: running\n",
            "database: ok\n",
            "watch paths: configured\n",
            "events observed: yes\n",
            "patterns detected: yes\n",
            "suggestions available: yes\n",
            "automations: 1 active\n",
            "intelligence layer: connected\n",
        )
    );
}

#[test]
fn doctor_reports_degraded_state_without_database_or_valid_watch_path() {
    let temp_dir = tempfile::tempdir().unwrap();
    let missing_watch_path = temp_dir.path().join("Missing");
    let missing_db_path = temp_dir.path().join("missing.db");
    let config_path = temp_dir.path().join("flowd.toml");
    write_config(
        &config_path,
        &format!(
            "database_path = \"{}\"\nobserved_folders = [\"{}\"]\nintelligence_enabled = false\n",
            missing_db_path.display(),
            missing_watch_path.display(),
        ),
    );

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["--config", config_path.to_str().unwrap(), "doctor"])
        .env("FLOWD_DOCTOR_DAEMON_RUNNING", "0")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("daemon: not running\n"));
    assert!(stdout.contains("database: error (failed to open database at "));
    assert!(stdout.contains("\nwatch paths: not configured\n"));
    assert!(stdout.contains("events observed: unknown\n"));
    assert!(stdout.contains("patterns detected: unknown\n"));
    assert!(stdout.contains("suggestions available: unknown\n"));
    assert!(stdout.contains("automations: unknown\n"));
    assert!(stdout.contains("intelligence layer: disabled\n"));
}

fn seed_database(db_path: &Path) {
    let conn = Connection::open(db_path).unwrap();
    run_migrations(&conn).unwrap();

    let raw_event = synthetic_file_event(
        chrono::Utc::now(),
        FileEventKind::Create,
        db_path.with_file_name("invoice.pdf").display().to_string(),
        None,
    );
    insert_raw_event(&conn, &raw_event).unwrap();

    let pattern_id = insert_pattern(
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
    let suggestion_id = insert_suggestion(
        &conn,
        pattern_id,
        "Repeated invoice file workflow detected",
        "2026-03-11T10:00:00Z",
        0.8,
    )
    .unwrap();
    insert_automation(
        &conn,
        suggestion_id,
        "id: invoice\ntrigger: {}\nactions: []\n",
        "active",
        "Invoice automation",
        "2026-03-11T12:00:00Z",
    )
    .unwrap();
}

fn write_config(path: &Path, contents: &str) {
    std::fs::write(path, contents).unwrap();
}
