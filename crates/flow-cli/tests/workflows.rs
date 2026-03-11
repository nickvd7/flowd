use flow_adapters::file_watcher::FileEvent;
use flow_db::{migrations::run_migrations, repo::insert_normalized_event_record};
use flow_dsl::{Action, AutomationSpec, Safety, Trigger};
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
    assert!(stdout.contains("score"));
    assert!(stdout.contains("freshness"));
    assert!(stdout.contains("description"));
    assert!(stdout.contains("current"));
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
    assert!(stdout.contains("active"));
    assert!(stdout.contains("Repeated invoice file workflow detected"));
}

#[test]
fn disable_and_enable_update_automation_status_in_cli_output() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);
    approve_suggestion(&db_path);

    let disable = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["disable", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(disable.status.success());
    assert!(String::from_utf8(disable.stdout)
        .unwrap()
        .contains("Disabled automation 1"));

    let disabled = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .arg("automations")
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(disabled.status.success());
    assert!(String::from_utf8(disabled.stdout)
        .unwrap()
        .contains("disabled"));

    let enable = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["enable", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(enable.status.success());
    assert!(String::from_utf8(enable.stdout)
        .unwrap()
        .contains("Enabled automation 1"));

    let enabled = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .arg("automations")
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(enabled.status.success());
    assert!(String::from_utf8(enabled.stdout)
        .unwrap()
        .contains("active"));
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

#[test]
fn runs_lists_completed_execution_history() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);
    approve_suggestion(&db_path);

    std::fs::create_dir_all(temp_dir.path().join("inbox")).unwrap();
    std::fs::write(temp_dir.path().join("inbox/invoice-1006.pdf"), "invoice").unwrap();

    let run = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["run", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();
    assert!(run.status.success());

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .arg("runs")
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("run_id"));
    assert!(stdout.contains("automation_id"));
    assert!(stdout.contains("completed"));
}

