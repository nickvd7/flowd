use crate::config::{expand_home, Config};
use std::path::{Component, Path, PathBuf};

/// Allowed filesystem roots for automation planning and execution.
///
/// When unrestricted (tests only), any path is accepted. Production configs
/// should always use concrete roots derived from `observed_folders` and/or
/// `execution_allowed_roots`.
///
/// Symlinks are resolved with `canonicalize` whenever the path (or its nearest
/// existing parent) exists, so a link that points outside the allowlist cannot
/// be used as an escape hatch.
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
            .map(|root| resolve_root(&root))
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
        roots
            .iter()
            .any(|root| path_is_within(&candidate, &resolve_root(root)))
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

fn resolve_root(root: &Path) -> PathBuf {
    if let Ok(canonical) = root.canonicalize() {
        return canonical;
    }
    normalize_lexical(root)
}

fn resolve_for_allowlist(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }

    // Walk up to the nearest existing ancestor so symlink parents that point
    // outside the allowlist are still detected for not-yet-created children.
    let mut current = normalize_lexical(path);
    let mut suffix = Vec::new();
    loop {
        if let Ok(canonical) = current.canonicalize() {
            let mut resolved = canonical;
            for part in suffix.iter().rev() {
                resolved.push(part);
            }
            return resolved;
        }
        match current.file_name() {
            Some(name) => {
                suffix.push(name.to_os_string());
                match current.parent() {
                    Some(parent) if !parent.as_os_str().is_empty() => {
                        current = parent.to_path_buf();
                    }
                    _ => break,
                }
            }
            None => break,
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
    use std::os::unix::fs::symlink;
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
    fn rejects_string_prefix_lookalike_roots() {
        let dir = tempdir().unwrap();
        let allowed = dir.path().join("allowed");
        let lookalike = dir.path().join("allowed_evil");
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&lookalike).unwrap();
        let allowlist = PathAllowlist::from_roots([allowed]);
        assert!(!allowlist.allows(&lookalike.join("secret.pdf")));
    }

    #[test]
    fn rejects_symlink_file_escape() {
        let dir = tempdir().unwrap();
        let allowed = dir.path().join("allowed");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let secret = outside.join("secret.pdf");
        std::fs::write(&secret, "secret").unwrap();
        let link = allowed.join("escape.pdf");
        symlink(&secret, &link).unwrap();

        let allowlist = PathAllowlist::from_roots([allowed]);
        assert!(!allowlist.allows(&link));
    }

    #[test]
    fn rejects_symlink_directory_escape_for_new_child() {
        let dir = tempdir().unwrap();
        let allowed = dir.path().join("allowed");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let link_dir = allowed.join("portal");
        symlink(&outside, &link_dir).unwrap();

        let allowlist = PathAllowlist::from_roots([allowed]);
        assert!(!allowlist.allows(&link_dir.join("new-file.pdf")));
    }

    #[test]
    fn allows_symlink_that_stays_inside_root() {
        let dir = tempdir().unwrap();
        let allowed = dir.path().join("allowed");
        let nested = allowed.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let target = nested.join("invoice.pdf");
        std::fs::write(&target, "invoice").unwrap();
        let link = allowed.join("alias.pdf");
        symlink(&target, &link).unwrap();

        let allowlist = PathAllowlist::from_roots([allowed]);
        assert!(allowlist.allows(&link));
    }

    #[test]
    fn allows_when_root_itself_is_a_symlink() {
        let dir = tempdir().unwrap();
        let real = dir.path().join("real-root");
        let link_root = dir.path().join("link-root");
        std::fs::create_dir_all(&real).unwrap();
        symlink(&real, &link_root).unwrap();

        let allowlist = PathAllowlist::from_roots([link_root]);
        assert!(allowlist.allows(&real.join("file.pdf")));
    }

    #[test]
    fn rejects_nested_symlink_chain_escape() {
        let dir = tempdir().unwrap();
        let allowed = dir.path().join("allowed");
        let mid = dir.path().join("mid");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&mid).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.pdf"), "secret").unwrap();

        let hop = mid.join("hop");
        symlink(&outside, &hop).unwrap();
        let portal = allowed.join("portal");
        symlink(&hop, &portal).unwrap();

        let allowlist = PathAllowlist::from_roots([allowed]);
        assert!(!allowlist.allows(&portal.join("secret.pdf")));
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
