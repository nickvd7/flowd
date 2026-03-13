use chrono::{DateTime, Utc};
use flow_core::events::{EventSource, RawEvent};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserBridgeError {
    Io(String),
    InvalidRecord(String),
}

impl std::fmt::Display for BrowserBridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(message) => write!(f, "{message}"),
            Self::InvalidRecord(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for BrowserBridgeError {}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct BrowserDownloadRecord {
    pub ts: DateTime<Utc>,
    pub filename: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub browser: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub page_url: Option<String>,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserDownloadsObserver {
    bridge_path: PathBuf,
    strip_query_strings: bool,
    consumed_line_count: usize,
    last_seen_content_len: usize,
}

impl BrowserDownloadsObserver {
    pub fn new(bridge_path: PathBuf, strip_query_strings: bool) -> Self {
        Self {
            bridge_path,
            strip_query_strings,
            consumed_line_count: 0,
            last_seen_content_len: 0,
        }
    }

    pub fn poll(&mut self) -> Result<Vec<RawEvent>, BrowserBridgeError> {
        let content = match fs::read_to_string(&self.bridge_path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(BrowserBridgeError::Io(format!(
                    "failed to read browser downloads bridge {}: {error}",
                    self.bridge_path.display()
                )))
            }
        };

        let complete_lines: Vec<&str> = content
            .split_inclusive('\n')
            .filter(|line| line.ends_with('\n'))
            .map(|line| line.trim_end_matches('\n'))
            .filter(|line| !line.trim().is_empty())
            .collect();

        if content.len() < self.last_seen_content_len
            || complete_lines.len() < self.consumed_line_count
        {
            self.consumed_line_count = 0;
        }

        let mut events = Vec::new();
        for line in complete_lines.iter().skip(self.consumed_line_count) {
            events.push(download_line_to_raw_event(line, self.strip_query_strings)?);
        }

        self.consumed_line_count = complete_lines.len();
        self.last_seen_content_len = content.len();
        Ok(events)
    }
}

pub fn visit_event(url: &str, title: &str) -> RawEvent {
    RawEvent {
        ts: Utc::now(),
        source: EventSource::Browser,
        payload: json!({
            "kind": "visit",
            "url": url,
            "title": title
        }),
    }
}

pub fn download_line_to_raw_event(
    line: &str,
    strip_query_strings: bool,
) -> Result<RawEvent, BrowserBridgeError> {
    let record: BrowserDownloadRecord = serde_json::from_str(line).map_err(|error| {
        BrowserBridgeError::InvalidRecord(format!("invalid browser download record: {error}"))
    })?;

    record_to_raw_event(&record, strip_query_strings)
}

pub fn record_to_raw_event(
    record: &BrowserDownloadRecord,
    strip_query_strings: bool,
) -> Result<RawEvent, BrowserBridgeError> {
    let filename = record.filename.trim();
    if filename.is_empty() {
        return Err(BrowserBridgeError::InvalidRecord(
            "browser download record filename must not be empty".to_string(),
        ));
    }

    let extension = file_extension(record.path.as_deref().unwrap_or(filename));
    let started_at = record.started_at.unwrap_or(record.ts);
    let duration_ms = record
        .ts
        .signed_duration_since(started_at)
        .num_milliseconds();

    Ok(RawEvent {
        ts: record.ts,
        source: EventSource::Browser,
        payload: json!({
            "kind": "download",
            "filename": filename,
            "path": record.path,
            "extension": extension,
            "browser": record.browser,
            "source_url": sanitize_optional_url(record.source_url.as_deref(), strip_query_strings),
            "page_url": sanitize_optional_url(record.page_url.as_deref(), strip_query_strings),
            "started_at": record.started_at.map(|value| value.to_rfc3339()),
            "duration_ms": duration_ms.max(0),
        }),
    })
}

pub fn synthetic_download_event(
    ts: DateTime<Utc>,
    filename: impl Into<String>,
    path: Option<String>,
    browser: Option<String>,
    source_url: Option<String>,
    page_url: Option<String>,
    started_at: Option<DateTime<Utc>>,
    strip_query_strings: bool,
) -> RawEvent {
    let record = BrowserDownloadRecord {
        ts,
        filename: filename.into(),
        path,
        browser,
        source_url,
        page_url,
        started_at,
    };
    record_to_raw_event(&record, strip_query_strings)
        .expect("synthetic browser download event must be valid")
}

fn sanitize_optional_url(value: Option<&str>, strip_query_strings: bool) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| sanitize_url(value, strip_query_strings))
}

