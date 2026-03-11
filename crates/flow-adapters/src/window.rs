use chrono::Utc;
use flow_core::events::{EventSource, RawEvent};
use serde_json::json;

pub fn app_switch_event(app: &str, title: &str) -> RawEvent {
    RawEvent {
        ts: Utc::now(),
        source: EventSource::ActiveWindow,
        payload: json!({
            "kind": "focus_change",
            "app": app,
            "title": title
        }),
    }
}
