use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DslError {
    #[error("yaml parse error: {0}")]
    Parse(#[from] serde_yaml::Error),
    #[error("toml parse error: {0}")]
    Toml(#[from] toml::de::Error),
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
    pub extension: Option<String>,
    pub name_contains: Option<String>,
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

/// Pack metadata and manifest for installable workflow packs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPackAutomationRef {
    pub file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPackManifest {
    pub pack: PackMetadata,
    #[serde(default)]
    pub automation: Vec<WorkflowPackAutomationRef>,
}

pub fn parse_pack_manifest(toml_str: &str) -> Result<WorkflowPackManifest, DslError> {
    Ok(toml::from_str(toml_str)?)
}

/// Metadata for a pack registry index (local file or HTTPS).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackRegistryMeta {
    pub name: String,
    pub schema_version: u32,
}

/// One pack listed in a registry index.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackRegistryEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Directory containing `workflow-pack.toml`, relative to the registry index.
    pub path: String,
}

/// Pack registry index consumed by `flowctl packs search|install`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackRegistryIndex {
    pub registry: PackRegistryMeta,
    #[serde(default)]
    pub packs: Vec<PackRegistryEntry>,
}

pub fn parse_pack_registry_index(toml_str: &str) -> Result<PackRegistryIndex, DslError> {
    Ok(toml::from_str(toml_str)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pack_registry_index() {
        let index = parse_pack_registry_index(
            r#"
[registry]
name = "example"
schema_version = 1

[[packs]]
id = "demo.rename-downloads"
name = "Demo Rename Downloads"
version = "0.1.0"
description = "Example pack"
path = "../packs/demo-pack"
"#,
        )
        .unwrap();

        assert_eq!(index.registry.name, "example");
        assert_eq!(index.registry.schema_version, 1);
        assert_eq!(index.packs.len(), 1);
        assert_eq!(index.packs[0].id, "demo.rename-downloads");
        assert_eq!(index.packs[0].path, "../packs/demo-pack");
    }
}
