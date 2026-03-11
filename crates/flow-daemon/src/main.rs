use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use flow_adapters::file_watcher::{event_to_file_events, notify_channel, watch_path, FileEvent};
use flow_core::config::Config;
use flow_core::events::RawEvent;
use flow_db::{open_database as open_sqlite_database, repo::insert_raw_event};
use rusqlite::Connection;
use std::{
    collections::VecDeque,
    env,
    path::{Path, PathBuf},
};

const FILE_EVENT_DEDUP_WINDOW_MS: i64 = 500;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let config = load_config().context("failed to load daemon config")?;
    let observed_paths = resolve_observed_paths(&config)?;
    let conn = open_database(&config).context("failed to initialize daemon database")?;
    let (mut watcher, rx) = notify_channel().context("failed to create filesystem watcher")?;
    let mut deduper =
        RecentFileEventDeduper::new(Duration::milliseconds(FILE_EVENT_DEDUP_WINDOW_MS));

    for path in &observed_paths {
        watch_path(&mut watcher, path)
            .with_context(|| format!("failed to watch {}", path.display()))?;
        println!("watching {}", path.display());
    }

    for result in rx {
        match result {
            Ok(event) => {
                for file_event in event_to_file_events(&event) {
                    if !deduper.should_emit(&file_event) {
                        continue;
                    }

                    let raw_event = file_event.into_raw_event();
                    persist_raw_event(&conn, &raw_event)
                        .context("failed to persist raw filesystem event")?;
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

fn open_database(config: &Config) -> Result<Connection> {
    let db_path = expand_home(&config.database_path);
    open_sqlite_database(&db_path)
}

fn persist_raw_event(conn: &Connection, raw_event: &RawEvent) -> Result<()> {
    insert_raw_event(conn, raw_event).context("failed to insert raw event")?;
    Ok(())
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecentFileEvent {
    ts: DateTime<Utc>,
    key: RecentFileEventKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecentFileEventKey {
    kind: String,
    path: String,
    from_path: Option<String>,
}

#[derive(Debug)]
struct RecentFileEventDeduper {
    window: Duration,
    recent_events: VecDeque<RecentFileEvent>,
}

impl RecentFileEventDeduper {
    fn new(window: Duration) -> Self {
        Self {
            window,
            recent_events: VecDeque::new(),
        }
    }

    fn should_emit(&mut self, event: &FileEvent) -> bool {
        self.prune(event.ts);

        let candidate = RecentFileEvent::from_file_event(event);
        if self
            .recent_events
            .iter()
            .any(|recent| recent.key == candidate.key)
        {
            return false;
        }

        self.recent_events.push_back(candidate);
        true
    }

    fn prune(&mut self, now: DateTime<Utc>) {
        while let Some(oldest) = self.recent_events.front() {
            if now.signed_duration_since(oldest.ts) <= self.window {
                break;
            }

            self.recent_events.pop_front();
        }
    }
}

impl RecentFileEvent {
    fn from_file_event(event: &FileEvent) -> Self {
        Self {
            ts: event.ts,
            key: RecentFileEventKey {
                kind: format!("{:?}", event.kind),
                path: event.path.clone(),
                from_path: event.from_path.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use flow_adapters::file_watcher::{synthetic_file_event, FileEventKind};
    use flow_core::events::EventSource;
    use tempfile::tempdir;

    #[test]
    fn expands_tilde_prefixed_paths() {
        let home = home_dir().unwrap();
        assert_eq!(expand_home("~/Downloads"), home.join("Downloads"));
    }

    #[test]
    fn opens_database_and_runs_migrations() {
        let dir = tempdir().unwrap();
        let config = Config {
            database_path: dir.path().join("flowd.db").display().to_string(),
            ..Config::default()
        };

        let conn = open_database(&config).unwrap();
        let table_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'raw_events'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(table_exists, 1);
    }

    #[test]
    fn persists_raw_events_to_sqlite() {
        let dir = tempdir().unwrap();
        let config = Config {
            database_path: dir.path().join("flowd.db").display().to_string(),
            ..Config::default()
        };
        let conn = open_database(&config).unwrap();
        let raw_event = synthetic_file_event(
            Utc::now(),
            FileEventKind::Create,
            dir.path().join("report.txt").display().to_string(),
            None,
        );

        persist_raw_event(&conn, &raw_event).unwrap();

        let (source, payload_json): (String, String) = conn
            .query_row(
                "SELECT source, payload_json FROM raw_events ORDER BY id DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(source, format!("{:?}", EventSource::FileWatcher));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&payload_json).unwrap(),
            raw_event.payload
        );
    }

    #[test]
    fn suppresses_duplicate_file_events_within_window() {
        let ts = Utc.with_ymd_and_hms(2026, 3, 11, 10, 0, 0).unwrap();
        let mut deduper =
            RecentFileEventDeduper::new(Duration::milliseconds(FILE_EVENT_DEDUP_WINDOW_MS));
        let first = FileEvent {
            ts,
            kind: FileEventKind::Rename,
            path: "/tmp/report-final.txt".to_string(),
            from_path: Some("/tmp/report.txt".to_string()),
        };
        let duplicate = FileEvent {
            ts: ts + Duration::milliseconds(200),
            ..first.clone()
        };

        assert!(deduper.should_emit(&first));
        assert!(!deduper.should_emit(&duplicate));
    }

    #[test]
    fn keeps_matching_file_events_outside_window() {
        let ts = Utc.with_ymd_and_hms(2026, 3, 11, 10, 0, 0).unwrap();
        let mut deduper =
            RecentFileEventDeduper::new(Duration::milliseconds(FILE_EVENT_DEDUP_WINDOW_MS));
        let first = FileEvent {
            ts,
            kind: FileEventKind::Create,
            path: "/tmp/report.txt".to_string(),
            from_path: None,
        };
        let later = FileEvent {
            ts: ts + Duration::milliseconds(FILE_EVENT_DEDUP_WINDOW_MS + 1),
            ..first.clone()
        };

        assert!(deduper.should_emit(&first));
        assert!(deduper.should_emit(&later));
    }

    #[test]
    fn keeps_events_with_different_sources_inside_window() {
        let ts = Utc.with_ymd_and_hms(2026, 3, 11, 10, 0, 0).unwrap();
        let mut deduper =
            RecentFileEventDeduper::new(Duration::milliseconds(FILE_EVENT_DEDUP_WINDOW_MS));
        let first = FileEvent {
            ts,
            kind: FileEventKind::Move,
            path: "/tmp/archive/report.txt".to_string(),
            from_path: Some("/tmp/report.txt".to_string()),
        };
        let second = FileEvent {
            ts: ts + Duration::milliseconds(200),
            kind: FileEventKind::Move,
            path: "/tmp/archive/report.txt".to_string(),
            from_path: Some("/tmp/report-draft.txt".to_string()),
        };

        assert!(deduper.should_emit(&first));
        assert!(deduper.should_emit(&second));
    }
}
