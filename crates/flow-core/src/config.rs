use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

use crate::errors::FlowError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    pub database_path: String,
    pub observed_folders: Vec<String>,
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
            observed_folders: vec!["~/Downloads".to_string()],
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let cfg = Config::default();
        assert_eq!(cfg.database_path, "./flowd.db");
        assert!(cfg.observe_terminal);
        assert!(cfg.redact_command_args);
    }
}
