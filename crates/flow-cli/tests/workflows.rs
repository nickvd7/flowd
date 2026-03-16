use chrono::{DateTime, Utc};
use flow_adapters::file_watcher::FileEvent;
use flow_analysis::refresh_analysis_state;
use flow_db::{
    migrations::run_migrations,
    repo::{
        get_automation, insert_normalized_event_record, insert_raw_event,
        list_all_suggestion_records, list_automations, list_normalized_events, list_patterns,
        list_pending_file_raw_events, list_raw_events_after, list_recent_sessions, list_suggestions,
    },
};
use flow_dsl::{Action, AutomationSpec, Safety, Trigger};
use flow_exec::preview_automation;
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
    let conn = Connection::open(&db_path).unwrap();
    let rows: Vec<Vec<String>> = list_patterns(&conn)
        .unwrap()
        .into_iter()
        .map(|pattern| {
            vec![
                pattern.pattern_id.to_string(),
                format!("{:.3}", pattern.usefulness_score),
                render_pattern_name(&pattern.signature),
                pattern.count.to_string(),
                format_duration(pattern.avg_duration_ms),
                format_timestamp(&pattern.last_seen_at),
                render_pattern_example(&pattern.canonical_summary),
            ]
        })
        .collect();
    let expected = format_table(
        &[
            "id",
            "score",
            "pattern",
            "runs",
            "avg",
            "last_seen",
            "example",
        ],
        &rows,
    );
    assert_eq!(stdout, expected);
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
    let conn = Connection::open(&db_path).unwrap();
    let suggestions = list_suggestions(&conn).unwrap();
    assert_eq!(suggestions.len(), 1);
    assert_eq!(suggestions[0].shown_count, 1);
    assert!(suggestions[0].last_shown_ts.is_some());
    let rows: Vec<Vec<String>> = suggestions
        .into_iter()
        .map(|suggestion| {
            vec![
                suggestion.suggestion_id.to_string(),
                format!("{:.3}", suggestion.usefulness_score),
                render_pattern_name(&suggestion.signature),
                suggestion.count.to_string(),
                format_duration(suggestion.avg_duration_ms),
                suggestion.freshness,
                format_timestamp(&suggestion.last_seen_at),
                suggestion.proposal_text,
            ]
        })
        .collect();
    let expected = format_table(
        &[
            "id",
            "score",
            "pattern",
            "runs",
            "avg",
            "freshness",
            "last_seen",
            "description",
        ],
        &rows,
    );
    let expected = format!(
        "{}\nNext steps:\n1. Inspect one suggestion: flowctl suggestions explain 1\n2. Review suggestion history: flowctl suggestions history\n3. Approve a suggestion: flowctl approve 1\n",
        expected
    );
    assert_eq!(stdout, expected);
}

#[test]
fn suggestions_explain_renders_explanation_column() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["suggestions", "--explain"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("explain"));
    assert!(stdout.contains("baseline fallback"));
    assert!(stdout.contains(
        "Open-core baseline order and wording were used because intelligence was unavailable."
    ));
}

#[test]
fn suggestions_history_renders_deterministic_feedback_table() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);

    let reject = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["reject", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();
    assert!(reject.status.success());

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["suggestions", "history"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let conn = Connection::open(&db_path).unwrap();
    let rows: Vec<Vec<String>> = list_all_suggestion_records(&conn)
        .unwrap()
        .into_iter()
        .map(|suggestion| {
            let latest = render_latest_interaction(&suggestion);
            vec![
                suggestion.suggestion_id.to_string(),
                suggestion.status,
                render_pattern_name(&suggestion.signature),
                suggestion.shown_count.to_string(),
                suggestion.accepted_count.to_string(),
                suggestion.rejected_count.to_string(),
                suggestion.snoozed_count.to_string(),
                latest,
                suggestion.proposal_text,
            ]
        })
        .collect();
    let expected = format_table(
        &[
            "id",
            "status",
            "pattern",
            "shown",
            "accepted",
            "rejected",
            "snoozed",
            "latest",
            "description",
        ],
        &rows,
    );
    assert_eq!(stdout, expected);
}

#[test]
fn suggestions_show_renders_history_details() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);

    let snooze = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["snooze", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();
    assert!(snooze.status.success());

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["suggestions", "show", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let conn = Connection::open(&db_path).unwrap();
    let suggestion = list_all_suggestion_records(&conn).unwrap().remove(0);
    let expected = format_suggestion_history_report(&suggestion);
    assert_eq!(stdout, expected);
}

#[test]
fn suggestions_show_reports_missing_suggestion() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["suggestions", "show", "999"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("suggestion 999 not found"));
}

