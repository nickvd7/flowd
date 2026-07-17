use anyhow::{anyhow, bail, Context, Result};
use flow_core::PathAllowlist;
use flow_dsl::{Action, AutomationSpec};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannedOperation {
    pub action: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionReport {
    pub operations: Vec<PlannedOperation>,
}

/// The execution engine always reduces an automation to a deterministic list of
/// file operations before touching the filesystem. Dry-run returns that plan as
/// preview text, and real execution applies the exact same ordered plan. Undo
/// depends on that determinism because it can only reverse operations that were
/// fully recorded at execution time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredExecutionReport {
    pub operations: Vec<PlannedOperation>,
}

impl From<ExecutionReport> for StoredExecutionReport {
    fn from(value: ExecutionReport) -> Self {
        Self {
            operations: value.operations,
        }
    }
}

impl From<&ExecutionReport> for StoredExecutionReport {
    fn from(value: &ExecutionReport) -> Self {
        Self {
            operations: value.operations.clone(),
        }
    }
}

impl From<StoredExecutionReport> for ExecutionReport {
    fn from(value: StoredExecutionReport) -> Self {
        Self {
            operations: value.operations,
        }
    }
}

pub fn dry_run(spec: &AutomationSpec, allowlist: &PathAllowlist) -> Result<Vec<String>> {
    let report = plan(spec, allowlist)?;

    if report.operations.is_empty() {
        return Ok(vec!["No matching files.".to_string()]);
    }

    Ok(report
        .operations
        .iter()
        .map(|operation| {
            format!(
                "{}: {} -> {}",
                operation.action, operation.from, operation.to
            )
        })
        .collect())
}

pub fn plan(spec: &AutomationSpec, allowlist: &PathAllowlist) -> Result<ExecutionReport> {
    let trigger_dir = spec
        .trigger
        .path
        .as_deref()
        .ok_or_else(|| anyhow!("automation trigger path is missing"))?;
    let trigger_dir = PathBuf::from(trigger_dir);
    let candidates = matching_files(spec, &trigger_dir)?;
    plan_for_candidates(spec, &candidates, allowlist)
}

/// Plan operations for a single concrete trigger file when it matches the spec.
pub fn plan_for_path(
    spec: &AutomationSpec,
    path: &Path,
    allowlist: &PathAllowlist,
) -> Result<ExecutionReport> {
    if !path.is_file() {
        return Ok(ExecutionReport {
            operations: Vec::new(),
        });
    }
    if !file_matches_trigger(spec, path) {
        return Ok(ExecutionReport {
            operations: Vec::new(),
        });
    }
    plan_for_candidates(spec, &[path.to_path_buf()], allowlist)
}

pub fn execute(spec: &AutomationSpec, allowlist: &PathAllowlist) -> Result<ExecutionReport> {
    let report = plan(spec, allowlist)?;
    apply_report(&report, allowlist)?;
    Ok(report)
}

pub fn execute_for_path(
    spec: &AutomationSpec,
    path: &Path,
    allowlist: &PathAllowlist,
) -> Result<ExecutionReport> {
    let report = plan_for_path(spec, path, allowlist)?;
    if report.operations.is_empty() {
        return Ok(report);
    }
    apply_report(&report, allowlist)?;
    Ok(report)
}

fn plan_for_candidates(
    spec: &AutomationSpec,
    candidates: &[PathBuf],
    allowlist: &PathAllowlist,
) -> Result<ExecutionReport> {
    let mut operations = Vec::new();

    for candidate in candidates {
        let mut current = candidate.clone();
        for action in &spec.actions {
            let next = match action {
                Action::Rename { template } => {
                    let file_name = render_template(&current, template)?;
                    current.with_file_name(file_name)
                }
                Action::Move { destination } => {
                    let destination_dir = PathBuf::from(destination);
                    let file_name = current
                        .file_name()
                        .ok_or_else(|| anyhow!("file name missing for {}", current.display()))?;
                    destination_dir.join(file_name)
                }
            };

            let action_name = match action {
                Action::Rename { .. } => "rename",
                Action::Move { .. } => "move",
            };
            operations.push(PlannedOperation {
                action: action_name.to_string(),
                from: current.display().to_string(),
                to: next.display().to_string(),
            });
            current = next;
        }
    }

    validate_operations(&operations, allowlist)?;
    Ok(ExecutionReport { operations })
}

