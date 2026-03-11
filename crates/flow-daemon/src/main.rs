use anyhow::{Context, Result};
use flow_adapters::file_watcher::{event_to_file_events, notify_channel, watch_path};
use flow_core::config::Config;
use std::{
    env,
    path::{Path, PathBuf},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let config = load_config().context("failed to load daemon config")?;
    let observed_paths = resolve_observed_paths(&config)?;
    let (mut watcher, rx) = notify_channel().context("failed to create filesystem watcher")?;

    for path in &observed_paths {
        watch_path(&mut watcher, path)
            .with_context(|| format!("failed to watch {}", path.display()))?;
        println!("watching {}", path.display());
    }

    for result in rx {
        match result {
            Ok(event) => {
                for file_event in event_to_file_events(&event) {
                    let raw_event = file_event.into_raw_event();
                    println!("{}", serde_json::to_string(&raw_event)?);
                }
            }
            Err(error) => eprintln!("watch error: {error}"),
        }
    }

    Ok(())
}

fn load_config() -> Result<Config> {
    let path = Path::new("flowd.toml");
    if path.exists() {
        return Config::load_from_path(path).map_err(Into::into);
    }

    Ok(Config::default())
}

fn resolve_observed_paths(config: &Config) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();

    for folder in &config.observed_folders {
        let path = expand_home(folder);
        if !path.exists() {
            continue;
        }

        if path.is_dir() {
            paths.push(path);
        }
    }

    if paths.is_empty() {
        anyhow::bail!("no existing observed_folders entries could be watched")
    }

    Ok(paths)
}

fn expand_home(raw: &str) -> PathBuf {
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

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_tilde_prefixed_paths() {
        let home = home_dir().unwrap();
        assert_eq!(expand_home("~/Downloads"), home.join("Downloads"));
    }
}
