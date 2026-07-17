//! Local entitlement skeleton for Private Intelligence.
//!
//! This is intentionally unsigned for now: a TOML license file under the user
//! config directory gates the product layer. Sibling development and local
//! evaluation stay unblocked via `FLOWD_INTELLIGENCE_DEV=1`.

use crate::config::home_dir;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

/// Product tiers advertised on flowd.net and stored in license files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IntelligenceTier {
    /// Local evaluation / early access.
    Eval,
    /// Individual Private Intelligence.
    Pro,
    /// Shared machines / policy packs / commercial support lane.
    Team,
    /// Explicit developer override recorded in a license file.
    Dev,
}

impl IntelligenceTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Eval => "eval",
            Self::Pro => "pro",
            Self::Team => "team",
            Self::Dev => "dev",
        }
    }
}

/// Resolved entitlement used to gate the Private Intelligence client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntelligenceEntitlement {
    /// `FLOWD_INTELLIGENCE_DEV=1` — unrestricted local development.
    DevMode,
    /// Valid license file for a product tier.
    Valid {
        tier: IntelligenceTier,
        issued_to: Option<String>,
        expires_at: Option<String>,
        path: PathBuf,
    },
    /// License present but past `expires_at`.
    Expired {
        tier: IntelligenceTier,
        expired_at: String,
        path: PathBuf,
    },
    /// No license file and no dev override.
    Missing { expected_path: PathBuf },
    /// License file could not be parsed or validated.
    Invalid { path: PathBuf, reason: String },
}

impl IntelligenceEntitlement {
    pub fn allows_intelligence(&self) -> bool {
        matches!(self, Self::DevMode | Self::Valid { .. })
    }

