use chrono::Utc;
use flow_core::events::{EventSource, RawEvent};
use serde_json::json;
use std::path::Path;

pub fn synthetic_create_event(path: &Path) -> RawEvent {
    RawEvent {
        ts: Utc::now(),
        source: EventSource::FileWatcher,
        payload: json!({
            "kind": "create",
            "path": path.display().to_string()
        }),
    }
}
