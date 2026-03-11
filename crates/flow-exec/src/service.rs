use crate::engine::{dry_run, execute, plan, ExecutionReport};
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use flow_core::events::{ActionType, NormalizedEvent};
use flow_db::repo::{
    get_automation, get_suggestion, insert_automation, insert_automation_run,
    load_example_events_for_pattern, set_automation_status, set_suggestion_status,
    AutomationRunRecord, AUTOMATION_STATUS_ACTIVE, AUTOMATION_STATUS_DISABLED,
    AUTOMATION_STATUS_FAILED,
};
use flow_dsl::{Action, AutomationSpec, Safety, Trigger};
use rusqlite::Connection;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct DryRunOutcome {
    pub preview: Vec<String>,
    pub report: ExecutionReport,
}

/// The execution layer owns approval, planning, execution, and
/// `automation_runs` persistence. It operates on suggestions and automations,
/// not on raw event capture or analysis rebuilding.
pub fn approve_suggestion(conn: &mut Connection, suggestion_id: i64) -> Result<i64> {
    let suggestion = get_suggestion(conn, suggestion_id)
        .context("failed to read suggestion")?
        .ok_or_else(|| anyhow!("suggestion {suggestion_id} not found"))?;

    if suggestion.status != "pending" {
        bail!(
            "suggestion {} is not pending; current status is {}",
            suggestion.suggestion_id,
            suggestion.status
        );
    }

    let events = load_example_events_for_pattern(conn, suggestion.pattern_id)
        .context("failed to load example events for suggestion")?;
    let spec = compile_automation_spec(&suggestion.proposal_text, &events)
        .context("failed to compile automation")?;
    let spec_yaml = serde_yaml::to_string(&spec).context("failed to serialize automation")?;
    let accepted_at = Utc::now().to_rfc3339();

    let tx = conn
        .transaction()
        .context("failed to start approval transaction")?;
    set_suggestion_status(&tx, suggestion_id, "approved")
        .context("failed to update suggestion status")?;
    let automation_id = insert_automation(
        &tx,
        suggestion_id,
        &spec_yaml,
        AUTOMATION_STATUS_ACTIVE,
        &suggestion.proposal_text,
        &accepted_at,
    )
    .context("failed to store automation")?;
    tx.commit()
        .context("failed to commit approval transaction")?;

    Ok(automation_id)
}

pub fn dry_run_automation(conn: &Connection, automation_id: i64) -> Result<DryRunOutcome> {
    let spec = load_automation_spec(conn, automation_id)?;
    let preview = dry_run(&spec).context("failed to preview automation")?;
    let report = plan(&spec).context("failed to plan automation")?;
    store_run_record(conn, automation_id, "dry_run", &report)?;
    Ok(DryRunOutcome { preview, report })
}

pub fn execute_automation(conn: &Connection, automation_id: i64) -> Result<ExecutionReport> {
    let stored = load_stored_automation(conn, automation_id)?;
    ensure_automation_status(automation_id, &stored.status)?;
    let spec = flow_dsl::parse_spec(&stored.spec_yaml).context("failed to parse automation")?;
    let report = match execute(&spec) {
        Ok(report) => report,
        Err(error) => {
            set_automation_status(conn, automation_id, AUTOMATION_STATUS_FAILED)
                .context("failed to update automation status")?;
            store_failed_run_record(conn, automation_id, &error.to_string())?;
            return Err(error).context("failed to execute automation");
        }
    };
    store_run_record(conn, automation_id, "completed", &report)?;
    Ok(report)
}

fn load_automation_spec(conn: &Connection, automation_id: i64) -> Result<AutomationSpec> {
    let stored = load_stored_automation(conn, automation_id)?;
    flow_dsl::parse_spec(&stored.spec_yaml).context("failed to parse automation")
}

fn load_stored_automation(
    conn: &Connection,
    automation_id: i64,
) -> Result<flow_db::repo::StoredAutomationSpec> {
    get_automation(conn, automation_id)
        .context("failed to read automation")?
        .ok_or_else(|| anyhow!("automation {automation_id} not found"))
}

fn ensure_automation_status(automation_id: i64, status: &str) -> Result<()> {
    if status == AUTOMATION_STATUS_ACTIVE {
        return Ok(());
    }

    if status == AUTOMATION_STATUS_DISABLED {
        bail!("automation {automation_id} is disabled");
    }

    bail!("automation {automation_id} is not active; current status is {status}");
}

