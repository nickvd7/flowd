use anyhow::{anyhow, bail, Context, Result};
use flow_dsl::{Action, AutomationSpec};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlannedOperation {
    pub action: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExecutionReport {
    pub operations: Vec<PlannedOperation>,
}

pub fn dry_run(spec: &AutomationSpec) -> Result<Vec<String>> {
    let report = plan(spec)?;

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

pub fn plan(spec: &AutomationSpec) -> Result<ExecutionReport> {
    let trigger_dir = spec
        .trigger
        .path
        .as_deref()
        .ok_or_else(|| anyhow!("automation trigger path is missing"))?;
    let trigger_dir = PathBuf::from(trigger_dir);

    let candidates = matching_files(spec, &trigger_dir)?;
    let mut operations = Vec::new();

    for candidate in candidates {
        let mut current = candidate;
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

    validate_operations(&operations)?;
    Ok(ExecutionReport { operations })
}

pub fn execute(spec: &AutomationSpec) -> Result<ExecutionReport> {
    let report = plan(spec)?;

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

    Ok(report)
}

fn matching_files(spec: &AutomationSpec, trigger_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for entry in fs::read_dir(trigger_dir)
        .with_context(|| format!("failed to read {}", trigger_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        if let Some(extension) = spec.trigger.extension.as_deref() {
            let path_extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if !path_extension.eq_ignore_ascii_case(extension) {
                continue;
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
                continue;
            }
        }

        files.push(path);
    }

    files.sort();
    Ok(files)
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

fn validate_operations(operations: &[PlannedOperation]) -> Result<()> {
    let mut seen_destinations = std::collections::BTreeSet::new();

    for operation in operations {
        if operation.from == operation.to {
            bail!("refusing no-op {} for {}", operation.action, operation.from);
        }

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

    #[test]
    fn dry_run_lists_predicted_actions_without_mutating_files() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("inbox");
        let destination = dir.path().join("archive");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("invoice-1003.pdf"), "invoice").unwrap();
        let spec = invoice_spec(&source, &destination);

        let preview = dry_run(&spec).unwrap();

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

        let report = execute(&spec).unwrap();

        assert_eq!(report.operations.len(), 2);
        assert!(!source.join("invoice-1004.pdf").exists());
        assert!(destination.join("invoice-1004-reviewed.pdf").exists());
    }
}
