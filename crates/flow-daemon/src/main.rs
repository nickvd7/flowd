mod observation;

use anyhow::{Context, Result};
use chrono::Duration;
use clap::Parser;
use flow_adapters::browser::{
    BrowserBridgeError, BrowserDownloadsObserver, BrowserVisitsObserver,
};
use flow_adapters::clipboard::{ClipboardObserver, ClipboardReadError, CommandClipboardReader};
use flow_adapters::file_watcher::{event_to_file_events, notify_channel, watch_path};
use flow_adapters::terminal::{TerminalBridgeError, TerminalHistoryObserver};
use flow_analysis::catch_up_analysis;
#[cfg(feature = "intelligence")]
use flow_analysis::{catch_up_analysis_with_intelligence, PrivateIntelligenceClient};
use flow_core::config::{expand_home, Config};
use flow_core::events::RawEvent;
use flow_core::PathAllowlist;
use flow_db::open_database as open_sqlite_database;
use flow_db::repo::list_automations;
use flow_exec::{execute_automation, execute_automation_for_path};
use observation::ObservationPipeline;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration as StdDuration, Instant};

#[derive(Debug, Parser)]
#[command(name = "flow-daemon", version, about = "Daemon for flowd")]
struct Cli {
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let loaded = Config::load(cli.config.as_deref()).context("failed to load daemon config")?;
    let config = loaded.config;
    let observed_paths = resolve_observed_paths(&config)?;
    let mut conn = open_database(&config).context("failed to initialize daemon database")?;

    refresh_daemon_analysis(&mut conn, &config)
        .context("failed to catch up analysis state")?;

    let (mut watcher, rx) = notify_channel().context("failed to create filesystem watcher")?;
    let mut observation =
        ObservationPipeline::new(Duration::milliseconds(config.file_event_dedup_window_ms));
    let mut clipboard = build_clipboard_observer(&config);
    let mut browser_downloads = build_browser_downloads_observer(&config);
    let mut browser_visits = build_browser_visits_observer(&config);
    let mut terminal_history = build_terminal_history_observer(&config);
    let mut last_auto_run_at: Option<Instant> = None;

    for path in &observed_paths {
        watch_path(&mut watcher, path)
            .with_context(|| format!("failed to watch {}", path.display()))?;
        println!("watching {}", path.display());
    }

    if config.observe_clipboard {
        match clipboard {
            Some(_) => println!(
                "clipboard observation enabled in {} mode",
                serde_json::to_string(&config.clipboard_capture_mode())?.trim_matches('"')
            ),
            None => eprintln!(
                "clipboard observation is enabled but no supported local backend was found"
            ),
        }
    }

    if config.observe_browser_downloads {
        println!(
            "browser download observation enabled at {}",
            expand_home(&config.browser_downloads_bridge_path).display()
        );
    }

    if config.observe_terminal {
        println!(
            "terminal observation enabled at {}",
            expand_home(&config.terminal_history_bridge_path).display()
        );
    }

    if config.auto_run_approved_automations {
        println!("auto-run of approved automations enabled (requires a prior dry-run)");
    }
    if config.observe_browser_visits {
        println!(
            "browser visit observation enabled at {}",
            expand_home(&config.browser_visits_bridge_path).display()
        );
    }

