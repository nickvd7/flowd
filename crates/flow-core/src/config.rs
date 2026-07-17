use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

use crate::errors::FlowError;

pub const PROJECT_CONFIG_FILE_NAME: &str = "flowd.toml";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardCaptureMode {
    MetadataOnly,
    Redacted,
    Content,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardPrivacyConfig {
    pub mode: ClipboardCaptureMode,
    pub max_capture_bytes: usize,
}

impl Default for ClipboardPrivacyConfig {
    fn default() -> Self {
        Self {
            mode: ClipboardCaptureMode::MetadataOnly,
            max_capture_bytes: 256,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardObservationConfig {
    pub privacy: ClipboardPrivacyConfig,
    pub poll_interval_ms: u64,
}

impl Default for ClipboardObservationConfig {
    fn default() -> Self {
        Self {
            privacy: ClipboardPrivacyConfig::default(),
            poll_interval_ms: 1000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub database_path: String,
    pub observed_folders: Vec<String>,
    pub observe_clipboard: bool,
    pub observe_browser_downloads: bool,
    pub browser_downloads_bridge_path: String,
    pub observe_terminal: bool,
    pub terminal_history_bridge_path: String,
    pub observe_active_window: bool,
    pub auto_run_approved_automations: bool,
    pub redact_clipboard_content: bool,
    pub clipboard_store_redacted_preview: bool,
    pub clipboard_max_capture_bytes: usize,
    pub clipboard_poll_interval_ms: u64,
    pub redact_command_args: bool,
    pub strip_browser_query_strings: bool,
    pub suggestion_min_usefulness_score: f64,
    /// Maximum suggestions to show per UTC day. `0` means unlimited.
    pub suggestion_daily_cap: u32,
    pub intelligence_enabled: bool,
    pub intelligence_rejected_cooldown_secs: i64,
    pub intelligence_snoozed_cooldown_secs: i64,
    pub intelligence_shown_cooldown_secs: i64,
    pub intelligence_minimum_score_for_show: f64,
    pub session_inactivity_secs: i64,
    pub file_event_dedup_window_ms: i64,
    pub auto_run_debounce_ms: u64,
    pub auto_run_on_browser_downloads: bool,
    pub auto_run_trigger_file_only: bool,
    /// Extra roots allowed for automation from/to paths. Empty means use
    /// `observed_folders` as the execution allowlist.
    pub execution_allowed_roots: Vec<String>,
    /// When true, dry-run/run/undo refuse paths outside the execution allowlist.
    pub enforce_execution_path_allowlist: bool,
    pub observe_browser_visits: bool,
    pub browser_visits_bridge_path: String,
    /// Opt-in local LLM labeling (metadata only; never executes actions).
    pub local_llm_enabled: bool,
    pub local_llm_endpoint: String,
    pub local_llm_model: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database_path: "./flowd.db".to_string(),
            observed_folders: vec!["~/Downloads".to_string()],
            observe_clipboard: false,
            observe_browser_downloads: false,
            browser_downloads_bridge_path: "~/.flowd/browser-downloads.ndjson".to_string(),
            observe_terminal: false,
            terminal_history_bridge_path: "~/.flowd/terminal-history.ndjson".to_string(),
            observe_active_window: false,
            auto_run_approved_automations: false,
            redact_clipboard_content: true,
            clipboard_store_redacted_preview: false,
            clipboard_max_capture_bytes: 256,
            clipboard_poll_interval_ms: 1000,
            redact_command_args: true,
            strip_browser_query_strings: true,
            suggestion_min_usefulness_score: 0.0,
            suggestion_daily_cap: 0,
            intelligence_enabled: false,
            intelligence_rejected_cooldown_secs: 14_400,
            intelligence_snoozed_cooldown_secs: 7_200,
            intelligence_shown_cooldown_secs: 7_200,
            intelligence_minimum_score_for_show: 12.0,
            session_inactivity_secs: 300,
            file_event_dedup_window_ms: 500,
            auto_run_debounce_ms: 1_500,
            auto_run_on_browser_downloads: false,
            auto_run_trigger_file_only: true,
            execution_allowed_roots: Vec::new(),
            enforce_execution_path_allowlist: true,
            observe_browser_visits: false,
            browser_visits_bridge_path: "~/.flowd/browser-visits.ndjson".to_string(),
            local_llm_enabled: false,
            local_llm_endpoint: "http://127.0.0.1:11434".to_string(),
            local_llm_model: "llama3.2".to_string(),
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

        if self.observe_browser_downloads && self.browser_downloads_bridge_path.trim().is_empty() {
            return Err(FlowError::Validation(
                "browser_downloads_bridge_path must not be empty when browser download observation is enabled"
                    .to_string(),
            ));
        }

        if self.observe_terminal && self.terminal_history_bridge_path.trim().is_empty() {
            return Err(FlowError::Validation(
                "terminal_history_bridge_path must not be empty when terminal observation is enabled"
                    .to_string(),
            ));
        }

        if self.observe_browser_visits && self.browser_visits_bridge_path.trim().is_empty() {
            return Err(FlowError::Validation(
                "browser_visits_bridge_path must not be empty when browser visit observation is enabled"
                    .to_string(),
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

        if !self.intelligence_minimum_score_for_show.is_finite()
            || self.intelligence_minimum_score_for_show < 0.0
        {
            return Err(FlowError::Validation(
                "intelligence_minimum_score_for_show must be a finite non-negative number"
                    .to_string(),
            ));
        }

        for (name, value) in [
            (
                "intelligence_rejected_cooldown_secs",
                self.intelligence_rejected_cooldown_secs,
            ),
            (
                "intelligence_snoozed_cooldown_secs",
                self.intelligence_snoozed_cooldown_secs,
            ),
            (
                "intelligence_shown_cooldown_secs",
                self.intelligence_shown_cooldown_secs,
            ),
        ] {
            if value < 0 {
                return Err(FlowError::Validation(format!(
                    "{name} must be greater than or equal to zero"
                )));
            }
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

        if self.clipboard_max_capture_bytes == 0 {
            return Err(FlowError::Validation(
                "clipboard_max_capture_bytes must be greater than zero".to_string(),
            ));
        }

        if self.clipboard_poll_interval_ms == 0 {
            return Err(FlowError::Validation(
                "clipboard_poll_interval_ms must be greater than zero".to_string(),
            ));
        }

        if self.local_llm_enabled && self.local_llm_endpoint.trim().is_empty() {
            return Err(FlowError::Validation(
                "local_llm_endpoint must not be empty when local_llm_enabled is true".to_string(),
            ));
        }

        if self.local_llm_enabled {
            let endpoint = self.local_llm_endpoint.trim();
            if !(endpoint.starts_with("http://127.0.0.1")
                || endpoint.starts_with("http://localhost")
                || endpoint.starts_with("https://127.0.0.1")
                || endpoint.starts_with("https://localhost"))
            {
                return Err(FlowError::Validation(
                    "local_llm_endpoint must point at localhost/127.0.0.1 for privacy-safe labeling"
                        .to_string(),
                ));
            }
        }

        Ok(())
    }

    pub fn to_pretty_toml(&self) -> Result<String, FlowError> {
        toml::to_string_pretty(self).map_err(FlowError::TomlSer)
    }

    pub fn clipboard_observation_config(&self) -> ClipboardObservationConfig {
        ClipboardObservationConfig {
            privacy: ClipboardPrivacyConfig {
                mode: self.clipboard_capture_mode(),
                max_capture_bytes: self.clipboard_max_capture_bytes,
            },
            poll_interval_ms: self.clipboard_poll_interval_ms,
        }
    }

    pub fn clipboard_capture_mode(&self) -> ClipboardCaptureMode {
        if !self.redact_clipboard_content {
            ClipboardCaptureMode::Content
        } else if self.clipboard_store_redacted_preview {
            ClipboardCaptureMode::Redacted
        } else {
            ClipboardCaptureMode::MetadataOnly
        }
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

pub fn preferred_setup_config_path() -> Result<PathBuf, FlowError> {
    let current_dir = env::current_dir().map_err(FlowError::Io)?;
    let xdg_config_home = env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    let home = home_dir();
    Ok(preferred_setup_config_path_from(
        &current_dir,
        xdg_config_home.as_deref(),
        home.as_deref(),
    ))
}

fn preferred_setup_config_path_from(
    current_dir: &Path,
    xdg_config_home: Option<&Path>,
    home: Option<&Path>,
) -> PathBuf {
    if let Some(path) = standard_config_path_from(xdg_config_home, home) {
        return path;
    }

    current_dir.join(PROJECT_CONFIG_FILE_NAME)
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
        assert!(!cfg.observe_terminal);
        assert!(cfg.redact_command_args);
        assert!(!cfg.intelligence_enabled);
        assert!(!cfg.auto_run_approved_automations);
        assert_eq!(cfg.session_inactivity_secs, 300);
        assert_eq!(cfg.file_event_dedup_window_ms, 500);
        assert!(!cfg.observe_browser_downloads);
        assert_eq!(
            cfg.browser_downloads_bridge_path,
            "~/.flowd/browser-downloads.ndjson"
        );
        assert_eq!(
            cfg.terminal_history_bridge_path,
            "~/.flowd/terminal-history.ndjson"
        );
        assert_eq!(
            cfg.clipboard_capture_mode(),
            ClipboardCaptureMode::MetadataOnly
        );
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
        assert!(!cfg.observe_browser_downloads);
        assert!(!cfg.observe_terminal);
        assert!(!cfg.intelligence_enabled);
        assert_eq!(cfg.suggestion_min_usefulness_score, 0.0);
    }

    #[test]
    fn derives_clipboard_capture_modes_from_legacy_flags() {
        let metadata_only = Config::default();
        assert_eq!(
            metadata_only.clipboard_capture_mode(),
            ClipboardCaptureMode::MetadataOnly
        );

        let redacted = Config {
            clipboard_store_redacted_preview: true,
            ..Config::default()
        };
        assert_eq!(
            redacted.clipboard_capture_mode(),
            ClipboardCaptureMode::Redacted
        );

        let content = Config {
            redact_clipboard_content: false,
            ..Config::default()
        };
        assert_eq!(
            content.clipboard_capture_mode(),
            ClipboardCaptureMode::Content
        );
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
    fn browser_download_observation_requires_bridge_path() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("flowd.toml");
        fs::write(
            &path,
            r#"
database_path = "./flowd.db"
observed_folders = ["~/Downloads"]
observe_browser_downloads = true
browser_downloads_bridge_path = "   "
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

    #[test]
    fn preferred_setup_path_falls_back_to_current_directory() {
        let dir = tempdir().unwrap();
        let path = preferred_setup_config_path_from(dir.path(), None, None);
        assert_eq!(path, dir.path().join(PROJECT_CONFIG_FILE_NAME));
    }
}
