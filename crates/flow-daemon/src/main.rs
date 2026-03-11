use anyhow::Context;
use flow_adapters::file_watcher::FileWatcherAdapter;
use flow_core::config::Config;
use flow_db::{migrations::run_migrations, repo::ingest_raw_event};
use rusqlite::Connection;
use std::path::Path;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let config = load_config()?;
    let watched_directories = config.expanded_watched_directories();
    let mut watcher = FileWatcherAdapter::watch_paths(&watched_directories)
        .with_context(|| format!("failed to watch directories: {watched_directories:?}"))?;
    let conn = Connection::open(&config.database_path)
        .with_context(|| format!("failed to open database at {}", config.database_path))?;
    run_migrations(&conn).context("failed to run database migrations")?;

    loop {
        let file_event = watcher.next_event().context("file watcher failed")?;
        println!(
            "file event detected: {:?} {}",
            file_event.kind, file_event.path
        );

        let raw_event = file_event.into_raw_event();
        ingest_raw_event(&conn, &raw_event).context("failed to store raw event pipeline")?;
    }
}

fn load_config() -> anyhow::Result<Config> {
    let config_path = std::env::var("FLOWD_CONFIG")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "flowd.toml".to_string());

    if Path::new(&config_path).exists() {
        Config::load_from_path(&config_path)
            .with_context(|| format!("failed to load config from {config_path}"))
    } else {
        Ok(Config::default())
    }
}