#[test]
fn intelligence_export_feedback_writes_deterministic_json_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    let export_path = temp_dir.path().join("exports").join("feedback.json");
    seed_database(&db_path);

    let suggest = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .arg("suggestions")
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();
    assert!(suggest.status.success());

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args([
            "intelligence",
            "export-feedback",
            "--output",
            export_path.to_str().unwrap(),
            "--generated-at",
            "2026-03-13T12:00:00+00:00",
        ])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout,
        format!(
            "Exported 1 suggestion records to {}\n",
            export_path.display()
        )
    );

    let exported = std::fs::read_to_string(&export_path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&exported).unwrap();
    assert_eq!(json["schema_name"], "flowd.intelligence_feedback_export");
    assert_eq!(json["export_version"], 1);
    assert_eq!(json["generated_at"], "2026-03-13T12:00:00+00:00");
    assert_eq!(json["context"]["candidate_count"], 1);
    assert_eq!(json["context"]["feedback_summary"]["shown_count"], 1);
    assert_eq!(json["suggestion_records"][0]["status"], "pending");
    assert_eq!(json["suggestion_records"][0]["feedback"]["shown_count"], 1);
    assert_eq!(
        json["suggestion_records"][0]["evaluation_context"]["pattern"]["count"],
        2
    );
    assert!(
        json["suggestion_records"][0]["evaluation_context"]["recency"]["reference_ts"].is_string()
    );
}

#[test]
fn intelligence_export_feedback_handles_empty_database() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    let export_path = temp_dir.path().join("feedback.json");
    let conn = Connection::open(&db_path).unwrap();
    run_migrations(&conn).unwrap();
    drop(conn);

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args([
            "intelligence",
            "export-feedback",
            "--output",
            export_path.to_str().unwrap(),
            "--generated-at",
            "2026-03-13T12:00:00+00:00",
        ])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let exported = std::fs::read_to_string(&export_path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&exported).unwrap();
    assert_eq!(json["schema_name"], "flowd.intelligence_feedback_export");
    assert_eq!(json["export_version"], 1);
    assert_eq!(json["context"]["candidate_count"], 0);
    assert_eq!(json["context"]["feedback_summary"]["shown_count"], 0);
    assert_eq!(json["suggestion_records"], serde_json::json!([]));
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
    let conn = Connection::open(&db_path).unwrap();
    let rows: Vec<Vec<String>> = list_recent_sessions(&conn, 20)
        .unwrap()
        .into_iter()
        .map(|session| {
            vec![
                session.session_id.to_string(),
                session.event_count.to_string(),
                format_duration(session.duration_ms),
                format_timestamp(&session.start_ts),
                format_timestamp(&session.end_ts),
            ]
        })
        .collect();
    let expected = format_table(&["id", "events", "duration", "start", "end"], &rows);
    assert_eq!(stdout, expected);
}

