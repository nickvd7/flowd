use flow_adapters::file_watcher::FileEvent;
use flow_db::{migrations::run_migrations, repo::insert_normalized_event_record};
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
    assert!(stdout.contains("Repeated invoice"));
    assert!(stdout.contains("repeats: 2"));
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
