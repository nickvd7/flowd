use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EventSource {
    FileWatcher,
    Clipboard,
    Terminal,
    ActiveWindow,
    Browser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawEvent {
    pub ts: DateTime<Utc>,
    pub source: EventSource,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ActionType {
    OpenApp,
    SwitchApp,
    CopyText,
    PasteText,
    RunCommand,
    CreateFile,
    RenameFile,
    MoveFile,
    VisitUrl,
    DownloadFile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedEvent {
    pub ts: DateTime<Utc>,
    pub action_type: ActionType,
    pub app: Option<String>,
    pub target: Option<String>,
    pub metadata: Value,
}