/// Undo only supports reversible filesystem actions that were stored in
/// `automation_runs` when the original run completed. The inverse plan is built
/// by swapping `from` and `to` and reversing the operation order so later
/// mutations are undone before earlier ones.
pub fn plan_undo(report: &ExecutionReport, allowlist: &PathAllowlist) -> Result<ExecutionReport> {
    let mut operations = Vec::with_capacity(report.operations.len());

    for operation in report.operations.iter().rev() {
        match operation.action.as_str() {
            "rename" | "move" => operations.push(PlannedOperation {
                action: operation.action.clone(),
                from: operation.to.clone(),
                to: operation.from.clone(),
            }),
            unsupported => bail!("unsupported operation in run metadata: {unsupported}"),
        }
    }

    validate_operations_without_fs(&operations, allowlist)?;
    Ok(ExecutionReport { operations })
}

pub fn execute_report(
    report: &ExecutionReport,
    allowlist: &PathAllowlist,
) -> Result<ExecutionReport> {
    apply_report(report, allowlist)?;
    Ok(report.clone())
}

fn apply_report(report: &ExecutionReport, allowlist: &PathAllowlist) -> Result<()> {
    validate_operation_sequence(&report.operations, allowlist)?;

    for operation in &report.operations {
        let from = Path::new(&operation.from);
        let to = Path::new(&operation.to);

        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        fs::rename(from, to).with_context(|| {
            format!(
                "failed to {} {} -> {}",
                operation.action,
                from.display(),
                to.display()
            )
        })?;
    }

    Ok(())
}

fn matching_files(spec: &AutomationSpec, trigger_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for entry in fs::read_dir(trigger_dir)
        .with_context(|| format!("failed to read {}", trigger_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && file_matches_trigger(spec, &path) {
            files.push(path);
        }
    }

    files.sort();
    Ok(files)
}

pub fn file_matches_trigger(spec: &AutomationSpec, path: &Path) -> bool {
    if let Some(trigger_dir) = spec.trigger.path.as_deref() {
        let trigger_dir = PathBuf::from(trigger_dir);
        let Some(parent) = path.parent() else {
            return false;
        };
        if parent != trigger_dir.as_path() {
            return false;
        }
    }

    if let Some(extension) = spec.trigger.extension.as_deref() {
        let path_extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let expected = extension.trim_start_matches('.');
        if !path_extension.eq_ignore_ascii_case(expected) {
            return false;
        }
    }

    if let Some(fragment) = spec.trigger.name_contains.as_deref() {
        let name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if !name
            .to_ascii_lowercase()
            .contains(&fragment.to_ascii_lowercase())
        {
            return false;
        }
    }

    true
}

fn render_template(path: &Path, template: &str) -> Result<String> {
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("file name missing for {}", path.display()))?;
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("file stem missing for {}", path.display()))?;
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let original = if extension.is_empty() {
        stem.to_string()
    } else {
        format!("{stem}.{extension}")
    };

    Ok(template
        .replace("{filename}", filename)
        .replace("{original}", &original)
        .replace("{stem}", stem)
        .replace("{ext}", extension))
}

fn validate_operations(operations: &[PlannedOperation], allowlist: &PathAllowlist) -> Result<()> {
    validate_operations_without_fs(operations, allowlist)?;
    let mut seen_destinations = BTreeSet::new();

    for operation in operations {
        if !seen_destinations.insert(operation.to.clone()) {
            bail!("multiple operations target {}", operation.to);
        }

        let destination = Path::new(&operation.to);
        if destination.exists() {
            bail!("destination already exists: {}", destination.display());
        }
    }

    Ok(())
}

