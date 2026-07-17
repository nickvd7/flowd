use crate::config::{expand_home, Config};
use std::path::{Component, Path, PathBuf};

/// Allowed filesystem roots for automation planning and execution.
///
/// When unrestricted (tests only), any path is accepted. Production configs
/// should always use concrete roots derived from `observed_folders` and/or
/// `execution_allowed_roots`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathAllowlist {
    roots: Option<Vec<PathBuf>>,
}

impl PathAllowlist {
    pub fn unrestricted() -> Self {
        Self { roots: None }
    }

    pub fn from_roots(roots: impl IntoIterator<Item = PathBuf>) -> Self {
        let roots: Vec<_> = roots
            .into_iter()
            .map(|root| normalize_lexical(&root))
            .filter(|root| !root.as_os_str().is_empty())
            .collect();
        Self {
            roots: Some(roots),
        }
    }

    /// Build an allowlist from config. Empty `execution_allowed_roots` means
    /// fall back to `observed_folders`.
    pub fn from_config(config: &Config) -> Self {
        let source = if config.execution_allowed_roots.is_empty() {
            config.observed_folders.as_slice()
        } else {
            config.execution_allowed_roots.as_slice()
        };
        Self::from_roots(source.iter().map(|value| expand_home(value)))
    }

    pub fn roots(&self) -> Option<&[PathBuf]> {
        self.roots.as_deref()
    }

    pub fn allows(&self, path: &Path) -> bool {
        let Some(roots) = &self.roots else {
            return true;
        };
        if roots.is_empty() {
            return false;
        }
        let candidate = resolve_for_allowlist(path);
        roots.iter().any(|root| path_is_within(&candidate, root))
    }

    pub fn ensure_allows(&self, path: &Path) -> Result<(), String> {
        if self.allows(path) {
            Ok(())
        } else {
            Err(format!(
                "path escapes execution allowlist: {}",
                path.display()
            ))
        }
    }
}

fn resolve_for_allowlist(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    if let Some(parent) = path.parent() {
        if let Ok(canonical_parent) = parent.canonicalize() {
            if let Some(name) = path.file_name() {
                return canonical_parent.join(name);
            }
        }
    }
    normalize_lexical(path)
}

fn path_is_within(path: &Path, root: &Path) -> bool {
    let root = normalize_lexical(root);
    let path = normalize_lexical(path);
    if path == root {
        return true;
    }
    let mut path_iter = path.components();
    for root_component in root.components() {
        match path_iter.next() {
            Some(component) if component == root_component => {}
            _ => return false,
        }
    }
    true
}

fn normalize_lexical(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn allows_paths_under_configured_roots() {
        let dir = tempdir().unwrap();
        let downloads = dir.path().join("Downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        let allowlist = PathAllowlist::from_roots([downloads.clone()]);
        assert!(allowlist.allows(&downloads.join("invoice.pdf")));
        assert!(!allowlist.allows(&dir.path().join("Secrets/secret.pdf")));
    }

    #[test]
    fn rejects_parent_directory_escape() {
        let dir = tempdir().unwrap();
        let downloads = dir.path().join("Downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        let allowlist = PathAllowlist::from_roots([downloads]);
        let escape = dir.path().join("Downloads/../Secrets/file.pdf");
        assert!(!allowlist.allows(&escape));
    }

    #[test]
    fn from_config_falls_back_to_observed_folders() {
        let config = Config {
            observed_folders: vec!["~/Downloads".to_string()],
            execution_allowed_roots: Vec::new(),
            ..Config::default()
        };
        let allowlist = PathAllowlist::from_config(&config);
        let roots = allowlist.roots().unwrap();
        assert_eq!(roots.len(), 1);
        assert!(roots[0].ends_with("Downloads"));
    }
}
