use chrono::{DateTime, Utc};
use flow_core::events::{EventSource, RawEvent};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileEventKind {
    Create,
    Rename,
    Move,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEvent {
    pub ts: DateTime<Utc>,
    pub kind: FileEventKind,
    pub path: String,
    #[serde(default)]
    pub from_path: Option<String>,
}

impl FileEvent {
    pub fn into_raw_event(self) -> RawEvent {
        RawEvent {
            ts: self.ts,
            source: EventSource::FileWatcher,
            payload: json!({
                "kind": self.kind,
                "path": self.path,
                "from_path": self.from_path,
            }),
        }
    }
}

pub fn synthetic_create_event(path: &Path) -> RawEvent {
    FileEvent {
        ts: Utc::now(),
        kind: FileEventKind::Create,
        path: path.display().to_string(),
        from_path: None,
    }
    .into_raw_event()
}

pub fn synthetic_file_event(
    ts: DateTime<Utc>,
    kind: FileEventKind,
    path: impl Into<String>,
    from_path: Option<String>,
) -> RawEvent {
    FileEvent {
        ts,
        kind,
        path: path.into(),
        from_path,
    }
    .into_raw_event()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn file_event_serializes_into_raw_event_payload() {
        let raw = synthetic_file_event(
            Utc.with_ymd_and_hms(2026, 1, 15, 9, 0, 0).unwrap(),
            FileEventKind::Rename,
            "/tmp/invoice-1001-reviewed.pdf",
            Some("/tmp/invoice-1001.pdf".to_string()),
        );

        assert_eq!(raw.source, EventSource::FileWatcher);
        assert_eq!(raw.payload["kind"], "rename");
        assert_eq!(raw.payload["path"], "/tmp/invoice-1001-reviewed.pdf");
        assert_eq!(raw.payload["from_path"], "/tmp/invoice-1001.pdf");
    }
}
