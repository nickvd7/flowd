use chrono::Utc;
use flow_core::events::{EventSource, RawEvent};
use serde_json::json;

pub fn visit_event(url: &str, title: &str) -> RawEvent {
    RawEvent {
        ts: Utc::now(),
        source: EventSource::Browser,
        payload: json!({
            "kind": "visit",
            "url": url,
            "title": title
        }),
    }
}
