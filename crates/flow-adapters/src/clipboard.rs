use chrono::Utc;
use flow_core::events::{EventSource, RawEvent};
use serde_json::json;

pub fn metadata_only_event(len: usize) -> RawEvent {
    RawEvent {
        ts: Utc::now(),
        source: EventSource::Clipboard,
        payload: json!({
            "kind": "clipboard_change",
            "content_length": len,
            "captured": false
        }),
    }
}