#[test]
fn watch_once_renders_deterministic_activity_snapshot() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["watch", "--once"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let conn = Connection::open(&db_path).unwrap();
    let mut expected_lines: Vec<String> = list_raw_events_after(&conn, 0)
        .unwrap()
        .iter()
        .filter_map(render_watch_raw_event)
        .collect();
    expected_lines.extend(
        list_patterns(&conn)
            .unwrap()
            .iter()
            .map(render_watch_pattern_detected),
    );
    expected_lines.extend(
        list_all_suggestion_records(&conn)
            .unwrap()
            .iter()
            .map(render_watch_suggestion_created),
    );

    let expected = if expected_lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", expected_lines.join("\n"))
    };
    assert_eq!(stdout, expected);
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
    assert!(approve_stdout.contains("Next steps:"));
    assert!(approve_stdout.contains("flowctl automations show 1"));
    assert!(approve_stdout.contains("flowctl dry-run 1"));
    assert!(approve_stdout.contains("flowctl run 1"));

    let automations = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .arg("automations")
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(automations.status.success());
    let stdout = String::from_utf8(automations.stdout).unwrap();
    assert!(stdout.contains("id"));
    assert!(stdout.contains("suggestion"));
    assert!(stdout.contains("status"));
    assert!(stdout.contains("runs"));
    assert!(stdout.contains("active"));
    assert!(stdout.contains("Repeated invoice file workflow detected"));

    let conn = Connection::open(&db_path).unwrap();
    let automation = list_automations(&conn).unwrap().remove(0);
    assert_eq!(automation.run_count, 0);
    assert_eq!(automation.status, "active");
    let accepted: (i64, Option<String>) = conn
        .query_row(
            "SELECT accepted_count, last_accepted_ts FROM suggestions WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(accepted.0, 1);
    assert!(accepted.1.is_some());
}

#[test]
fn reject_updates_feedback_history_and_hides_suggestion() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);

    let reject = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["reject", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(reject.status.success());
    assert!(String::from_utf8(reject.stdout)
        .unwrap()
        .contains("Rejected suggestion 1."));

    let conn = Connection::open(&db_path).unwrap();
    let feedback: (String, i64, Option<String>) = conn
        .query_row(
            "SELECT status, rejected_count, last_rejected_ts FROM suggestions WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(feedback.0, "rejected");
    assert_eq!(feedback.1, 1);
    assert!(feedback.2.is_some());
    assert!(list_suggestions(&conn).unwrap().is_empty());
}

#[test]
fn snooze_updates_feedback_history_and_hides_suggestion() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);

    let snooze = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["snooze", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(snooze.status.success());
    assert!(String::from_utf8(snooze.stdout)
        .unwrap()
        .contains("Snoozed suggestion 1."));

    let conn = Connection::open(&db_path).unwrap();
    let feedback: (String, i64, Option<String>) = conn
        .query_row(
            "SELECT status, snoozed_count, last_snoozed_ts FROM suggestions WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(feedback.0, "snoozed");
    assert_eq!(feedback.1, 1);
    assert!(feedback.2.is_some());
    assert!(list_suggestions(&conn).unwrap().is_empty());
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
fn automations_show_renders_detailed_report() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);
    approve_suggestion(&db_path);

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["automations", "show", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let conn = Connection::open(&db_path).unwrap();
    let automation = get_automation(&conn, 1).unwrap().unwrap();
    let spec = flow_dsl::parse_spec(&automation.spec_yaml).unwrap();
    let preview = preview_automation(&conn, 1).unwrap();
    let expected = format_automation_report(&automation, &spec, &preview);
    let expected = format!(
        "{}\n\nNext steps:\n1. Preview this automation: flowctl dry-run 1\n2. Run this automation: flowctl run 1\n3. Review automation run history: flowctl runs\n",
        expected.trim_end()
    );
    assert_eq!(stdout, expected);
    assert!(stdout.contains("trigger:"));
    assert!(stdout.contains("actions:"));
    assert!(stdout.contains("Automation preview"));
    assert!(stdout.contains("rename template="));
    assert!(stdout.contains("move destination="));
}

#[test]
fn automations_show_preview_is_deterministic() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);
    approve_suggestion(&db_path);

    std::fs::create_dir_all(temp_dir.path().join("inbox")).unwrap();
    std::fs::write(temp_dir.path().join("inbox/invoice-1009.pdf"), "invoice").unwrap();

    let first = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["automations", "show", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();
    let second = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["automations", "show", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(first.status.success());
    assert!(second.status.success());
    assert_eq!(first.stdout, second.stdout);
}

#[test]
fn automations_show_preview_handles_missing_context_without_failing() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    let missing_inbox = temp_dir.path().join("missing");
    let archive = temp_dir.path().join("archive");
    seed_manual_automation(&db_path, rename_and_move_spec(&missing_inbox, &archive));

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["automations", "show", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Automation preview"));
    assert!(stdout.contains("affected file count unavailable"));
    assert!(stdout.contains("Best-effort preview only:"));
}

#[test]
fn automations_show_reports_missing_id() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database(&db_path);

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["automations", "show", "999"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("automation 999 not found"));
}