#[test]
fn undo_reverses_a_completed_rename_and_move_run_in_reverse_order() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);
    approve_suggestion(&db_path);

    std::fs::create_dir_all(temp_dir.path().join("inbox")).unwrap();
    std::fs::write(temp_dir.path().join("inbox/invoice-1007.pdf"), "invoice").unwrap();

    let run = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["run", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();
    assert!(run.status.success());

    let undo = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["undo", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(undo.status.success());
    let stdout = String::from_utf8(undo.stdout).unwrap();
    let move_index = stdout.find("move:").unwrap();
    let rename_index = stdout.find("rename:").unwrap();
    assert!(move_index < rename_index);
    assert!(stdout.contains("Undid automation run 1."));
    assert!(temp_dir.path().join("inbox/invoice-1007.pdf").exists());
    assert!(!temp_dir
        .path()
        .join("archive/invoice-1007-reviewed.pdf")
        .exists());

    let conn = Connection::open(&db_path).unwrap();
    let undo_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM automation_runs WHERE result = 'undone'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(undo_count, 1);
}

#[test]
fn undo_aborts_when_filesystem_state_is_no_longer_safe() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);
    approve_suggestion(&db_path);

    std::fs::create_dir_all(temp_dir.path().join("inbox")).unwrap();
    std::fs::write(temp_dir.path().join("inbox/invoice-1008.pdf"), "invoice").unwrap();

    let run = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["run", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();
    assert!(run.status.success());

    std::fs::write(temp_dir.path().join("inbox/invoice-1008.pdf"), "collision").unwrap();

    let undo = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["undo", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(!undo.status.success());
    let stderr = String::from_utf8(undo.stderr).unwrap();
    assert!(stderr.contains("destination already exists"));
    assert!(temp_dir
        .path()
        .join("archive/invoice-1008-reviewed.pdf")
        .exists());

    let conn = Connection::open(&db_path).unwrap();
    let undo_failed_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM automation_runs WHERE result = 'undo_failed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(undo_failed_count, 1);
}

#[test]
fn undo_restores_a_completed_rename_only_run() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    let inbox = temp_dir.path().join("inbox");
    seed_manual_automation(&db_path, rename_only_spec(&inbox));
    std::fs::create_dir_all(&inbox).unwrap();
    std::fs::write(inbox.join("invoice-1009.pdf"), "invoice").unwrap();

    let run = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["run", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();
    assert!(run.status.success());
    assert!(inbox.join("invoice-1009-reviewed.pdf").exists());

    let undo = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["undo", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();
    assert!(undo.status.success());
    assert!(inbox.join("invoice-1009.pdf").exists());
    assert!(!inbox.join("invoice-1009-reviewed.pdf").exists());
}

#[test]
fn undo_restores_a_completed_move_only_run() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    let inbox = temp_dir.path().join("inbox");
    let archive = temp_dir.path().join("archive");
    seed_manual_automation(&db_path, move_only_spec(&inbox, &archive));
    std::fs::create_dir_all(&inbox).unwrap();
    std::fs::write(inbox.join("invoice-1010.pdf"), "invoice").unwrap();

    let run = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["run", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();
    assert!(run.status.success());
    assert!(archive.join("invoice-1010.pdf").exists());

    let undo = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["undo", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();
    assert!(undo.status.success());
    assert!(inbox.join("invoice-1010.pdf").exists());
    assert!(!archive.join("invoice-1010.pdf").exists());
}

#[test]
fn undo_rejects_runs_with_unsupported_operations() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    let conn = Connection::open(&db_path).unwrap();
    run_migrations(&conn).unwrap();
    conn.execute(
        "INSERT INTO automations (suggestion_id, spec_yaml, state, summary, accepted_at) VALUES (NULL, ?1, 'active', 'unsupported undo', '2026-03-11T00:00:00Z')",
        [rename_only_yaml(temp_dir.path().join("inbox").as_path())],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO automation_runs (automation_id, started_at, finished_at, result, undo_payload_json) VALUES (1, '2026-03-11T00:00:00Z', '2026-03-11T00:00:01Z', 'completed', ?1)",
        [r#"{"operations":[{"action":"copy","from":"/tmp/a","to":"/tmp/b"}]}"#],
    )
    .unwrap();

    let undo = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["undo", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(!undo.status.success());
    let stderr = String::from_utf8(undo.stderr).unwrap();
    assert!(stderr.contains("unsupported operation"));
}

#[test]
fn run_blocks_disabled_automation_without_mutating_files() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);
    approve_suggestion(&db_path);

    std::fs::create_dir_all(temp_dir.path().join("inbox")).unwrap();
    std::fs::write(temp_dir.path().join("inbox/invoice-1005.pdf"), "invoice").unwrap();

    let disable = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["disable", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();
    assert!(disable.status.success());

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["run", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("automation 1 is disabled"));
    assert!(temp_dir.path().join("inbox/invoice-1005.pdf").exists());
    assert!(!temp_dir
        .path()
        .join("archive/invoice-1005-reviewed.pdf")
        .exists());

    let conn = Connection::open(&db_path).unwrap();
    let run_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM automation_runs WHERE result = 'completed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(run_count, 0);
}

#[test]
fn run_marks_automation_failed_when_execution_errors() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);
    approve_suggestion(&db_path);

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["run", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(!output.status.success());

    let automations = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .arg("automations")
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(automations.status.success());
    assert!(String::from_utf8(automations.stdout)
        .unwrap()
        .contains("failed"));

    let conn = Connection::open(&db_path).unwrap();
    let failed_runs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM automation_runs WHERE result = 'failed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(failed_runs, 1);
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

fn seed_manual_automation(db_path: &Path, spec: AutomationSpec) {
    let conn = Connection::open(db_path).unwrap();
    run_migrations(&conn).unwrap();
    let spec_yaml = serde_yaml::to_string(&spec).unwrap();
    conn.execute(
        "INSERT INTO automations (suggestion_id, spec_yaml, state, summary, accepted_at) VALUES (NULL, ?1, 'active', 'manual automation', '2026-03-11T00:00:00Z')",
        [spec_yaml],
    )
    .unwrap();
}

fn rename_only_spec(inbox: &Path) -> AutomationSpec {
    AutomationSpec {
        id: "rename_only".to_string(),
        trigger: Trigger {
            r#type: "file_created".to_string(),
            path: Some(inbox.display().to_string()),
            extension: Some("pdf".to_string()),
            name_contains: Some("invoice".to_string()),
        },
        actions: vec![Action::Rename {
            template: "{stem}-reviewed.{ext}".to_string(),
        }],
        safety: Some(Safety {
            dry_run_first: true,
            undo_log: true,
        }),
    }
}

fn move_only_spec(inbox: &Path, archive: &Path) -> AutomationSpec {
    AutomationSpec {
        id: "move_only".to_string(),
        trigger: Trigger {
            r#type: "file_created".to_string(),
            path: Some(inbox.display().to_string()),
            extension: Some("pdf".to_string()),
            name_contains: Some("invoice".to_string()),
        },
        actions: vec![Action::Move {
            destination: archive.display().to_string(),
        }],
        safety: Some(Safety {
            dry_run_first: true,
            undo_log: true,
        }),
    }
}

fn rename_only_yaml(inbox: &Path) -> String {
    serde_yaml::to_string(&rename_only_spec(inbox)).unwrap()
}
