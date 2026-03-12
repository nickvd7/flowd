use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

use crate::errors::FlowError;

pub const PROJECT_CONFIG_FILE_NAME: &str = "flowd.toml";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub database_path: String,
    pub observed_folders: Vec<String>,
    pub observe_clipboard: bool,
    pub observe_terminal: bool,
    pub observe_active_window: bool,
    pub redact_clipboard_content: bool,
    pub redact_command_args: bool,
    pub strip_browser_query_strings: bool,
    pub suggestion_min_usefulness_score: f64,
    pub intelligence_enabled: bool,
    pub session_inactivity_secs: i64,
    pub file_event_dedup_window_ms: i64,
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
            suggestion_min_usefulness_score: 0.0,
            intelligence_enabled: true,
            session_inactivity_secs: 300,
            file_event_dedup_window_ms: 500,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    Default,
    File(PathBuf),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadedConfig {
    pub config: Config,
    pub source: ConfigSource,
}

impl Config {
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, FlowError> {
        let raw = fs::read_to_string(path).map_err(FlowError::Io)?;
        let parsed: Self = toml::from_str(&raw).map_err(FlowError::TomlDe)?;
        parsed.validate()?;
        Ok(parsed)
    }

    pub fn load(config_path: Option<&Path>) -> Result<LoadedConfig, FlowError> {
        let current_dir = env::current_dir().map_err(FlowError::Io)?;
        let xdg_config_home = env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
        let home = home_dir();
        load_with_search_roots(
            config_path,
            &current_dir,
            xdg_config_home.as_deref(),
            home.as_deref(),
        )
    }

    pub fn validate(&self) -> Result<(), FlowError> {
        if self.database_path.trim().is_empty() {
            return Err(FlowError::Validation(
                "database_path must not be empty".to_string(),
            ));
        }

        if self.observed_folders.is_empty() {
            return Err(FlowError::Validation(
                "observed_folders must contain at least one path".to_string(),
            ));
        }

        if self
            .observed_folders
            .iter()
            .any(|path| path.trim().is_empty())
        {
            return Err(FlowError::Validation(
                "observed_folders must not contain empty paths".to_string(),
            ));
        }

        if !self.suggestion_min_usefulness_score.is_finite()
            || !(0.0..=1.0).contains(&self.suggestion_min_usefulness_score)
        {
            return Err(FlowError::Validation(
                "suggestion_min_usefulness_score must be between 0.0 and 1.0".to_string(),
            ));
        }

        if self.session_inactivity_secs <= 0 {
            return Err(FlowError::Validation(
                "session_inactivity_secs must be greater than zero".to_string(),
            ));
        }

        if self.file_event_dedup_window_ms <= 0 {
            return Err(FlowError::Validation(
                "file_event_dedup_window_ms must be greater than zero".to_string(),
            ));
        }

        Ok(())
    }

    pub fn to_pretty_toml(&self) -> Result<String, FlowError> {
        toml::to_string_pretty(self).map_err(FlowError::TomlSer)
    }
}

pub fn discover_config_path() -> Option<PathBuf> {
    let current_dir = env::current_dir().ok()?;
    let xdg_config_home = env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    let home = home_dir();
    discover_config_path_from(&current_dir, xdg_config_home.as_deref(), home.as_deref())
}

fn discover_config_path_from(
    current_dir: &Path,
    xdg_config_home: Option<&Path>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    let project_path = current_dir.join(PROJECT_CONFIG_FILE_NAME);
    if project_path.is_file() {
        return Some(project_path);
    }

    standard_config_path_from(xdg_config_home, home).filter(|path| path.is_file())
}

pub fn standard_config_path() -> Option<PathBuf> {
    let xdg_config_home = env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    let home = home_dir();
    standard_config_path_from(xdg_config_home.as_deref(), home.as_deref())
}

pub fn expand_home(raw: &str) -> PathBuf {
    if raw == "~" {
        return home_dir().unwrap_or_else(|| PathBuf::from(raw));
    }

    if let Some(stripped) = raw.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return home.join(stripped);
        }
    }

    PathBuf::from(raw)
}

pub fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

fn standard_config_path_from(
    xdg_config_home: Option<&Path>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    let config_root = xdg_config_home
        .map(Path::to_path_buf)
        .or_else(|| home.map(|value| value.join(".config")))?;
    Some(config_root.join("flowd").join("config.toml"))
}

fn load_with_search_roots(
    config_path: Option<&Path>,
    current_dir: &Path,
    xdg_config_home: Option<&Path>,
    home: Option<&Path>,
) -> Result<LoadedConfig, FlowError> {
    if let Some(path) = config_path {
        let config = Config::load_from_path(path)?;
        return Ok(LoadedConfig {
            config,
            source: ConfigSource::File(path.to_path_buf()),
        });
    }

    if let Some(path) = discover_config_path_from(current_dir, xdg_config_home, home) {
        let config = Config::load_from_path(&path)?;
        return Ok(LoadedConfig {
            config,
            source: ConfigSource::File(path),
        });
    }

    Ok(LoadedConfig {
        config: Config::default(),
        source: ConfigSource::Default,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn default_config_has_expected_values() {
        let cfg = Config::default();
        assert_eq!(cfg.database_path, "./flowd.db");
        assert!(cfg.observe_terminal);
        assert!(cfg.redact_command_args);
        assert!(cfg.intelligence_enabled);
        assert_eq!(cfg.session_inactivity_secs, 300);
        assert_eq!(cfg.file_event_dedup_window_ms, 500);
    }

    #[test]
    fn load_from_path_merges_missing_fields_with_defaults() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("flowd.toml");
        fs::write(
            &path,
            r#"
database_path = "./custom.db"
observed_folders = ["~/Inbox"]
"#,
        )
        .unwrap();

        let cfg = Config::load_from_path(&path).unwrap();

        assert_eq!(cfg.database_path, "./custom.db");
        assert_eq!(cfg.observed_folders, vec!["~/Inbox".to_string()]);
        assert!(!cfg.observe_clipboard);
        assert!(cfg.observe_terminal);
        assert_eq!(cfg.suggestion_min_usefulness_score, 0.0);
    }

    #[test]
    fn load_uses_defaults_when_no_config_exists() {
        let dir = tempdir().unwrap();
        let loaded = load_with_search_roots(None, dir.path(), None, None).unwrap();
        assert_eq!(loaded.source, ConfigSource::Default);
        assert_eq!(loaded.config, Config::default());
    }

    #[test]
    fn invalid_config_returns_validation_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("flowd.toml");
        fs::write(
            &path,
            r#"
database_path = "./flowd.db"
observed_folders = []
"#,
        )
        .unwrap();

        let error = Config::load_from_path(&path).unwrap_err();
        assert!(matches!(error, FlowError::Validation(_)));
    }

    #[test]
    fn discovers_standard_config_path_from_xdg_location() {
        let dir = tempdir().unwrap();
        let path = standard_config_path_from(Some(dir.path()), None).unwrap();
        assert_eq!(path, dir.path().join("flowd").join("config.toml"));
    }
}
