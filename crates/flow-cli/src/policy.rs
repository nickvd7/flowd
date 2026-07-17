use anyhow::{bail, Context, Result};
use flow_core::config::Config;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

/// Local team policy pack for machine-level safety defaults.
///
/// This is intentionally not a cloud admin surface. Teams can share a policy
/// file that clamps risky settings on import.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TeamPolicyPack {
    pub policy: TeamPolicyMeta,
    pub constraints: TeamPolicyConstraints,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TeamPolicyMeta {
    pub name: String,
    pub schema_version: u32,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TeamPolicyConstraints {
    pub force_auto_run_approved_automations: bool,
    pub force_intelligence_enabled: bool,
    pub max_suggestion_daily_cap: u32,
    #[serde(default)]
    pub observed_folders_allowlist: Vec<String>,
    pub allow_local_llm: bool,
    pub allow_browser_visit_observation: bool,
}

impl Default for TeamPolicyConstraints {
    fn default() -> Self {
        Self {
            force_auto_run_approved_automations: false,
            force_intelligence_enabled: false,
            max_suggestion_daily_cap: 8,
            observed_folders_allowlist: vec!["~/Downloads".to_string(), "~/Desktop".to_string()],
            allow_local_llm: false,
            allow_browser_visit_observation: false,
        }
    }
}

pub fn export_policy_from_config(config: &Config, name: &str) -> TeamPolicyPack {
    TeamPolicyPack {
        policy: TeamPolicyMeta {
            name: name.to_string(),
            schema_version: 1,
            description: Some(
                "Local team policy pack exported from flowd config constraints.".to_string(),
            ),
        },
        constraints: TeamPolicyConstraints {
            force_auto_run_approved_automations: config.auto_run_approved_automations,
            force_intelligence_enabled: config.intelligence_enabled,
            max_suggestion_daily_cap: if config.suggestion_daily_cap == 0 {
                8
            } else {
                config.suggestion_daily_cap
            },
            observed_folders_allowlist: config.observed_folders.clone(),
            allow_local_llm: config.local_llm_enabled,
            allow_browser_visit_observation: config.observe_browser_visits,
        },
    }
}

pub fn load_policy_pack(path: &Path) -> Result<TeamPolicyPack> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read policy pack {}", path.display()))?;
    let pack: TeamPolicyPack = toml::from_str(&raw)
        .with_context(|| format!("failed to parse policy pack {}", path.display()))?;
    if pack.policy.schema_version != 1 {
        bail!(
            "unsupported policy schema_version {} (expected 1)",
            pack.policy.schema_version
        );
    }
    Ok(pack)
}

pub fn apply_policy_to_config(config: &mut Config, pack: &TeamPolicyPack) -> Result<Vec<String>> {
    let mut notes = Vec::new();
    let constraints = &pack.constraints;

    if !constraints.force_auto_run_approved_automations && config.auto_run_approved_automations {
        config.auto_run_approved_automations = false;
        notes.push("disabled auto_run_approved_automations".to_string());
    }
    if !constraints.force_intelligence_enabled && config.intelligence_enabled {
        config.intelligence_enabled = false;
        notes.push("disabled intelligence_enabled".to_string());
    }
    if constraints.max_suggestion_daily_cap > 0
        && (config.suggestion_daily_cap == 0
            || config.suggestion_daily_cap > constraints.max_suggestion_daily_cap)
    {
        config.suggestion_daily_cap = constraints.max_suggestion_daily_cap;
        notes.push(format!(
            "clamped suggestion_daily_cap to {}",
            constraints.max_suggestion_daily_cap
        ));
    }
    if !constraints.allow_local_llm && config.local_llm_enabled {
        config.local_llm_enabled = false;
        notes.push("disabled local_llm_enabled".to_string());
    }
    if !constraints.allow_browser_visit_observation && config.observe_browser_visits {
        config.observe_browser_visits = false;
        notes.push("disabled observe_browser_visits".to_string());
    }
    if !constraints.observed_folders_allowlist.is_empty() {
        let before = config.observed_folders.len();
        config.observed_folders.retain(|folder| {
            constraints
                .observed_folders_allowlist
                .iter()
                .any(|allowed| allowed == folder)
        });
        if config.observed_folders.is_empty() {
            config.observed_folders = constraints.observed_folders_allowlist.clone();
            notes.push("replaced observed_folders with policy allowlist".to_string());
        } else if config.observed_folders.len() != before {
            notes.push("removed observed folders outside policy allowlist".to_string());
        }
    }

    config.validate().map_err(|error| anyhow::anyhow!("{error}"))?;
    Ok(notes)
}

pub fn write_policy_pack(path: &Path, pack: &TeamPolicyPack) -> Result<()> {
    let rendered = toml::to_string_pretty(pack).context("failed to serialize policy pack")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, rendered)
        .with_context(|| format!("failed to write policy pack {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_policy_clamps_risky_settings() {
        let mut config = Config {
            auto_run_approved_automations: true,
            intelligence_enabled: true,
            suggestion_daily_cap: 0,
            local_llm_enabled: true,
            observe_browser_visits: true,
            observed_folders: vec!["~/Downloads".to_string(), "~/Secrets".to_string()],
            ..Config::default()
        };
        let pack = TeamPolicyPack {
            policy: TeamPolicyMeta {
                name: "safe".to_string(),
                schema_version: 1,
                description: None,
            },
            constraints: TeamPolicyConstraints::default(),
        };
        let notes = apply_policy_to_config(&mut config, &pack).unwrap();
        assert!(!config.auto_run_approved_automations);
        assert!(!config.intelligence_enabled);
        assert_eq!(config.suggestion_daily_cap, 8);
        assert!(!config.local_llm_enabled);
        assert!(!config.observe_browser_visits);
        assert_eq!(config.observed_folders, vec!["~/Downloads".to_string()]);
        assert!(!notes.is_empty());
    }
}