    pub fn summary(&self) -> String {
        match self {
            Self::DevMode => "dev mode (FLOWD_INTELLIGENCE_DEV=1)".to_string(),
            Self::Valid {
                tier,
                issued_to,
                expires_at,
                ..
            } => {
                let mut parts = vec![format!("licensed ({})", tier.as_str())];
                if let Some(who) = issued_to {
                    parts.push(format!("to {who}"));
                }
                if let Some(when) = expires_at {
                    parts.push(format!("expires {when}"));
                }
                parts.join(", ")
            }
            Self::Expired {
                tier, expired_at, ..
            } => format!("expired {} license ({expired_at})", tier.as_str()),
            Self::Missing { expected_path } => {
                format!("missing license ({})", expected_path.display())
            }
            Self::Invalid { path, reason } => {
                format!("invalid license {} ({reason})", path.display())
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct LicenseFile {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    tier: IntelligenceTier,
    #[serde(default)]
    issued_to: Option<String>,
    #[serde(default)]
    expires_at: Option<String>,
    #[serde(default)]
    token: Option<String>,
}

fn default_schema_version() -> u32 {
    1
}

/// Default path: `$XDG_CONFIG_HOME/flowd/intelligence.license.toml` or
/// `~/.config/flowd/intelligence.license.toml`.
pub fn default_intelligence_license_path() -> PathBuf {
    if let Some(explicit) = env::var_os("FLOWD_INTELLIGENCE_LICENSE") {
        return PathBuf::from(explicit);
    }
    let root = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"));
    root.join("flowd").join("intelligence.license.toml")
}

/// Resolve entitlement for Private Intelligence gating.
pub fn resolve_intelligence_entitlement() -> IntelligenceEntitlement {
    resolve_intelligence_entitlement_at(&default_intelligence_license_path(), Utc::now())
}

pub fn resolve_intelligence_entitlement_at(
    path: &Path,
    now: DateTime<Utc>,
) -> IntelligenceEntitlement {
    if env_flag_enabled("FLOWD_INTELLIGENCE_DEV") {
        return IntelligenceEntitlement::DevMode;
    }

    if !path.is_file() {
        return IntelligenceEntitlement::Missing {
            expected_path: path.to_path_buf(),
        };
    }

    let raw = match fs::read_to_string(path) {
        Ok(value) => value,
        Err(error) => {
            return IntelligenceEntitlement::Invalid {
                path: path.to_path_buf(),
                reason: error.to_string(),
            };
        }
    };

    let license: LicenseFile = match toml::from_str(&raw) {
        Ok(value) => value,
        Err(error) => {
            return IntelligenceEntitlement::Invalid {
                path: path.to_path_buf(),
                reason: error.to_string(),
            };
        }
    };

    if license.schema_version == 0 {
        return IntelligenceEntitlement::Invalid {
            path: path.to_path_buf(),
            reason: "schema_version must be >= 1".to_string(),
        };
    }

    // Skeleton accepts any non-empty token when present; cryptographic verify
    // can replace this later without changing the file shape.
    if let Some(token) = &license.token {
        if token.trim().is_empty() {
            return IntelligenceEntitlement::Invalid {
                path: path.to_path_buf(),
                reason: "token is empty".to_string(),
            };
        }
    }

    if let Some(expires_at) = &license.expires_at {
        match DateTime::parse_from_rfc3339(expires_at) {
            Ok(parsed) => {
                if parsed.with_timezone(&Utc) < now {
                    return IntelligenceEntitlement::Expired {
                        tier: license.tier,
                        expired_at: expires_at.clone(),
                        path: path.to_path_buf(),
                    };
                }
            }
            Err(error) => {
                return IntelligenceEntitlement::Invalid {
                    path: path.to_path_buf(),
                    reason: format!("expires_at: {error}"),
                };
            }
        }
    }

    IntelligenceEntitlement::Valid {
        tier: license.tier,
        issued_to: license.issued_to,
        expires_at: license.expires_at,
        path: path.to_path_buf(),
    }
}

fn env_flag_enabled(name: &str) -> bool {
    match env::var(name) {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::tempdir;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn missing_license_blocks_without_dev_mode() {
        let _guard = ENV_LOCK.lock().unwrap();
        env::remove_var("FLOWD_INTELLIGENCE_DEV");
        let dir = tempdir().unwrap();
        let path = dir.path().join("missing.toml");
        let status = resolve_intelligence_entitlement_at(&path, Utc::now());
        assert!(!status.allows_intelligence());
        assert!(matches!(status, IntelligenceEntitlement::Missing { .. }));
    }

    #[test]
    fn valid_pro_license_allows_intelligence() {
        let _guard = ENV_LOCK.lock().unwrap();
        env::remove_var("FLOWD_INTELLIGENCE_DEV");
        let dir = tempdir().unwrap();
        let path = dir.path().join("license.toml");
        fs::write(
            &path,
            r#"
schema_version = 1
tier = "pro"
issued_to = "ada@example.com"
expires_at = "2099-01-01T00:00:00Z"
token = "local-unsigned-v1:demo"
"#,
        )
        .unwrap();
        let status = resolve_intelligence_entitlement_at(&path, Utc::now());
        assert!(status.allows_intelligence());
        match status {
            IntelligenceEntitlement::Valid { tier, issued_to, .. } => {
                assert_eq!(tier, IntelligenceTier::Pro);
                assert_eq!(issued_to.as_deref(), Some("ada@example.com"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn expired_license_is_rejected() {
        let _guard = ENV_LOCK.lock().unwrap();
        env::remove_var("FLOWD_INTELLIGENCE_DEV");
        let dir = tempdir().unwrap();
        let path = dir.path().join("license.toml");
        fs::write(
            &path,
            r#"
schema_version = 1
tier = "eval"
expires_at = "2020-01-01T00:00:00Z"
"#,
        )
        .unwrap();
        let status = resolve_intelligence_entitlement_at(&path, Utc::now());
        assert!(!status.allows_intelligence());
        assert!(matches!(status, IntelligenceEntitlement::Expired { .. }));
    }

    #[test]
    fn dev_env_overrides_missing_license() {
        let _guard = ENV_LOCK.lock().unwrap();
        env::set_var("FLOWD_INTELLIGENCE_DEV", "1");
        let dir = tempdir().unwrap();
        let path = dir.path().join("missing.toml");
        let status = resolve_intelligence_entitlement_at(&path, Utc::now());
        env::remove_var("FLOWD_INTELLIGENCE_DEV");
        assert_eq!(status, IntelligenceEntitlement::DevMode);
        assert!(status.allows_intelligence());
    }
}