#[test]
fn automations_show_reflects_current_status() {
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

    let output = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["automations", "show", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("status: disabled"));
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
fn full_open_core_loop_runs_from_observed_events_through_undo() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("flowd.db");
    seed_database_from_observed_events(&db_path);

    let conn = Connection::open(&db_path).unwrap();
    assert_eq!(list_pending_file_raw_events(&conn).unwrap().len(), 0);
    assert!(list_normalized_events(&conn).unwrap().len() >= 6);
    assert_eq!(list_patterns(&conn).unwrap().len(), 1);
    assert_eq!(list_suggestions(&conn).unwrap().len(), 1);

    let suggest = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .arg("suggest")
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();
    assert!(suggest.status.success());
    let suggest_stdout = String::from_utf8(suggest.stdout).unwrap();
    assert!(suggest_stdout.contains("Repeated invoice file workflow detected"));

    let approve = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["approve", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();
    assert!(approve.status.success());
    assert!(String::from_utf8(approve.stdout)
        .unwrap()
        .contains("Approved suggestion 1 as automation 1"));

    std::fs::create_dir_all(temp_dir.path().join("inbox")).unwrap();
    std::fs::write(temp_dir.path().join("inbox/invoice-2001.pdf"), "invoice").unwrap();

    let dry_run = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["dry-run", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();
    assert!(dry_run.status.success());
    let dry_run_stdout = String::from_utf8(dry_run.stdout).unwrap();
    assert!(dry_run_stdout.contains("rename:"));
    assert!(dry_run_stdout.contains("move:"));
    assert!(temp_dir.path().join("inbox/invoice-2001.pdf").exists());
    assert!(!temp_dir
        .path()
        .join("archive/invoice-2001-reviewed.pdf")
        .exists());

    let run = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["run", "1"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();
    assert!(run.status.success());
    let run_stdout = String::from_utf8(run.stdout).unwrap();
    assert!(run_stdout.contains("rename:"));
    assert!(run_stdout.contains("move:"));
    assert!(!temp_dir.path().join("inbox/invoice-2001.pdf").exists());
    assert!(temp_dir
        .path()
        .join("archive/invoice-2001-reviewed.pdf")
        .exists());

    let runs = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .arg("runs")
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();
    assert!(runs.status.success());
    let runs_stdout = String::from_utf8(runs.stdout).unwrap();
    assert!(runs_stdout.contains("dry_run"));
    assert!(runs_stdout.contains("completed"));

    let undo = Command::new(env!("CARGO_BIN_EXE_flow-cli"))
        .args(["undo", "2"])
        .env("FLOWD_DB_PATH", &db_path)
        .output()
        .unwrap();
    assert!(undo.status.success());
    let undo_stdout = String::from_utf8(undo.stdout).unwrap();
    let move_index = undo_stdout.find("move:").unwrap();
    let rename_index = undo_stdout.find("rename:").unwrap();
    assert!(move_index < rename_index);
    assert!(undo_stdout.contains("Undid automation run 2."));
    assert!(temp_dir.path().join("inbox/invoice-2001.pdf").exists());
    assert!(!temp_dir
        .path()
        .join("archive/invoice-2001-reviewed.pdf")
        .exists());

    let conn = Connection::open(&db_path).unwrap();
    let run_results: Vec<String> = {
        let mut statement = conn
            .prepare("SELECT result FROM automation_runs ORDER BY id ASC")
            .unwrap();
        statement
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert_eq!(run_results, vec!["dry_run", "completed", "undone"]);
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
    assert!(stdout.contains("automation"));
    assert!(stdout.contains("ops"));
    assert!(stdout.contains("completed"));
    assert!(stdout.contains("2"));
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
    let raw_events: Vec<_> = fixture_file_events(db_path)
        .into_iter()
        .map(FileEvent::into_raw_event)
        .collect();
    let normalized: Vec<_> = raw_events.iter().filter_map(normalize).collect();

    for event in &normalized {
        insert_normalized_event_record(&mut conn, event).unwrap();
    }

    refresh_analysis_state(&mut conn, 300).unwrap();
}

fn seed_database_from_observed_events(db_path: &Path) {
    let mut conn = Connection::open(db_path).unwrap();
    run_migrations(&conn).unwrap();

    for event in fixture_file_events(db_path) {
        insert_raw_event(&conn, &event.into_raw_event()).unwrap();
    }

    let pending = list_pending_file_raw_events(&conn).unwrap();
    let normalized: Vec<_> = pending
        .into_iter()
        .filter_map(|record| normalize(&record.event).map(|event| (record.id, event)))
        .collect();
    flow_db::repo::insert_normalized_events_for_raw_events(&mut conn, &normalized).unwrap();
    refresh_analysis_state(&mut conn, 300).unwrap();
}

fn fixture_file_events(db_path: &Path) -> Vec<FileEvent> {
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

    file_events
}

fn format_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = headers.iter().map(|header| header.len()).collect();
    for row in rows {
        for (index, value) in row.iter().enumerate() {
            widths[index] = widths[index].max(value.len());
        }
    }

    let mut lines = Vec::with_capacity(rows.len() + 2);
    lines.push(format_row(headers.iter().copied(), &widths));
    lines.push(
        widths
            .iter()
            .map(|width| "-".repeat(*width))
            .collect::<Vec<_>>()
            .join("-+-"),
    );
    for row in rows {
        lines.push(format_row(row.iter(), &widths));
    }

    format!("{}\n", lines.join("\n"))
}

fn format_automation_report(
    automation: &flow_db::repo::StoredAutomationSpec,
    spec: &AutomationSpec,
    preview: &flow_exec::AutomationPreview,
) -> String {
    let mut lines = vec![
        format!("automation: {}", automation.automation_id),
        format!("spec_id: {}", spec.id),
        format!("title: {}", render_optional_value(&automation.summary)),
        format!("status: {}", automation.status),
        format!(
            "suggestion: {}",
            automation
                .suggestion_id
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string())
        ),
        format!(
            "accepted: {}",
            automation
                .accepted_at
                .as_deref()
                .map(format_timestamp)
                .unwrap_or_else(|| "-".to_string())
        ),
        "trigger:".to_string(),
        format!("  type: {}", spec.trigger.r#type),
        format!(
            "  path: {}",
            render_optional_value(spec.trigger.path.as_deref().unwrap_or(""))
        ),
        format!(
            "  extension: {}",
            render_optional_value(spec.trigger.extension.as_deref().unwrap_or(""))
        ),
        format!(
            "  name_contains: {}",
            render_optional_value(spec.trigger.name_contains.as_deref().unwrap_or(""))
        ),
        "actions:".to_string(),
    ];

    if spec.actions.is_empty() {
        lines.push("  - none".to_string());
    } else {
        lines.extend(
            spec.actions.iter().enumerate().map(|(index, action)| {
                format!("  {}. {}", index + 1, render_action_details(action))
            }),
        );
    }

    lines.push("safety:".to_string());
    if let Some(safety) = &spec.safety {
        lines.push(format!("  dry_run_first: {}", safety.dry_run_first));
        lines.push(format!("  undo_log: {}", safety.undo_log));
    } else {
        lines.push("  - none".to_string());
    }

    lines.push(String::new());
    lines.extend(render_automation_preview(preview));

    format!("{}\n", lines.join("\n"))
}

fn format_suggestion_history_report(suggestion: &flow_db::repo::StoredSuggestionRecord) -> String {
    let lines = vec![
        format!("suggestion: {}", suggestion.suggestion_id),
        format!("status: {}", suggestion.status),
        format!("pattern: {}", suggestion.canonical_summary),
        format!("signature: {}", suggestion.signature),
        format!("proposal: {}", suggestion.proposal_text),
        format!(
            "feedback: shown={}, accepted={}, rejected={}, snoozed={}",
            suggestion.shown_count,
            suggestion.accepted_count,
            suggestion.rejected_count,
            suggestion.snoozed_count
        ),
        format!("latest: {}", render_latest_interaction(suggestion)),
        format!(
            "last_shown: {}",
            suggestion
                .last_shown_ts
                .as_deref()
                .map(format_timestamp)
                .unwrap_or_else(|| "-".to_string())
        ),
        format!(
            "last_accepted: {}",
            suggestion
                .last_accepted_ts
                .as_deref()
                .map(format_timestamp)
                .unwrap_or_else(|| "-".to_string())
        ),
        format!(
            "last_rejected: {}",
            suggestion
                .last_rejected_ts
                .as_deref()
                .map(format_timestamp)
                .unwrap_or_else(|| "-".to_string())
        ),
        format!(
            "last_snoozed: {}",
            suggestion
                .last_snoozed_ts
                .as_deref()
                .map(format_timestamp)
                .unwrap_or_else(|| "-".to_string())
        ),
    ];

    format!("{}\n", lines.join("\n"))
}

fn render_automation_preview(preview: &flow_exec::AutomationPreview) -> Vec<String> {
    let mut lines = vec![
        "Automation preview".to_string(),
        String::new(),
        "Estimated impact:".to_string(),
    ];

    lines.push(match preview.estimated_affected_files {
        Some(count) if preview.exact_count => format!("- affects {count} files"),
        Some(count) => format!("- affects approximately {count} files"),
        None => "- affected file count unavailable".to_string(),
    });

    lines.push(String::new());
    lines.push("Examples:".to_string());
    if preview.examples.is_empty() {
        lines.push("- no representative examples available".to_string());
    } else {
        lines.extend(
            preview
                .examples
                .iter()
                .map(|example| format!("- {} -> {}", example.before, example.after)),
        );
    }

    lines.push(String::new());
    lines.push("Destination:".to_string());
    if preview.destination_paths.is_empty() {
        lines.push("- destination unavailable".to_string());
    } else {
        lines.extend(
            preview
                .destination_paths
                .iter()
                .map(|destination| format!("- {destination}")),
        );
    }

    lines.push(String::new());
    lines.push("Risk:".to_string());
    lines.push(format!("- {}", preview.risk.as_str()));

    lines.push(String::new());
    lines.push("Action summary:".to_string());
    if preview.action_summary.is_empty() {
        lines.push("- action summary unavailable".to_string());
    } else {
        lines.extend(
            preview
                .action_summary
                .iter()
                .map(|action| format!("- {action}")),
        );
    }

    if !preview.notes.is_empty() {
        lines.push(String::new());
        lines.push("Notes:".to_string());
        lines.extend(preview.notes.iter().map(|note| format!("- {note}")));
    }

    lines
}

fn format_row<'a, I, T>(values: I, widths: &[usize]) -> String
where
    I: IntoIterator<Item = T>,
    T: std::fmt::Display,
{
    values
        .into_iter()
        .enumerate()
        .map(|(index, value)| format!("{value:<width$}", width = widths[index]))
        .collect::<Vec<_>>()
        .join(" | ")
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

fn render_pattern_name(signature: &str) -> String {
    let mut groups = Vec::new();
    for group in signature
        .split("->")
        .map(|part| part.split(':').nth(1).unwrap_or("file").replace('-', "_"))
    {
        if groups.last() != Some(&group) {
            groups.push(group);
        }
    }
    groups.push("workflow".to_string());
    groups.join("_")
}

fn render_pattern_example(summary: &str) -> String {
    summary
        .split(" -> ")
        .map(render_action_label)
        .collect::<Vec<_>>()
        .join(" -> ")
}

fn render_action_label(action: &str) -> String {
    let normalized = action.strip_suffix("File").unwrap_or(action);
    let mut label = String::new();

    for (index, ch) in normalized.chars().enumerate() {
        if ch.is_uppercase() && index > 0 {
            label.push(' ');
        }
        label.push(ch.to_ascii_lowercase());
    }

    label
}

fn render_latest_interaction(suggestion: &flow_db::repo::StoredSuggestionRecord) -> String {
    [
        ("shown", suggestion.last_shown_ts.as_deref()),
        ("accepted", suggestion.last_accepted_ts.as_deref()),
        ("rejected", suggestion.last_rejected_ts.as_deref()),
        ("snoozed", suggestion.last_snoozed_ts.as_deref()),
    ]
    .into_iter()
    .filter_map(|(label, value)| value.map(|timestamp| (label, timestamp)))
    .max_by(|left, right| left.1.cmp(right.1))
    .map(|(label, timestamp)| format!("{label} {}", format_timestamp(timestamp)))
    .unwrap_or_else(|| "-".to_string())
}

fn render_action_details(action: &Action) -> String {
    match action {
        Action::Rename { template } => format!("rename template={template}"),
        Action::Move { destination } => format!("move destination={destination}"),
    }
}

fn render_optional_value(value: &str) -> String {
    if value.trim().is_empty() {
        "-".to_string()
    } else {
        value.to_string()
    }
}

fn render_watch_raw_event(record: &flow_db::repo::StoredRawEvent) -> Option<String> {
    let event = &record.event;
    match event.source {
        flow_core::events::EventSource::FileWatcher => {
            let kind = event
                .payload
                .get("kind")
                .and_then(|value| value.as_str())
                .unwrap_or("event");
            let path = event
                .payload
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or("-");
            let from_path = event
                .payload
                .get("from_path")
                .and_then(|value| value.as_str());

            match (kind, from_path) {
                ("rename", Some(from_path)) | ("move", Some(from_path)) => {
                    Some(format!("[event] {kind}: {from_path} -> {path}"))
                }
                ("create", _) => Some(format!("[event] file created: {path}")),
                ("remove" | "delete", _) => Some(format!("[event] file removed: {path}")),
                ("write" | "modify" | "access", _) => None,
                _ => Some(format!("[event] file {kind}: {path}")),
            }
        }
        flow_core::events::EventSource::Terminal => {
            let command = event
                .payload
                .get("redacted_command")
                .and_then(|value| value.as_str())
                .unwrap_or("command");
            let kind = event
                .payload
                .get("kind")
                .and_then(|value| value.as_str())
                .unwrap_or("command");
            Some(format!("[event] terminal {kind}: {command}"))
        }
        flow_core::events::EventSource::Clipboard => None,
        flow_core::events::EventSource::Browser => match event
            .payload
            .get("kind")
            .and_then(|value| value.as_str())
            .unwrap_or("browser")
        {
            "download" => {
                let path = event
                    .payload
                    .get("path")
                    .and_then(|value| value.as_str())
                    .or_else(|| {
                        event
                            .payload
                            .get("filename")
                            .and_then(|value| value.as_str())
                    })
                    .unwrap_or("-");
                Some(format!("[event] browser download: {path}"))
            }
            "visit" => {
                let url = event
                    .payload
                    .get("url")
                    .and_then(|value| value.as_str())
                    .unwrap_or("-");
                Some(format!("[event] browser visit: {url}"))
            }
            _ => None,
        },
        flow_core::events::EventSource::ActiveWindow => None,
    }
}

fn render_watch_pattern_detected(pattern: &flow_db::repo::StoredPattern) -> String {
    format!(
        "[pattern] candidate detected: {} (repetitions: {})",
        render_pattern_name(&pattern.signature),
        pattern.count,
    )
}

fn render_watch_suggestion_created(suggestion: &flow_db::repo::StoredSuggestionRecord) -> String {
    format!("[suggestion] new: {}", suggestion.proposal_text)
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

fn rename_and_move_spec(inbox: &Path, archive: &Path) -> AutomationSpec {
    AutomationSpec {
        id: "rename_and_move".to_string(),
        trigger: Trigger {
            r#type: "file_created".to_string(),
            path: Some(inbox.display().to_string()),
            extension: Some("pdf".to_string()),
            name_contains: Some("invoice".to_string()),
        },
        actions: vec![
            Action::Rename {
                template: "{stem}-reviewed.{ext}".to_string(),
            },
            Action::Move {
                destination: archive.display().to_string(),
            },
        ],
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
