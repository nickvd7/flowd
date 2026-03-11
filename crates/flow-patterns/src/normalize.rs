use flow_core::events::{ActionType, EventSource, NormalizedEvent, RawEvent};
use serde_json::json;
use std::path::Path;

pub fn normalize(raw: &RawEvent) -> Option<NormalizedEvent> {
    match raw.source {
        EventSource::FileWatcher => normalize_file_event(raw),
        EventSource::Terminal => Some(NormalizedEvent {
            ts: raw.ts,
            action_type: ActionType::RunCommand,
            app: Some("terminal".to_string()),
            target: raw
                .payload
                .get("command")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            metadata: raw.payload.clone(),
        }),
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use flow_adapters::file_watcher::{synthetic_file_event, FileEventKind};

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
}
