use flow_core::events::{ActionType, EventSource, NormalizedEvent, RawEvent};
use serde_json::json;
use std::path::Path;

pub fn normalize(raw: &RawEvent) -> Option<NormalizedEvent> {
    match raw.source {
        EventSource::FileWatcher => normalize_file_event(raw),
        EventSource::Terminal => normalize_terminal_event(raw),
        EventSource::Clipboard => Some(NormalizedEvent {
            ts: raw.ts,
            action_type: ActionType::CopyText,
            app: None,
            target: None,
            metadata: raw.payload.clone(),
        }),
        EventSource::Browser => Some(NormalizedEvent {
            ts: raw.ts,
            action_type: ActionType::VisitUrl,
            app: Some("browser".to_string()),
            target: raw
                .payload
                .get("url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            metadata: raw.payload.clone(),
        }),
        EventSource::ActiveWindow => Some(NormalizedEvent {
            ts: raw.ts,
            action_type: ActionType::SwitchApp,
            app: raw
                .payload
                .get("app")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            target: raw
                .payload
                .get("title")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            metadata: json!({ "source": "active_window" }),
        }),
    }
}

fn normalize_file_event(raw: &RawEvent) -> Option<NormalizedEvent> {
    let path = raw.payload.get("path").and_then(|value| value.as_str())?;
    let action_type = match raw.payload.get("kind").and_then(|value| value.as_str())? {
        "create" => ActionType::CreateFile,
        "rename" => ActionType::RenameFile,
        "move" => ActionType::MoveFile,
        _ => return None,
    };

    Some(NormalizedEvent {
        ts: raw.ts,
        action_type,
        app: None,
        target: Some(path.to_string()),
        metadata: json!({
            "kind": raw.payload.get("kind").cloned().unwrap_or_default(),
            "path": path,
            "from_path": raw.payload.get("from_path").cloned().unwrap_or_default(),
            "extension": file_extension(path),
            "file_group": file_group(path),
        }),
    })
}

fn normalize_terminal_event(raw: &RawEvent) -> Option<NormalizedEvent> {
    let kind = raw.payload.get("kind").and_then(|value| value.as_str())?;
    let succeeded = raw
        .payload
        .get("succeeded")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let path = raw
        .payload
        .get("path")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let from_path = raw
        .payload
        .get("from_path")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let command_name = raw
        .payload
        .get("command_name")
        .and_then(|value| value.as_str())
        .unwrap_or("command")
        .to_string();

    let action_type = if succeeded {
        match kind {
            "copy" => ActionType::CreateFile,
            "rename" => ActionType::RenameFile,
            "move" => ActionType::MoveFile,
            _ => ActionType::RunCommand,
        }
    } else {
        ActionType::RunCommand
    };

    let target = match action_type {
        ActionType::CreateFile | ActionType::RenameFile | ActionType::MoveFile => path.clone(),
        _ => path.clone().or_else(|| Some(command_name.clone())),
    };

    let group_source = path
        .as_deref()
        .or_else(|| raw.payload.get("cwd").and_then(|value| value.as_str()))
        .unwrap_or("command");
    let metadata = json!({
        "kind": kind,
        "path": path,
        "from_path": from_path,
        "paths": raw.payload.get("paths").cloned().unwrap_or_default(),
        "path_count": raw.payload.get("path_count").cloned().unwrap_or_default(),
        "command_name": command_name,
        "cwd": raw.payload.get("cwd").cloned().unwrap_or_default(),
        "shell": raw.payload.get("shell").cloned().unwrap_or_default(),
        "exit_code": raw.payload.get("exit_code").cloned().unwrap_or_default(),
        "redacted_command": raw.payload.get("redacted_command").cloned().unwrap_or_default(),
        "succeeded": succeeded,
        "destructive": raw.payload.get("destructive").cloned().unwrap_or_default(),
        "source": "terminal",
        "extension": path
            .as_deref()
            .map(file_extension)
            .unwrap_or_else(|| "unknown".to_string()),
        "file_group": path
            .as_deref()
            .map(file_group)
            .unwrap_or_else(|| command_group(group_source, &command_name)),
    });

    Some(NormalizedEvent {
        ts: raw.ts,
        action_type,
        app: Some("terminal".to_string()),
        target,
        metadata,
    })
}

fn file_extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_else(|| "unknown".to_string())
}

fn file_group(path: &str) -> String {
    let stem = Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    let tokens: Vec<&str> = stem
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .filter(|token| token.chars().any(|ch| ch.is_ascii_alphabetic()))
        .collect();

    if tokens.is_empty() {
        "file".to_string()
    } else {
        tokens.join("_")
    }
}

fn command_group(path_or_cwd: &str, command_name: &str) -> String {
    let group = file_group(path_or_cwd);
    if group == "file" {
        command_name.to_string()
    } else {
        group
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use flow_adapters::file_watcher::{synthetic_file_event, FileEventKind};
    use flow_adapters::terminal::synthetic_terminal_history_event;

    #[test]
    fn normalizes_file_events_with_group_metadata() {
        let raw = synthetic_file_event(
            Utc.with_ymd_and_hms(2026, 1, 15, 9, 0, 0).unwrap(),
            FileEventKind::Move,
            "/tmp/archive/invoice-1001.pdf",
            Some("/tmp/inbox/invoice-1001.pdf".to_string()),
        );

        let event = normalize(&raw).unwrap();

        assert_eq!(event.action_type, ActionType::MoveFile);
        assert_eq!(
            event.target.as_deref(),
            Some("/tmp/archive/invoice-1001.pdf")
        );
        assert_eq!(event.metadata["extension"], "pdf");
        assert_eq!(event.metadata["file_group"], "invoice");
        assert_eq!(event.metadata["from_path"], "/tmp/inbox/invoice-1001.pdf");
    }

    #[test]
    fn normalizes_terminal_move_into_file_workflow_event() {
        let raw = synthetic_terminal_history_event(
            Utc.with_ymd_and_hms(2026, 3, 11, 9, 0, 0).unwrap(),
            "/tmp/workspace",
            "mv inbox/report.txt archive/report.txt",
            Some(0),
        );

        let event = normalize(&raw).unwrap();

        assert_eq!(event.action_type, ActionType::MoveFile);
        assert_eq!(
            event.target.as_deref(),
            Some("/tmp/workspace/archive/report.txt")
        );
        assert_eq!(event.app.as_deref(), Some("terminal"));
        assert_eq!(event.metadata["source"], "terminal");
        assert_eq!(event.metadata["command_name"], "mv");
        assert_eq!(event.metadata["file_group"], "report");
    }

    #[test]
    fn keeps_destructive_terminal_commands_as_run_command_events() {
        let raw = synthetic_terminal_history_event(
            Utc.with_ymd_and_hms(2026, 3, 11, 9, 5, 0).unwrap(),
            "/tmp/workspace",
            "rm -rf secrets.txt",
            Some(0),
        );

        let event = normalize(&raw).unwrap();

        assert_eq!(event.action_type, ActionType::RunCommand);
        assert_eq!(event.target.as_deref(), Some("/tmp/workspace/secrets.txt"));
        assert_eq!(event.metadata["destructive"], true);
        assert_eq!(event.metadata["command_name"], "rm");
        assert_eq!(event.metadata["file_group"], "secrets");
    }
}
