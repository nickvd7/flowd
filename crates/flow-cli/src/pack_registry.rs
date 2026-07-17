use anyhow::{anyhow, bail, Context, Result};
use flow_dsl::{
    parse_pack_manifest, parse_pack_registry_index, PackRegistryEntry, PackRegistryIndex,
    WorkflowPackManifest,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistrySource {
    Local(PathBuf),
    Http(String),
}

impl RegistrySource {
    pub fn parse(raw: &str) -> Self {
        if raw.starts_with("https://") || raw.starts_with("http://") {
            Self::Http(raw.to_string())
        } else {
            Self::Local(PathBuf::from(raw))
        }
    }

    pub fn display(&self) -> String {
        match self {
            Self::Local(path) => path.display().to_string(),
            Self::Http(url) => url.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadedRegistry {
    pub source: RegistrySource,
    pub index: PackRegistryIndex,
    base: RegistryBase,
}

#[derive(Debug, Clone)]
enum RegistryBase {
    Local(PathBuf),
    Http(String),
}

impl LoadedRegistry {
    pub fn load(source: RegistrySource) -> Result<Self> {
        let (contents, base) = match &source {
            RegistrySource::Local(path) => {
                let contents = fs::read_to_string(path).with_context(|| {
                    format!("failed to read pack registry index at {}", path.display())
                })?;
                let base = path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf();
                (contents, RegistryBase::Local(base))
            }
            RegistrySource::Http(url) => {
                let contents = http_get_text(url)
                    .with_context(|| format!("failed to fetch pack registry index from {url}"))?;
                let base = http_parent_url(url)?;
                (contents, RegistryBase::Http(base))
            }
        };

        let index = parse_pack_registry_index(&contents)
            .with_context(|| format!("failed to parse pack registry index from {}", source.display()))?;
        if index.registry.schema_version != 1 {
            bail!(
                "unsupported pack registry schema_version {} (expected 1)",
                index.registry.schema_version
            );
        }

        Ok(Self {
            source,
            index,
            base,
        })
    }

    pub fn search(&self, query: Option<&str>) -> Vec<&PackRegistryEntry> {
        let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) else {
            return self.index.packs.iter().collect();
        };
        let needle = query.to_ascii_lowercase();
        self.index
            .packs
            .iter()
            .filter(|entry| {
                entry.id.to_ascii_lowercase().contains(&needle)
                    || entry.name.to_ascii_lowercase().contains(&needle)
                    || entry
                        .description
                        .as_deref()
                        .map(|value| value.to_ascii_lowercase().contains(&needle))
                        .unwrap_or(false)
            })
            .collect()
    }

    pub fn find_pack(&self, pack_id: &str) -> Result<&PackRegistryEntry> {
        self.index
            .packs
            .iter()
            .find(|entry| entry.id == pack_id)
            .ok_or_else(|| {
                anyhow!(
                    "pack '{}' was not found in registry {}",
                    pack_id,
                    self.source.display()
                )
            })
    }

    /// Materialize a pack directory (local copy or HTTPS file fetch) into `destination`.
    pub fn materialize_pack(&self, entry: &PackRegistryEntry, destination: &Path) -> Result<()> {
        if destination.exists() {
            fs::remove_dir_all(destination).with_context(|| {
                format!(
                    "failed to clear temporary pack directory {}",
                    destination.display()
                )
            })?;
        }
        fs::create_dir_all(destination).with_context(|| {
            format!(
                "failed to create temporary pack directory {}",
                destination.display()
            )
        })?;

        match &self.base {
            RegistryBase::Local(base) => {
                let source_dir = base.join(&entry.path);
                copy_dir_recursive(&source_dir, destination).with_context(|| {
                    format!(
                        "failed to copy pack '{}' from {}",
                        entry.id,
                        source_dir.display()
                    )
                })?;
            }
            RegistryBase::Http(base) => {
                fetch_pack_tree(base, &entry.path, destination).with_context(|| {
                    format!(
                        "failed to download pack '{}' from registry {}",
                        entry.id,
                        self.source.display()
                    )
                })?;
            }
        }

        Ok(())
    }
}

fn fetch_pack_tree(base_url: &str, pack_path: &str, destination: &Path) -> Result<()> {
    let pack_base = join_url(base_url, pack_path)?;
    let manifest_url = join_url(&pack_base, "workflow-pack.toml")?;
    let manifest_text = http_get_text(&manifest_url)
        .with_context(|| format!("failed to download pack manifest from {manifest_url}"))?;
    let manifest_path = destination.join("workflow-pack.toml");
    fs::write(&manifest_path, &manifest_text).with_context(|| {
        format!(
            "failed to write downloaded manifest to {}",
            manifest_path.display()
        )
    })?;

    let manifest: WorkflowPackManifest = parse_pack_manifest(&manifest_text)
        .with_context(|| format!("failed to parse downloaded manifest from {manifest_url}"))?;

    for automation_ref in &manifest.automation {
        let file_url = join_url(&pack_base, &automation_ref.file)?;
        let contents = http_get_text(&file_url)
            .with_context(|| format!("failed to download automation spec from {file_url}"))?;
        let target = destination.join(&automation_ref.file);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create automation directory {}", parent.display())
            })?;
        }
        fs::write(&target, contents).with_context(|| {
            format!(
                "failed to write downloaded automation spec to {}",
                target.display()
            )
        })?;
    }

    Ok(())
}