    loop {
        match rx.recv_timeout(std::time::Duration::from_millis(
            config.clipboard_poll_interval_ms,
        )) {
            Ok(result) => match result {
                Ok(event) => {
                    let mut trigger_paths = Vec::new();
                    for file_event in event_to_file_events(&event) {
                        let trigger_path = file_event.path.clone();
                        let Some(raw_event) = observation
                            .accept(&conn, file_event)
                            .context("failed during observation")?
                        else {
                            continue;
                        };

                        trigger_paths.push(PathBuf::from(trigger_path));
                        refresh_daemon_analysis(&mut conn, &config)
                            .context("failed during analysis refresh")?;
                        println!("{}", serde_json::to_string(&raw_event)?);
                    }
                    if !trigger_paths.is_empty() && config.auto_run_approved_automations {
                        maybe_auto_run_approved_automations(
                            &conn,
                            &config,
                            &trigger_paths,
                            &mut last_auto_run_at,
                        )?;
                    }
                }
                Err(error) => eprintln!("watch error: {error}"),
            },
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        if let Some(observer) = clipboard.as_mut() {
            match observer.poll() {
                Ok(Some(raw_event)) => {
                    observation
                        .accept_raw_event(&conn, raw_event.clone())
                        .context("failed during clipboard observation")?;
                    refresh_daemon_analysis(&mut conn, &config)
                        .context("failed during analysis refresh")?;
                    println!("{}", serde_json::to_string(&raw_event)?);
                }
                Ok(None) => {}
                Err(error) => handle_clipboard_error(error),
            }
        }

        if let Some(observer) = browser_downloads.as_mut() {
            match observer.poll() {
                Ok(raw_events) => {
                    let mut trigger_paths = Vec::new();
                    for raw_event in raw_events {
                        if let Some(path) = download_path_from_event(&raw_event) {
                            trigger_paths.push(path);
                        }
                        observation
                            .accept_raw_event(&conn, raw_event.clone())
                            .context("failed during browser download observation")?;
                        refresh_daemon_analysis(&mut conn, &config)
                            .context("failed during analysis refresh")?;
                        println!("{}", serde_json::to_string(&raw_event)?);
                    }
                    if config.auto_run_approved_automations
                        && config.auto_run_on_browser_downloads
                        && !trigger_paths.is_empty()
                    {
                        maybe_auto_run_approved_automations(
                            &conn,
                            &config,
                            &trigger_paths,
                            &mut last_auto_run_at,
                        )?;
                    }
                }
                Err(error) => handle_browser_bridge_error(error),
            }
        }

        if let Some(observer) = browser_visits.as_mut() {
            match observer.poll() {
                Ok(raw_events) => {
                    for raw_event in raw_events {
                        observation
                            .accept_raw_event(&conn, raw_event.clone())
                            .context("failed during browser visit observation")?;
                        refresh_daemon_analysis(&mut conn, &config)
                            .context("failed during analysis refresh")?;
                        println!("{}", serde_json::to_string(&raw_event)?);
                    }
                }
                Err(error) => handle_browser_bridge_error(error),
            }
        }

        if let Some(observer) = terminal_history.as_mut() {
            match observer.poll() {
                Ok(raw_events) => {
                    for raw_event in raw_events {
                        observation
                            .accept_raw_event(&conn, raw_event.clone())
                            .context("failed during terminal observation")?;
                        refresh_daemon_analysis(&mut conn, &config)
                            .context("failed during analysis refresh")?;
                        println!("{}", serde_json::to_string(&raw_event)?);
                    }
                }
                Err(error) => handle_terminal_bridge_error(error),
            }
        }
    }

    Ok(())
}

fn refresh_daemon_analysis(conn: &mut Connection, config: &Config) -> Result<()> {
    #[cfg(feature = "intelligence")]
    if config.intelligence_enabled {
        return catch_up_analysis_with_intelligence(
            conn,
            config.session_inactivity_secs,
            &PrivateIntelligenceClient::from_open_core_config(config),
        );
    }

    catch_up_analysis(conn, config.session_inactivity_secs)
}

fn build_clipboard_observer(config: &Config) -> Option<ClipboardObserver<CommandClipboardReader>> {
    if !config.observe_clipboard {
        return None;
    }

    let reader = CommandClipboardReader::detect()?;
    Some(ClipboardObserver::new(
        reader,
        config.clipboard_observation_config(),
    ))
}

fn build_browser_downloads_observer(config: &Config) -> Option<BrowserDownloadsObserver> {
    if !config.observe_browser_downloads {
        return None;
    }

    Some(BrowserDownloadsObserver::new(
        expand_home(&config.browser_downloads_bridge_path),
        config.strip_browser_query_strings,
    ))
}

fn build_browser_visits_observer(config: &Config) -> Option<BrowserVisitsObserver> {
    if !config.observe_browser_visits {
        return None;
    }

    Some(BrowserVisitsObserver::new(
        expand_home(&config.browser_visits_bridge_path),
        config.strip_browser_query_strings,
    ))
}

fn build_terminal_history_observer(config: &Config) -> Option<TerminalHistoryObserver> {
    if !config.observe_terminal {
        return None;
    }

    Some(TerminalHistoryObserver::new(expand_home(
        &config.terminal_history_bridge_path,
    )))
}

fn maybe_auto_run_approved_automations(
    conn: &Connection,
    config: &Config,
    trigger_paths: &[PathBuf],
    last_auto_run_at: &mut Option<Instant>,
) -> Result<()> {
    if config.auto_run_debounce_ms > 0 {
        if let Some(previous) = *last_auto_run_at {
            if previous.elapsed() < StdDuration::from_millis(config.auto_run_debounce_ms) {
                return Ok(());
            }
        }
    }

    let allowlist = if config.enforce_execution_path_allowlist {
        PathAllowlist::from_config(config)
    } else {
        PathAllowlist::unrestricted()
    };
    let automations = list_automations(conn).context("failed to list automations for auto-run")?;
    let mut ran_any = false;
    for automation in automations
        .into_iter()
        .filter(|automation| automation.status == "active")
    {
        let result = if config.auto_run_trigger_file_only && !trigger_paths.is_empty() {
            let mut operations = Vec::new();
            let mut first_error = None;
            for path in trigger_paths {
                match execute_automation_for_path(conn, automation.automation_id, path, &allowlist)
                {
                    Ok(outcome) => operations.extend(outcome.report.operations),
                    Err(error) => {
                        let message = error.to_string();
                        if message.contains("dry-run") {
                            first_error = Some(error);
                            break;
                        }
                        eprintln!(
                            "auto-run error for automation {} on {}: {error:#}",
                            automation.automation_id,
                            path.display()
                        );
                    }
                }
            }
            if let Some(error) = first_error {
                Err(error)
            } else {
                Ok(flow_exec::ExecutionOutcome {
                    run_id: None,
                    report: flow_exec::ExecutionReport { operations },
                })
            }
        } else {
            execute_automation(conn, automation.automation_id, &allowlist)
        };

        match result {
            Ok(outcome) if outcome.report.operations.is_empty() => {}
            Ok(outcome) => {
                ran_any = true;
                if let Some(run_id) = outcome.run_id {
                    println!(
                        "auto-run recorded as run {run_id} for automation {}",
                        automation.automation_id
                    );
                }
                for operation in outcome.report.operations {
                    println!(
                        "auto-run {}: {} -> {}",
                        automation.automation_id, operation.from, operation.to
                    );
                }
            }
            Err(error) => {
                let message = error.to_string();
                if message.contains("dry-run") {
                    continue;
                }
                eprintln!(
                    "auto-run error for automation {}: {error:#}",
                    automation.automation_id
                );
            }
        }
    }

    if ran_any || trigger_paths.is_empty() {
        *last_auto_run_at = Some(Instant::now());
    }
    Ok(())
}

fn download_path_from_event(event: &RawEvent) -> Option<PathBuf> {
    event
        .payload
        .get("path")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn handle_clipboard_error(error: ClipboardReadError) {
    eprintln!("clipboard observation error: {error}");
}

fn handle_browser_bridge_error(error: BrowserBridgeError) {
    eprintln!("browser download observation error: {error}");
}

fn handle_terminal_bridge_error(error: TerminalBridgeError) {
    eprintln!("terminal observation error: {error}");
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use flow_adapters::{
        browser::synthetic_download_event,
        file_watcher::{synthetic_file_event, FileEvent, FileEventKind},
    };
    use flow_core::events::EventSource;
    use observation::RecentFileEventDeduper;
    use tempfile::tempdir;

    const DEFAULT_FILE_EVENT_DEDUP_WINDOW_MS: i64 = 500;
    const DEFAULT_SESSION_INACTIVITY_SECS: i64 = 300;

    #[test]
    fn expands_tilde_prefixed_paths() {
        let home = flow_core::config::home_dir().unwrap();
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
    fn persists_browser_download_raw_events_into_sqlite() {
        let dir = tempdir().unwrap();
        let config = Config {
            database_path: dir.path().join("flowd.db").display().to_string(),
            ..Config::default()
        };
        let conn = open_database(&config).unwrap();
        let raw_event = synthetic_download_event(
            Utc.with_ymd_and_hms(2026, 3, 13, 10, 0, 2).unwrap(),
            "invoice-1001.pdf",
            Some("/tmp/Downloads/invoice-1001.pdf".to_string()),
            Some("chrome".to_string()),
            Some("https://example.test/files/invoice-1001.pdf?token=secret".to_string()),
            Some("https://example.test/invoices?month=march".to_string()),
            Some(Utc.with_ymd_and_hms(2026, 3, 13, 10, 0, 0).unwrap()),
            true,
        );

        flow_db::repo::insert_raw_event(&conn, &raw_event).unwrap();

        let (source, payload_json): (String, String) = conn
            .query_row(
                "SELECT source, payload_json FROM raw_events ORDER BY id DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(source, format!("{:?}", EventSource::Browser));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&payload_json).unwrap(),
            raw_event.payload
        );
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
        flow_analysis::refresh_analysis_state(&mut conn, DEFAULT_SESSION_INACTIVITY_SECS).unwrap();
        flow_analysis::refresh_analysis_state(&mut conn, DEFAULT_SESSION_INACTIVITY_SECS).unwrap();

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
            RecentFileEventDeduper::new(Duration::milliseconds(DEFAULT_FILE_EVENT_DEDUP_WINDOW_MS));
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
            RecentFileEventDeduper::new(Duration::milliseconds(DEFAULT_FILE_EVENT_DEDUP_WINDOW_MS));
        let first = FileEvent {
            ts,
            kind: FileEventKind::Create,
            path: "/tmp/report.txt".to_string(),
            from_path: None,
        };
        let later = FileEvent {
            ts: ts + Duration::milliseconds(DEFAULT_FILE_EVENT_DEDUP_WINDOW_MS + 1),
            ..first.clone()
        };

        assert!(deduper.should_emit(&first));
        assert!(deduper.should_emit(&later));
    }

    #[test]
    fn keeps_events_with_different_sources_inside_window() {
        let ts = Utc.with_ymd_and_hms(2026, 3, 11, 10, 0, 0).unwrap();
        let mut deduper =
            RecentFileEventDeduper::new(Duration::milliseconds(DEFAULT_FILE_EVENT_DEDUP_WINDOW_MS));
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
