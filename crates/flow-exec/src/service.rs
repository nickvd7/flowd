use crate::engine::{
    dry_run, execute, execute_report, plan, plan_undo, ExecutionReport, StoredExecutionReport,
};
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use flow_core::events::{ActionType, NormalizedEvent};
use flow_db::repo::{
    get_automation, get_suggestion, increment_accepted, insert_automation, insert_automation_run,
    list_automation_runs, load_automation_run, load_example_events_for_pattern,
    set_automation_status, set_suggestion_status, AutomationRunRecord, StoredAutomationRun,
    AUTOMATION_STATUS_ACTIVE, AUTOMATION_STATUS_DISABLED, AUTOMATION_STATUS_FAILED,
};
use flow_dsl::{Action, AutomationSpec, Safety, Trigger};
use rusqlite::Connection;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Debug, Clone)]
pub struct DryRunOutcome {
    pub preview: Vec<String>,
    pub report: ExecutionReport,
}

#[derive(Debug, Clone)]
pub struct UndoOutcome {
    pub source_run_id: i64,
    pub report: ExecutionReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutomationPreview {
    pub estimated_affected_files: Option<usize>,
    pub exact_count: bool,
    pub examples: Vec<PreviewExample>,
    pub destination_paths: Vec<String>,
    pub action_summary: Vec<String>,
    pub risk: PreviewRisk,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewExample {
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewRisk {
    Low,
    Medium,
    High,
}

impl PreviewRisk {
    pub fn as_str(self) -> &'static str {
        match self {
            PreviewRisk::Low => "low",
            PreviewRisk::Medium => "medium",
            PreviewRisk::High => "high",
        }
    }
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
    increment_accepted(&tx, suggestion_id).context("failed to record suggestion acceptance")?;
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

pub fn list_runs(conn: &Connection) -> Result<Vec<StoredAutomationRun>> {
    list_automation_runs(conn).context("failed to read automation runs")
}

pub fn preview_automation(conn: &Connection, automation_id: i64) -> Result<AutomationPreview> {
    let spec = load_automation_spec(conn, automation_id)?;
    Ok(build_preview(&spec, None))
}

pub fn preview_suggestion(conn: &Connection, suggestion_id: i64) -> Result<AutomationPreview> {
    let suggestion = get_suggestion(conn, suggestion_id)
        .context("failed to read suggestion")?
        .ok_or_else(|| anyhow!("suggestion {suggestion_id} not found"))?;
    let events = load_example_events_for_pattern(conn, suggestion.pattern_id)
        .context("failed to load example events for suggestion")?;

    match compile_automation_spec(&suggestion.proposal_text, &events) {
        Ok(spec) => Ok(build_preview(&spec, Some(&events))),
        Err(error) => Ok(best_effort_preview_without_spec(
            &events,
            vec![format!("Best-effort preview only: {error}")],
        )),
    }
}

/// Undo is explicit and per-run: `flow-cli undo <run_id>` only targets one
/// completed automation run and rebuilds its inverse plan from stored run
/// metadata. There is no bulk undo path because inspectable, deterministic
/// single-run reversal is the safety boundary.
pub fn undo_automation_run(conn: &Connection, run_id: i64) -> Result<UndoOutcome> {
    let run = load_automation_run(conn, run_id)
        .context("failed to read automation run")?
        .ok_or_else(|| anyhow!("automation run {run_id} not found"))?;

    let report = match load_completed_execution_report(&run) {
        Ok(report) => report,
        Err(error) => {
            store_failed_undo_run_record(conn, run.automation_id, run.run_id, &error.to_string())?;
            return Err(error).context("failed to prepare undo");
        }
    };

    // Undo builds the inverse plan by swapping each recorded `from` and `to`
    // pair and then reversing the full operation order. Reversal matters
    // because later filesystem mutations depend on earlier ones, so undo must
    // reestablish intermediate paths before it can restore original names.
    let undo_plan = match plan_undo(&report) {
        Ok(plan) => plan,
        Err(error) => {
            store_failed_undo_run_record(conn, run.automation_id, run.run_id, &error.to_string())?;
            return Err(error).context("failed to derive undo plan");
        }
    };

    let executed = match execute_report(&undo_plan) {
        Ok(report) => report,
        Err(error) => {
            store_failed_undo_run_record(conn, run.automation_id, run.run_id, &error.to_string())?;
            return Err(error).context("failed to execute undo");
        }
    };

    store_undo_run_record(conn, run.automation_id, run.run_id, &executed)?;
    Ok(UndoOutcome {
        source_run_id: run.run_id,
        report: executed,
    })
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
    // Undo requires complete execution metadata for every finished run. The
    // stored report is the deterministic audit trail that later allows an
    // inverse plan to be reconstructed without guessing from the live
    // filesystem state.
    let payload = serde_json::to_string(&StoredExecutionReport::from(report))
        .context("failed to serialize run report")?;
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

fn store_undo_run_record(
    conn: &Connection,
    automation_id: i64,
    source_run_id: i64,
    report: &ExecutionReport,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    let payload = serde_json::json!({
        "kind": "undo",
        "source_run_id": source_run_id,
        "report": StoredExecutionReport::from(report),
    })
    .to_string();
    insert_automation_run(
        conn,
        &AutomationRunRecord {
            automation_id,
            started_at: &now,
            finished_at: &now,
            result: "undone",
            undo_payload_json: Some(&payload),
        },
    )
    .context("failed to insert undo automation run")?;
    Ok(())
}

fn store_failed_undo_run_record(
    conn: &Connection,
    automation_id: i64,
    source_run_id: i64,
    error: &str,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    let payload = serde_json::json!({
        "kind": "undo_failed",
        "source_run_id": source_run_id,
        "error": error,
    })
    .to_string();
    insert_automation_run(
        conn,
        &AutomationRunRecord {
            automation_id,
            started_at: &now,
            finished_at: &now,
            result: "undo_failed",
            undo_payload_json: Some(&payload),
        },
    )
    .context("failed to insert failed undo automation run")?;
    Ok(())
}

fn load_completed_execution_report(run: &StoredAutomationRun) -> Result<ExecutionReport> {
    if run.result != "completed" {
        bail!(
            "automation run {} is not undoable; current result is {}",
            run.run_id,
            run.result
        );
    }

    if run.finished_at.is_none() {
        bail!("automation run {} is incomplete", run.run_id);
    }

    let payload = run.undo_payload_json.as_deref().ok_or_else(|| {
        anyhow!(
            "automation run {} is missing execution metadata",
            run.run_id
        )
    })?;
    let report: StoredExecutionReport =
        serde_json::from_str(payload).context("failed to parse stored execution metadata")?;

    if report.operations.is_empty() {
        bail!("automation run {} has no reversible operations", run.run_id);
    }

    Ok(report.into())
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

fn build_preview(
    spec: &AutomationSpec,
    example_events: Option<&[NormalizedEvent]>,
) -> AutomationPreview {
    let action_summary = action_summary_from_spec(spec);
    let destination_paths_from_spec = destination_paths_from_spec(spec);

    match plan(spec) {
        Ok(report) => {
            let mut notes = Vec::new();
            let estimated_affected_files = Some(count_affected_files(&report));
            let exact_count = true;
            let destination_paths = if report.operations.is_empty() {
                destination_paths_from_spec
            } else {
                destination_paths_from_report(&report)
            };
            let examples = if report.operations.is_empty() {
                let fallback = example_events
                    .map(preview_examples_from_events)
                    .unwrap_or_default();
                if !fallback.is_empty() {
                    notes.push(
                        "No matching files are currently available; examples come from stored workflow history."
                            .to_string(),
                    );
                }
                fallback
            } else {
                preview_examples_from_report(&report)
            };

            AutomationPreview {
                estimated_affected_files,
                exact_count,
                examples,
                destination_paths,
                action_summary: action_summary.clone(),
                risk: assess_risk(spec, estimated_affected_files, exact_count),
                notes,
            }
        }
        Err(error) => {
            let mut notes = vec![format!("Best-effort preview only: {error}")];
            let event_examples = example_events
                .map(preview_examples_from_events)
                .unwrap_or_default();
            if event_examples.is_empty() {
                notes.push(
                    "No representative examples are available from stored workflow history."
                        .to_string(),
                );
            }

            AutomationPreview {
                estimated_affected_files: None,
                exact_count: false,
                examples: event_examples,
                destination_paths: example_events
                    .map(destination_paths_from_events)
                    .filter(|paths| !paths.is_empty())
                    .unwrap_or(destination_paths_from_spec),
                action_summary,
                risk: assess_risk(spec, None, false),
                notes,
            }
        }
    }
}

fn best_effort_preview_without_spec(
    events: &[NormalizedEvent],
    mut notes: Vec<String>,
) -> AutomationPreview {
    let action_summary = action_summary_from_events(events);
    let examples = preview_examples_from_events(events);
    let destination_paths = destination_paths_from_events(events);

    if events.is_empty() {
        notes.push("No stored example events are available for this suggestion.".to_string());
    }

    AutomationPreview {
        estimated_affected_files: None,
        exact_count: false,
        examples,
        destination_paths,
        action_summary,
        risk: if events.is_empty() {
            PreviewRisk::High
        } else {
            PreviewRisk::Medium
        },
        notes,
    }
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

fn action_summary_from_spec(spec: &AutomationSpec) -> Vec<String> {
    let mut actions = Vec::new();
    for action in &spec.actions {
        let label = match action {
            Action::Rename { .. } => "rename",
            Action::Move { .. } => "move",
        };
        if !actions.iter().any(|value| value == label) {
            actions.push(label.to_string());
        }
    }
    actions
}

fn action_summary_from_events(events: &[NormalizedEvent]) -> Vec<String> {
    let mut actions = Vec::new();
    for event in events {
        let label = match event.action_type {
            ActionType::RenameFile => Some("rename"),
            ActionType::MoveFile => Some("move"),
            _ => None,
        };
        if let Some(label) = label {
            if !actions.iter().any(|value| value == label) {
                actions.push(label.to_string());
            }
        }
    }
    actions
}

fn destination_paths_from_spec(spec: &AutomationSpec) -> Vec<String> {
    let mut destinations = BTreeSet::new();
    for action in &spec.actions {
        if let Action::Move { destination } = action {
            destinations.insert(destination.clone());
        }
    }
    destinations.into_iter().collect()
}

fn destination_paths_from_report(report: &ExecutionReport) -> Vec<String> {
    let mut destinations = BTreeSet::new();
    for final_path in final_destination_paths(report) {
        if let Some(parent) = Path::new(&final_path).parent() {
            destinations.insert(parent.display().to_string());
        }
    }
    destinations.into_iter().collect()
}

fn destination_paths_from_events(events: &[NormalizedEvent]) -> Vec<String> {
    let mut move_destinations = BTreeSet::new();
    let mut other_destinations = BTreeSet::new();
    for event in events {
        if let Some(target) = event.target.as_deref() {
            match event.action_type {
                ActionType::MoveFile => {
                    if let Some(parent) = Path::new(target).parent() {
                        move_destinations.insert(parent.display().to_string());
                    }
                }
                ActionType::RenameFile => {
                    if let Some(parent) = Path::new(target).parent() {
                        other_destinations.insert(parent.display().to_string());
                    }
                }
                _ => {}
            }
        }
    }
    if move_destinations.is_empty() {
        other_destinations.into_iter().collect()
    } else {
        move_destinations.into_iter().collect()
    }
}

fn count_affected_files(report: &ExecutionReport) -> usize {
    if report.operations.is_empty() {
        return 0;
    }

    let destinations: BTreeSet<&str> = report
        .operations
        .iter()
        .map(|operation| operation.to.as_str())
        .collect();
    report
        .operations
        .iter()
        .filter(|operation| !destinations.contains(operation.from.as_str()))
        .count()
}

fn preview_examples_from_report(report: &ExecutionReport) -> Vec<PreviewExample> {
    let finals = final_destination_map(report);
    finals
        .into_iter()
        .take(2)
        .map(|(before, after)| PreviewExample {
            before: file_label(&before),
            after: file_label(&after),
        })
        .collect()
}

fn preview_examples_from_events(events: &[NormalizedEvent]) -> Vec<PreviewExample> {
    let Some(before) = events.iter().find_map(|event| {
        (event.action_type == ActionType::CreateFile)
            .then_some(event.target.as_deref())
            .flatten()
    }) else {
        return Vec::new();
    };

    let after = events
        .iter()
        .rev()
        .find_map(|event| event.target.as_deref())
        .unwrap_or(before);

    vec![PreviewExample {
        before: file_label(before),
        after: file_label(after),
    }]
}

fn final_destination_paths(report: &ExecutionReport) -> Vec<String> {
    final_destination_map(report).into_values().collect()
}

fn final_destination_map(report: &ExecutionReport) -> BTreeMap<String, String> {
    let from_to: BTreeMap<&str, &str> = report
        .operations
        .iter()
        .map(|operation| (operation.from.as_str(), operation.to.as_str()))
        .collect();
    let destinations: BTreeSet<&str> = report
        .operations
        .iter()
        .map(|operation| operation.to.as_str())
        .collect();
    let mut finals = BTreeMap::new();

    for operation in &report.operations {
        if destinations.contains(operation.from.as_str()) {
            continue;
        }

        let mut final_path = operation.to.as_str();
        while let Some(next) = from_to.get(final_path) {
            final_path = next;
        }
        finals.insert(operation.from.clone(), final_path.to_string());
    }

    finals
}

fn file_label(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.to_string())
        .unwrap_or_else(|| path.to_string())
}

fn assess_risk(
    spec: &AutomationSpec,
    estimated_affected_files: Option<usize>,
    exact_count: bool,
) -> PreviewRisk {
    let Some(safety) = spec.safety.as_ref() else {
        return PreviewRisk::High;
    };

    if !safety.dry_run_first || !safety.undo_log {
        return PreviewRisk::High;
    }

    if !exact_count {
        return PreviewRisk::Medium;
    }

    match estimated_affected_files.unwrap_or(0) {
        0..=25 => PreviewRisk::Low,
        26..=100 => PreviewRisk::Medium,
        _ => PreviewRisk::High,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;
    use tempfile::tempdir;

    fn invoice_event(
        action_type: ActionType,
        target: &str,
        from_path: Option<&str>,
    ) -> NormalizedEvent {
        let mut metadata = serde_json::Map::new();
        if let Some(from_path) = from_path {
            metadata.insert("from_path".to_string(), json!(from_path));
        }

        NormalizedEvent {
            ts: Utc.with_ymd_and_hms(2026, 3, 12, 12, 0, 0).unwrap(),
            action_type,
            app: Some("finder".to_string()),
            target: Some(target.to_string()),
            metadata: serde_json::Value::Object(metadata),
        }
    }

    fn invoice_spec(source: &Path, destination: &Path) -> AutomationSpec {
        AutomationSpec {
            id: "invoice".to_string(),
            trigger: Trigger {
                r#type: "file_created".to_string(),
                path: Some(source.display().to_string()),
                extension: Some("pdf".to_string()),
                name_contains: Some("invoice".to_string()),
            },
            actions: vec![
                Action::Rename {
                    template: "{stem}-reviewed.{ext}".to_string(),
                },
                Action::Move {
                    destination: destination.display().to_string(),
                },
            ],
            safety: Some(Safety {
                dry_run_first: true,
                undo_log: true,
            }),
        }
    }

    #[test]
    fn preview_generation_uses_exact_plan_when_files_match() {
        let dir = tempdir().unwrap();
        let inbox = dir.path().join("inbox");
        let archive = dir.path().join("archive");
        std::fs::create_dir_all(&inbox).unwrap();
        std::fs::write(inbox.join("invoice-1001.pdf"), "invoice").unwrap();
        std::fs::write(inbox.join("invoice-1002.pdf"), "invoice").unwrap();

        let preview = build_preview(&invoice_spec(&inbox, &archive), None);

        assert_eq!(preview.estimated_affected_files, Some(2));
        assert!(preview.exact_count);
        assert_eq!(preview.examples.len(), 2);
        assert_eq!(preview.examples[0].before, "invoice-1001.pdf");
        assert_eq!(preview.examples[0].after, "invoice-1001-reviewed.pdf");
        assert_eq!(
            preview.destination_paths,
            vec![archive.display().to_string()]
        );
        assert_eq!(preview.action_summary, vec!["rename", "move"]);
        assert_eq!(preview.risk, PreviewRisk::Low);
    }

    #[test]
    fn preview_generation_falls_back_to_history_when_current_context_is_missing() {
        let dir = tempdir().unwrap();
        let inbox = dir.path().join("missing-inbox");
        let archive = dir.path().join("archive");
        let preview = build_preview(
            &invoice_spec(&inbox, &archive),
            Some(&[
                invoice_event(ActionType::CreateFile, "/tmp/inbox/invoice-1001.pdf", None),
                invoice_event(
                    ActionType::RenameFile,
                    "/tmp/inbox/invoice-1001-reviewed.pdf",
                    Some("/tmp/inbox/invoice-1001.pdf"),
                ),
                invoice_event(
                    ActionType::MoveFile,
                    "/tmp/archive/invoice-1001-reviewed.pdf",
                    Some("/tmp/inbox/invoice-1001-reviewed.pdf"),
                ),
            ]),
        );

        assert_eq!(preview.estimated_affected_files, None);
        assert!(!preview.exact_count);
        assert_eq!(
            preview.examples,
            vec![PreviewExample {
                before: "invoice-1001.pdf".to_string(),
                after: "invoice-1001-reviewed.pdf".to_string(),
            }]
        );
        assert_eq!(preview.destination_paths, vec!["/tmp/archive".to_string()]);
        assert_eq!(preview.risk, PreviewRisk::Medium);
        assert!(preview.notes[0].starts_with("Best-effort preview only:"));
    }

    #[test]
    fn best_effort_preview_without_context_is_explicit() {
        let preview = best_effort_preview_without_spec(
            &[],
            vec!["Best-effort preview only: no example events available".to_string()],
        );

        assert_eq!(preview.estimated_affected_files, None);
        assert!(preview.examples.is_empty());
        assert!(preview.destination_paths.is_empty());
        assert!(preview.action_summary.is_empty());
        assert_eq!(preview.risk, PreviewRisk::High);
        assert_eq!(preview.notes.len(), 2);
    }

    #[test]
    fn preview_examples_from_report_are_stable() {
        let report = ExecutionReport {
            operations: vec![
                crate::engine::PlannedOperation {
                    action: "rename".to_string(),
                    from: "/tmp/inbox/invoice-1002.pdf".to_string(),
                    to: "/tmp/inbox/invoice-1002-reviewed.pdf".to_string(),
                },
                crate::engine::PlannedOperation {
                    action: "move".to_string(),
                    from: "/tmp/inbox/invoice-1002-reviewed.pdf".to_string(),
                    to: "/tmp/archive/invoice-1002-reviewed.pdf".to_string(),
                },
                crate::engine::PlannedOperation {
                    action: "rename".to_string(),
                    from: "/tmp/inbox/invoice-1001.pdf".to_string(),
                    to: "/tmp/inbox/invoice-1001-reviewed.pdf".to_string(),
                },
                crate::engine::PlannedOperation {
                    action: "move".to_string(),
                    from: "/tmp/inbox/invoice-1001-reviewed.pdf".to_string(),
                    to: "/tmp/archive/invoice-1001-reviewed.pdf".to_string(),
                },
            ],
        };

        assert_eq!(
            preview_examples_from_report(&report),
            vec![
                PreviewExample {
                    before: "invoice-1001.pdf".to_string(),
                    after: "invoice-1001-reviewed.pdf".to_string(),
                },
                PreviewExample {
                    before: "invoice-1002.pdf".to_string(),
                    after: "invoice-1002-reviewed.pdf".to_string(),
                },
            ]
        );
    }
}