fn sanitize_url(value: &str, strip_query_strings: bool) -> String {
    if !strip_query_strings {
        return value.to_string();
    }

    let base = value.split('#').next().unwrap_or(value);
    base.split('?').next().unwrap_or(base).to_string()
}

fn file_extension(path_or_name: &str) -> String {
    Path::new(path_or_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn download_record_serializes_into_browser_raw_event() {
        let raw = synthetic_download_event(
            Utc.with_ymd_and_hms(2026, 3, 13, 10, 0, 2).unwrap(),
            "invoice-1001.pdf",
            Some("/tmp/Downloads/invoice-1001.pdf".to_string()),
            Some("chrome".to_string()),
            Some("https://example.test/files/invoice-1001.pdf?token=secret".to_string()),
            Some("https://example.test/invoices?month=march".to_string()),
            Some(Utc.with_ymd_and_hms(2026, 3, 13, 10, 0, 0).unwrap()),
            true,
        );

        assert_eq!(raw.source, EventSource::Browser);
        assert_eq!(raw.payload["kind"], "download");
        assert_eq!(raw.payload["filename"], "invoice-1001.pdf");
        assert_eq!(raw.payload["extension"], "pdf");
        assert_eq!(raw.payload["browser"], "chrome");
        assert_eq!(
            raw.payload["source_url"],
            "https://example.test/files/invoice-1001.pdf"
        );
        assert_eq!(raw.payload["page_url"], "https://example.test/invoices");
        assert_eq!(raw.payload["duration_ms"], 2000);
    }

    #[test]
    fn observer_ingests_only_new_complete_records() {
        let dir = tempdir().unwrap();
        let bridge_path = dir.path().join("browser-downloads.ndjson");
        fs::write(
            &bridge_path,
            concat!(
                "{\"ts\":\"2026-03-13T10:00:02Z\",\"filename\":\"invoice-1001.pdf\",\"browser\":\"chrome\"}\n",
                "{\"ts\":\"2026-03-13T10:00:03Z\",\"filename\":\"report.csv\",\"browser\":\"firefox\"}"
            ),
        )
        .unwrap();

        let mut observer = BrowserDownloadsObserver::new(bridge_path.clone(), true);

        let first_poll = observer.poll().unwrap();
        assert_eq!(first_poll.len(), 1);
        assert_eq!(first_poll[0].payload["filename"], "invoice-1001.pdf");

        let second_poll = observer.poll().unwrap();
        assert!(second_poll.is_empty());

        fs::write(
            &bridge_path,
            concat!(
                "{\"ts\":\"2026-03-13T10:00:02Z\",\"filename\":\"invoice-1001.pdf\",\"browser\":\"chrome\"}\n",
                "{\"ts\":\"2026-03-13T10:00:03Z\",\"filename\":\"report.csv\",\"browser\":\"firefox\"}\n"
            ),
        )
        .unwrap();

        let third_poll = observer.poll().unwrap();
        assert_eq!(third_poll.len(), 1);
        assert_eq!(third_poll[0].payload["filename"], "report.csv");
    }

    #[test]
    fn observer_resets_after_bridge_truncation() {
        let dir = tempdir().unwrap();
        let bridge_path = dir.path().join("browser-downloads.ndjson");
        fs::write(
            &bridge_path,
            "{\"ts\":\"2026-03-13T10:00:02Z\",\"filename\":\"invoice-1001.pdf\"}\n",
        )
        .unwrap();

        let mut observer = BrowserDownloadsObserver::new(bridge_path.clone(), true);
        assert_eq!(observer.poll().unwrap().len(), 1);

        fs::write(
            &bridge_path,
            "{\"ts\":\"2026-03-13T10:05:00Z\",\"filename\":\"report.csv\"}\n",
        )
        .unwrap();

        let next_poll = observer.poll().unwrap();
        assert_eq!(next_poll.len(), 1);
        assert_eq!(next_poll[0].payload["filename"], "report.csv");
    }
}
