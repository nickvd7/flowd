use flow_core::events::{ActionType, EventSource, NormalizedEvent, RawEvent};
use serde_json::json;

pub fn normalize(raw: &RawEvent) -> Option<NormalizedEvent> {
    match raw.source {
        EventSource::FileWatcher => Some(NormalizedEvent {
            ts: raw.ts,
            action_type: ActionType::CreateFile,
            app: None,
            target: raw.payload.get("path").and_then(|v| v.as_str()).map(|s| s.to_string()),
            metadata: raw.payload.clone(),
        }),
        EventSource::Terminal => Some(NormalizedEvent {
            ts: raw.ts,
            action_type: ActionType::RunCommand,
            app: Some("terminal".to_string()),
            target: raw.payload.get("command").and_then(|v| v.as_str()).map(|s| s.to_string()),
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
            target: raw.payload.get("url").and_then(|v| v.as_str()).map(|s| s.to_string()),
            metadata: raw.payload.clone(),
        }),
        EventSource::ActiveWindow => Some(NormalizedEvent {
            ts: raw.ts,
            action_type: ActionType::SwitchApp,
            app: raw.payload.get("app").and_then(|v| v.as_str()).map(|s| s.to_string()),
            target: raw.payload.get("title").and_then(|v| v.as_str()).map(|s| s.to_string()),
            metadata: json!({ "source": "active_window" }),
        }),
    }
}