/// Safety guards reject unsupported actions, missing sources, and any target
/// that would overwrite an existing path. The validator simulates the full
/// ordered plan against the current filesystem state so undo can abort before
/// mutating anything if a later step would become unsafe.
fn validate_operation_sequence(
    operations: &[PlannedOperation],
    allowlist: &PathAllowlist,
) -> Result<()> {
    validate_operations_without_fs(operations, allowlist)?;

    let mut existing = BTreeSet::new();
    for operation in operations {
        existing.insert(operation.from.clone());
        existing.insert(operation.to.clone());
    }

    let mut present = BTreeSet::new();
    for path in &existing {
        if Path::new(path).exists() {
            present.insert(path.clone());
        }
    }

    for operation in operations {
        if !present.contains(&operation.from) {
            bail!("source path is missing: {}", operation.from);
        }

        if present.contains(&operation.to) {
            bail!("destination already exists: {}", operation.to);
        }

        present.remove(&operation.from);
        present.insert(operation.to.clone());
    }

    Ok(())
}

fn validate_operations_without_fs(
    operations: &[PlannedOperation],
    allowlist: &PathAllowlist,
) -> Result<()> {
    let mut seen_destinations = BTreeSet::new();

    for operation in operations {
        match operation.action.as_str() {
            "rename" | "move" => {}
            unsupported => bail!("unsupported operation: {unsupported}"),
        }

        if operation.from == operation.to {
            bail!("refusing no-op {} for {}", operation.action, operation.from);
        }

        if !seen_destinations.insert(operation.to.clone()) {
            bail!("multiple operations target {}", operation.to);
        }

        allowlist
            .ensure_allows(Path::new(&operation.from))
            .map_err(|message| anyhow!(message))?;
        allowlist
            .ensure_allows(Path::new(&operation.to))
            .map_err(|message| anyhow!(message))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flow_dsl::{Safety, Trigger};
    use tempfile::tempdir;

    fn invoice_spec(source: &Path, destination: &Path) -> AutomationSpec {
        AutomationSpec {
            id: "auto_invoice".to_string(),
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

    fn allowlist_for(dir: &Path) -> PathAllowlist {
        PathAllowlist::from_roots([dir.to_path_buf()])
    }

    #[test]
    fn dry_run_lists_predicted_actions_without_mutating_files() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("inbox");
        let destination = dir.path().join("archive");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("invoice-1003.pdf"), "invoice").unwrap();
        let spec = invoice_spec(&source, &destination);
        let allowlist = allowlist_for(dir.path());

        let preview = dry_run(&spec, &allowlist).unwrap();

        assert_eq!(preview.len(), 2);
        assert!(preview[0].contains("rename"));
        assert!(preview[1].contains("move"));
        assert!(source.join("invoice-1003.pdf").exists());
        assert!(!destination.join("invoice-1003-reviewed.pdf").exists());
    }

    #[test]
    fn execute_applies_rename_and_move() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("inbox");
        let destination = dir.path().join("archive");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("invoice-1004.pdf"), "invoice").unwrap();
        let spec = invoice_spec(&source, &destination);
        let allowlist = allowlist_for(dir.path());

        let report = execute(&spec, &allowlist).unwrap();

        assert_eq!(report.operations.len(), 2);
        assert!(!source.join("invoice-1004.pdf").exists());
        assert!(destination.join("invoice-1004-reviewed.pdf").exists());
    }

    #[test]
    fn plan_for_path_scopes_to_one_matching_file() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("inbox");
        let destination = dir.path().join("archive");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("invoice-1005.pdf"), "invoice").unwrap();
        fs::write(source.join("invoice-1006.pdf"), "invoice").unwrap();
        let spec = invoice_spec(&source, &destination);
        let target = source.join("invoice-1005.pdf");
        let allowlist = allowlist_for(dir.path());

        let report = plan_for_path(&spec, &target, &allowlist).unwrap();

        assert_eq!(report.operations.len(), 2);
        assert!(report.operations[0].from.ends_with("invoice-1005.pdf"));
        assert!(!report
            .operations
            .iter()
            .any(|operation| operation.from.ends_with("invoice-1006.pdf")));
    }

    #[test]
    fn plan_rejects_paths_outside_allowlist() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("inbox");
        let destination = dir.path().join("outside");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("invoice-1007.pdf"), "invoice").unwrap();
        let spec = invoice_spec(&source, &destination);
        let allowlist = PathAllowlist::from_roots([source.clone()]);

        let error = plan(&spec, &allowlist).unwrap_err().to_string();
        assert!(error.contains("allowlist"), "{error}");
    }

    /// Undo tests validate that inverse plans are derived from stored
    /// execution metadata, run in reverse order, and stop before mutation when
    /// the current filesystem no longer matches the recorded run state.
    #[test]
    fn plan_undo_reverses_operation_order() {
        let report = ExecutionReport {
            operations: vec![
                PlannedOperation {
                    action: "rename".to_string(),
                    from: "/tmp/inbox/invoice.pdf".to_string(),
                    to: "/tmp/inbox/invoice-reviewed.pdf".to_string(),
                },
                PlannedOperation {
                    action: "move".to_string(),
                    from: "/tmp/inbox/invoice-reviewed.pdf".to_string(),
                    to: "/tmp/archive/invoice-reviewed.pdf".to_string(),
                },
            ],
        };

        let undo = plan_undo(&report, &PathAllowlist::unrestricted()).unwrap();

        assert_eq!(
            undo.operations,
            vec![
                PlannedOperation {
                    action: "move".to_string(),
                    from: "/tmp/archive/invoice-reviewed.pdf".to_string(),
                    to: "/tmp/inbox/invoice-reviewed.pdf".to_string(),
                },
                PlannedOperation {
                    action: "rename".to_string(),
                    from: "/tmp/inbox/invoice-reviewed.pdf".to_string(),
                    to: "/tmp/inbox/invoice.pdf".to_string(),
                },
            ]
        );
    }

    #[test]
    fn execute_report_aborts_before_partial_undo_when_future_step_is_unsafe() {
        let dir = tempdir().unwrap();
        let archive = dir.path().join("archive");
        let inbox = dir.path().join("inbox");
        fs::create_dir_all(&archive).unwrap();
        fs::create_dir_all(&inbox).unwrap();
        fs::write(archive.join("invoice-reviewed.pdf"), "invoice").unwrap();
        fs::write(inbox.join("invoice.pdf"), "collision").unwrap();
        let allowlist = allowlist_for(dir.path());

        let undo = ExecutionReport {
            operations: vec![
                PlannedOperation {
                    action: "move".to_string(),
                    from: archive.join("invoice-reviewed.pdf").display().to_string(),
                    to: inbox.join("invoice-reviewed.pdf").display().to_string(),
                },
                PlannedOperation {
                    action: "rename".to_string(),
                    from: inbox.join("invoice-reviewed.pdf").display().to_string(),
                    to: inbox.join("invoice.pdf").display().to_string(),
                },
            ],
        };

        let error = execute_report(&undo, &allowlist).unwrap_err().to_string();

        assert!(error.contains("destination already exists"));
        assert!(archive.join("invoice-reviewed.pdf").exists());
        assert!(!inbox.join("invoice-reviewed.pdf").exists());
    }

    #[test]
    fn plan_undo_rejects_unsupported_operations() {
        let report = ExecutionReport {
            operations: vec![PlannedOperation {
                action: "copy".to_string(),
                from: "/tmp/a".to_string(),
                to: "/tmp/b".to_string(),
            }],
        };

        let error = plan_undo(&report, &PathAllowlist::unrestricted())
            .unwrap_err()
            .to_string();

        assert!(error.contains("unsupported operation"));
    }
}
