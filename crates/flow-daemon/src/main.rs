mod observation;

use anyhow::{Context, Result};
use chrono::Duration;
use flow_adapters::file_watcher::{event_to_file_events, notify_channel, watch_path};
use flow_analysis::catch_up_analysis;
use flow_core::config::Config;
use flow_db::open_database as open_sqlite_database;
use observation::ObservationPipeline;
use rusqlite::Connection;
use std::{
    env,
    path::{Path, PathBuf},
};

const FILE_EVENT_DEDUP_WINDOW_MS: i64 = 500;
const SESSION_INACTIVITY_SECS: i64 = 300;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let config = load_config().context("failed to load daemon config")?;
    let observed_paths = resolve_observed_paths(&config)?;
    let mut conn = open_database(&config).context("failed to initialize daemon database")?;

    catch_up_analysis(&mut conn, SESSION_INACTIVITY_SECS)
        .context("failed to catch up analysis state")?;

    let (mut watcher, rx) = notify_channel().context("failed to create filesystem watcher")?;
    let mut observation =
        ObservationPipeline::new(Duration::milliseconds(FILE_EVENT_DEDUP_WINDOW_MS));

    for path in &observed_paths {
        watch_path(&mut watcher, path)
            .with_context(|| format!("failed to watch {}", path.display()))?;
        println!("watching {}", path.display());
    }

    for result in rx {
        match result {
            Ok(event) => {
                for file_event in event_to_file_events(&event) {
                    let Some(raw_event) = observation
                        .accept(&conn, file_event)
                        .context("failed during observation")?
                    else {
                        continue;
                    };

                    catch_up_analysis(&mut conn, SESSION_INACTIVITY_SECS)
                        .context("failed during analysis refresh")?;
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
    use chrono::{TimeZone, Utc};
    use flow_adapters::file_watcher::{synthetic_file_event, FileEvent, FileEventKind};
    use flow_core::events::EventSource;
    use observation::RecentFileEventDeduper;
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

        flow_db::repo::insert_raw_event(&conn, &raw_event).unwrap();

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
    fn normalizes_persisted_file_events_into_sqlite() {
        let dir = tempdir().unwrap();
        let config = Config {
            database_path: dir.path().join("flowd.db").display().to_string(),
            ..Config::default()
        };
        let mut conn = open_database(&config).unwrap();
        let raw_event = synthetic_file_event(
            Utc::now(),
            FileEventKind::Move,
            dir.path()
                .join("archive")
                .join("report.txt")
                .display()
                .to_string(),
            Some(dir.path().join("report.txt").display().to_string()),
        );

        flow_db::repo::insert_raw_event(&conn, &raw_event).unwrap();
        flow_analysis::normalize_pending_raw_events(&mut conn).unwrap();
        flow_analysis::normalize_pending_raw_events(&mut conn).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM normalized_events", [], |row| {
                row.get(0)
            })
            .unwrap();
        let raw_event_id: i64 = conn
            .query_row(
                "SELECT raw_event_id FROM normalized_events ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(count, 1);
        assert_eq!(raw_event_id, 1);
    }

    #[test]
    fn refreshes_patterns_from_persisted_normalized_events() {
        let dir = tempdir().unwrap();
        let config = Config {
            database_path: dir.path().join("flowd.db").display().to_string(),
            ..Config::default()
        };
        let mut conn = open_database(&config).unwrap();

        let events = [
            synthetic_file_event(
                Utc.with_ymd_and_hms(2026, 3, 11, 9, 0, 0).unwrap(),
                FileEventKind::Create,
                dir.path().join("invoice-1001.pdf").display().to_string(),
                None,
            ),
            synthetic_file_event(
                Utc.with_ymd_and_hms(2026, 3, 11, 9, 0, 20).unwrap(),
                FileEventKind::Rename,
                dir.path()
                    .join("invoice-1001-reviewed.pdf")
                    .display()
                    .to_string(),
                Some(dir.path().join("invoice-1001.pdf").display().to_string()),
            ),
            synthetic_file_event(
                Utc.with_ymd_and_hms(2026, 3, 11, 9, 0, 40).unwrap(),
                FileEventKind::Move,
                dir.path()
                    .join("archive")
                    .join("invoice-1001-reviewed.pdf")
                    .display()
                    .to_string(),
                Some(
                    dir.path()
                        .join("invoice-1001-reviewed.pdf")
                        .display()
                        .to_string(),
                ),
            ),
            synthetic_file_event(
                Utc.with_ymd_and_hms(2026, 3, 11, 10, 0, 0).unwrap(),
                FileEventKind::Create,
                dir.path().join("invoice-1002.pdf").display().to_string(),
                None,
            ),
            synthetic_file_event(
                Utc.with_ymd_and_hms(2026, 3, 11, 10, 0, 20).unwrap(),
                FileEventKind::Rename,
                dir.path()
                    .join("invoice-1002-reviewed.pdf")
                    .display()
                    .to_string(),
                Some(dir.path().join("invoice-1002.pdf").display().to_string()),
            ),
            synthetic_file_event(
                Utc.with_ymd_and_hms(2026, 3, 11, 10, 0, 40).unwrap(),
                FileEventKind::Move,
                dir.path()
                    .join("archive")
                    .join("invoice-1002-reviewed.pdf")
                    .display()
                    .to_string(),
                Some(
                    dir.path()
                        .join("invoice-1002-reviewed.pdf")
                        .display()
                        .to_string(),
                ),
            ),
        ];

        for event in events {
            flow_db::repo::insert_raw_event(&conn, &event).unwrap();
        }

        flow_analysis::normalize_pending_raw_events(&mut conn).unwrap();
        flow_analysis::refresh_analysis_state(&mut conn, SESSION_INACTIVITY_SECS).unwrap();
        flow_analysis::refresh_analysis_state(&mut conn, SESSION_INACTIVITY_SECS).unwrap();

        let pattern_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM patterns", [], |row| row.get(0))
            .unwrap();
        let suggestion_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM suggestions", [], |row| row.get(0))
            .unwrap();
        let repeats: i64 = conn
            .query_row("SELECT count FROM patterns LIMIT 1", [], |row| row.get(0))
            .unwrap();

        assert_eq!(pattern_count, 1);
        assert_eq!(suggestion_count, 1);
        assert_eq!(repeats, 2);
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
