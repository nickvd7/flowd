use chrono::Utc;
use flow_core::events::{EventSource, RawEvent};
use serde_json::json;

pub fn command_event(command: &str, cwd: &str) -> RawEvent {
    RawEvent {
        ts: Utc::now(),
        source: EventSource::Terminal,
        payload: json!({
            "kind": "command",
            "command": command,
            "cwd": cwd
        }),
    }
}