fn store_run_record(
    conn: &Connection,
    automation_id: i64,
    result: &str,
    report: &ExecutionReport,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    let payload = serde_json::to_string(report).context("failed to serialize run report")?;
    insert_automation_run(
        conn,
        &AutomationRunRecord {
            automation_id,
            started_at: &now,
            finished_at: &now,
            result,
            undo_payload_json: Some(&payload),
        },
    )
    .context("failed to insert automation run")?;
    Ok(())
}

fn store_failed_run_record(conn: &Connection, automation_id: i64, error: &str) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    let payload = serde_json::json!({ "error": error }).to_string();
    insert_automation_run(
        conn,
        &AutomationRunRecord {
            automation_id,
            started_at: &now,
            finished_at: &now,
            result: AUTOMATION_STATUS_FAILED,
            undo_payload_json: Some(&payload),
        },
    )
    .context("failed to insert automation run")?;
    Ok(())
}

pub fn disable_automation(conn: &Connection, automation_id: i64) -> Result<()> {
    let stored = load_stored_automation(conn, automation_id)?;
    if stored.status == AUTOMATION_STATUS_DISABLED {
        return Ok(());
    }

    set_automation_status(conn, automation_id, AUTOMATION_STATUS_DISABLED)
        .context("failed to update automation status")?;
    Ok(())
}

pub fn enable_automation(conn: &Connection, automation_id: i64) -> Result<()> {
    load_stored_automation(conn, automation_id)?;
    set_automation_status(conn, automation_id, AUTOMATION_STATUS_ACTIVE)
        .context("failed to update automation status")?;
    Ok(())
}

fn compile_automation_spec(summary: &str, events: &[NormalizedEvent]) -> Result<AutomationSpec> {
    if events.is_empty() {
        bail!("no example events available");
    }

    let create_event = events
        .iter()
        .find(|event| event.action_type == ActionType::CreateFile)
        .ok_or_else(|| anyhow!("create event is required for approval"))?;
    let source_path = create_event
        .target
        .as_deref()
        .ok_or_else(|| anyhow!("create event path is missing"))?;
    let source_dir = Path::new(source_path)
        .parent()
        .ok_or_else(|| anyhow!("source directory is missing"))?;
    let extension = Path::new(source_path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_string());
    let name_contains = create_event
        .metadata
        .get("file_group")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());

    let mut actions = Vec::new();

    if let Some(rename_event) = events
        .iter()
        .find(|event| event.action_type == ActionType::RenameFile)
    {
        let from_path = rename_event
            .metadata
            .get("from_path")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow!("rename source path is missing"))?;
        let to_path = rename_event
            .target
            .as_deref()
            .ok_or_else(|| anyhow!("rename destination path is missing"))?;
        let template = compile_rename_template(from_path, to_path)?;
        actions.push(Action::Rename { template });
    }

    if let Some(move_event) = events
        .iter()
        .find(|event| event.action_type == ActionType::MoveFile)
    {
        let destination = move_event
            .target
            .as_deref()
            .ok_or_else(|| anyhow!("move destination path is missing"))?;
        let destination_dir = Path::new(destination)
            .parent()
            .ok_or_else(|| anyhow!("move destination directory is missing"))?;
        actions.push(Action::Move {
            destination: destination_dir.display().to_string(),
        });
    }

    if actions.is_empty() {
        bail!("only rename and move suggestions can be approved");
    }

    Ok(AutomationSpec {
        id: sanitize_id(summary),
        trigger: Trigger {
            r#type: "file_created".to_string(),
            path: Some(source_dir.display().to_string()),
            extension,
            name_contains,
        },
        actions,
        safety: Some(Safety {
            dry_run_first: true,
            undo_log: true,
        }),
    })
}

fn compile_rename_template(from_path: &str, to_path: &str) -> Result<String> {
    let from = Path::new(from_path);
    let to = Path::new(to_path);

    let from_stem = from
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("rename source stem is missing"))?;
    let to_stem = to
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("rename destination stem is missing"))?;
    let from_ext = from
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let to_ext = to
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");

    if from_stem == to_stem && from_ext == to_ext {
        bail!("rename step does not change the file name");
    }

    if to_stem.starts_with(from_stem) && from_ext == to_ext {
        let suffix = &to_stem[from_stem.len()..];
        if from_ext.is_empty() {
            return Ok(format!("{{stem}}{suffix}"));
        }
        return Ok(format!("{{stem}}{suffix}.{{ext}}"));
    }

    bail!("unsupported rename pattern for v1")
}

fn sanitize_id(summary: &str) -> String {
    let mut id = String::new();

    for ch in summary.chars() {
        if ch.is_ascii_alphanumeric() {
            id.push(ch.to_ascii_lowercase());
        } else if !id.ends_with('_') {
            id.push('_');
        }
    }

    id.trim_matches('_').to_string()
}
