use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DslError {
    #[error("yaml parse error: {0}")]
    Parse(#[from] serde_yaml::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationSpec {
    pub id: String,
    pub trigger: Trigger,
    pub actions: Vec<Action>,
    pub safety: Option<Safety>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trigger {
    pub r#type: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Action {
    Rename { template: String },
    Move { destination: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Safety {
    pub dry_run_first: bool,
    pub undo_log: bool,
}

pub fn parse_spec(yaml: &str) -> Result<AutomationSpec, DslError> {
    Ok(serde_yaml::from_str(yaml)?)
}
