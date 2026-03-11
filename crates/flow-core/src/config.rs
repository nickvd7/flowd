use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

use crate::errors::FlowError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    pub database_path: String,
    #[serde(default = "default_watched_directories", alias = "observed_folders")]
    pub watched_directories: Vec<String>,
    pub observe_clipboard: bool,
    pub observe_terminal: bool,
    pub observe_active_window: bool,
    pub redact_clipboard_content: bool,
    pub redact_command_args: bool,
    pub strip_browser_query_strings: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database_path: "./flowd.db".to_string(),
            watched_directories: default_watched_directories(),
            observe_clipboard: false,
            observe_terminal: true,
            observe_active_window: false,
            redact_clipboard_content: true,
            redact_command_args: true,
            strip_browser_query_strings: true,
        }
    }
}

impl Config {
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, FlowError> {
        let raw = fs::read_to_string(path).map_err(FlowError::Io)?;
        let parsed = toml::from_str(&raw).map_err(FlowError::TomlDe)?;
        Ok(parsed)
    }

    pub fn expanded_watched_directories(&self) -> Vec<PathBuf> {
        self.watched_directories
            .iter()
            .map(|path| expand_home(path))
            .collect()
    }
}

fn default_watched_directories() -> Vec<String> {
    vec![
        "~/Downloads".to_string(),
        "~/Desktop".to_string(),
        "~/Documents".to_string(),
    ]
}

fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        return env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(path));
    }

    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }

    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let cfg = Config::default();
        assert_eq!(cfg.database_path, "./flowd.db");
        assert_eq!(cfg.watched_directories, default_watched_directories());
        assert!(cfg.observe_terminal);
        assert!(cfg.redact_command_args);
    }

    #[test]
    fn expands_tilde_prefixed_watched_directories() {
        let cfg = Config {
            watched_directories: vec!["~/Downloads".to_string(), "/tmp".to_string()],
            ..Config::default()
        };

        let expanded = cfg.expanded_watched_directories();

        assert!(expanded[0].is_absolute());
        assert_eq!(expanded[1], PathBuf::from("/tmp"));
    }
}