fn http_get_text(url: &str) -> Result<String> {
    let response = ureq::get(url)
        .call()
        .map_err(|error| anyhow!("HTTP GET {url} failed: {error}"))?;
    response
        .into_string()
        .map_err(|error| anyhow!("failed to read response body from {url}: {error}"))
}

fn http_parent_url(url: &str) -> Result<String> {
    let trimmed = url.trim_end_matches('/');
    let Some((parent, _)) = trimmed.rsplit_once('/') else {
        bail!("registry URL has no parent path: {url}");
    };
    if parent == "http:" || parent == "https:" {
        bail!("registry URL has no parent path: {url}");
    }
    Ok(parent.to_string())
}

fn join_url(base: &str, relative: &str) -> Result<String> {
    let relative = relative.trim_start_matches('/');
    if relative.is_empty() {
        return Ok(base.trim_end_matches('/').to_string());
    }

    let mut segments = Vec::new();
    for segment in base.trim_end_matches('/').split('/') {
        segments.push(segment.to_string());
    }
    // Keep scheme + authority intact; only normalize path segments after host.
    let mut path_start = 0usize;
    if segments.len() >= 3 && (segments[0] == "http:" || segments[0] == "https:") {
        path_start = 3;
    }

    for segment in relative.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.len() > path_start {
                    segments.pop();
                } else {
                    bail!("pack path escapes registry base URL: {relative}");
                }
            }
            other => segments.push(other.to_string()),
        }
    }

    Ok(segments.join("/"))
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn loads_and_searches_local_registry() {
        let dir = tempdir().unwrap();
        let pack_dir = dir.path().join("demo-pack");
        fs::create_dir_all(pack_dir.join("automations")).unwrap();
        fs::write(
            pack_dir.join("workflow-pack.toml"),
            r#"
[pack]
id = "demo.rename-downloads"
name = "Demo Rename Downloads"
version = "0.1.0"
description = "Example pack"

automation = [
  { file = "automations/rename-downloads.yaml" }
]
"#,
        )
        .unwrap();
        fs::write(
            pack_dir.join("automations/rename-downloads.yaml"),
            "id: rename-downloads\ntrigger:\n  type: file\nactions: []\n",
        )
        .unwrap();

        let index_path = dir.path().join("index.toml");
        fs::write(
            &index_path,
            r#"
[registry]
name = "test registry"
schema_version = 1

[[packs]]
id = "demo.rename-downloads"
name = "Demo Rename Downloads"
version = "0.1.0"
description = "Example pack for invoices"
path = "demo-pack"
"#,
        )
        .unwrap();

        let registry = LoadedRegistry::load(RegistrySource::Local(index_path)).unwrap();
        assert_eq!(registry.search(None).len(), 1);
        assert_eq!(registry.search(Some("invoice")).len(), 1);
        assert!(registry.search(Some("missing")).is_empty());

        let entry = registry.find_pack("demo.rename-downloads").unwrap();
        let materialized = dir.path().join("materialized");
        registry.materialize_pack(entry, &materialized).unwrap();
        assert!(materialized.join("workflow-pack.toml").is_file());
        assert!(materialized
            .join("automations/rename-downloads.yaml")
            .is_file());
    }

    #[test]
    fn join_url_resolves_relative_segments() {
        assert_eq!(
            join_url("https://example.test/flowd/registry", "../packs/demo").unwrap(),
            "https://example.test/flowd/packs/demo"
        );
    }
}
