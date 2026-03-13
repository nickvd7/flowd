use chrono::{DateTime, Utc};
use flow_core::config::{ClipboardCaptureMode, ClipboardObservationConfig, ClipboardPrivacyConfig};
use flow_core::events::{EventSource, RawEvent};
use serde_json::{json, Value};
use std::{env, path::Path, process::Command};

const DEFAULT_BACKEND_TIMEOUT_LABEL: &str = "clipboard backend command failed";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardReadError {
    BackendUnavailable,
    BackendFailed(String),
}

impl std::fmt::Display for ClipboardReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BackendUnavailable => write!(f, "no supported clipboard backend is available"),
            Self::BackendFailed(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ClipboardReadError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardBackend {
    MacOsPbpaste,
    WaylandWlPaste,
    Xclip,
    Xsel,
}

impl ClipboardBackend {
    pub fn command(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Self::MacOsPbpaste => ("pbpaste", &[]),
            Self::WaylandWlPaste => ("wl-paste", &["--no-newline"]),
            Self::Xclip => ("xclip", &["-selection", "clipboard", "-o"]),
            Self::Xsel => ("xsel", &["--clipboard", "--output"]),
        }
    }
}

pub trait ClipboardReader {
    fn read_clipboard(&mut self) -> Result<Option<Vec<u8>>, ClipboardReadError>;
}

#[derive(Debug, Clone)]
pub struct CommandClipboardReader {
    backend: ClipboardBackend,
}

impl CommandClipboardReader {
    pub fn new(backend: ClipboardBackend) -> Self {
        Self { backend }
    }

    pub fn detect() -> Option<Self> {
        let candidates = if cfg!(target_os = "macos") {
            vec![ClipboardBackend::MacOsPbpaste]
        } else {
            vec![
                ClipboardBackend::WaylandWlPaste,
                ClipboardBackend::Xclip,
                ClipboardBackend::Xsel,
            ]
        };

        candidates.into_iter().find_map(|backend| {
            if backend_is_available(backend) {
                Some(Self::new(backend))
            } else {
                None
            }
        })
    }
}

impl ClipboardReader for CommandClipboardReader {
    fn read_clipboard(&mut self) -> Result<Option<Vec<u8>>, ClipboardReadError> {
        let (program, args) = self.backend.command();
        let output = Command::new(program).args(args).output().map_err(|error| {
            ClipboardReadError::BackendFailed(format!("{DEFAULT_BACKEND_TIMEOUT_LABEL}: {error}"))
        })?;

        if !output.status.success() {
            return Err(ClipboardReadError::BackendFailed(format!(
                "{program} exited with status {}",
                output.status
            )));
        }

        if output.stdout.is_empty() {
            return Ok(None);
        }

        Ok(Some(output.stdout))
    }
}

fn backend_is_available(backend: ClipboardBackend) -> bool {
    let (program, args) = backend.command();
    let _ = args;
    binary_in_path(program)
}

fn binary_in_path(program: &str) -> bool {
    if program.contains(std::path::MAIN_SEPARATOR) {
        return Path::new(program).is_file();
    }

    env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| env::split_paths(&paths).collect::<Vec<_>>())
        .map(|dir| dir.join(program))
        .any(|candidate| candidate.is_file())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardObserver<R> {
    reader: R,
    config: ClipboardObservationConfig,
    last_fingerprint: Option<ClipboardFingerprint>,
}

impl<R> ClipboardObserver<R>
where
    R: ClipboardReader,
{
    pub fn new(reader: R, config: ClipboardObservationConfig) -> Self {
        Self {
            reader,
            config,
            last_fingerprint: None,
        }
    }

    pub fn poll(&mut self) -> Result<Option<RawEvent>, ClipboardReadError> {
        let Some(bytes) = self.reader.read_clipboard()? else {
            self.last_fingerprint = None;
            return Ok(None);
        };

        let fingerprint = ClipboardFingerprint::from_bytes(&bytes);
        if self.last_fingerprint.as_ref() == Some(&fingerprint) {
            return Ok(None);
        }

        self.last_fingerprint = Some(fingerprint);
        Ok(Some(snapshot_to_raw_event(
            Utc::now(),
            &bytes,
            &self.config.privacy,
        )))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClipboardFingerprint {
    len: usize,
    hash: u64,
}

impl ClipboardFingerprint {
    fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            len: bytes.len(),
            hash: fnv1a(bytes),
        }
    }
}

pub fn snapshot_to_raw_event(
    ts: DateTime<Utc>,
    bytes: &[u8],
    privacy: &ClipboardPrivacyConfig,
) -> RawEvent {
    let summary = ClipboardSummary::from_bytes(bytes);
    RawEvent {
        ts,
        source: EventSource::Clipboard,
        payload: build_payload(summary, privacy),
    }
}

pub fn synthetic_clipboard_event(
    ts: DateTime<Utc>,
    bytes: &[u8],
    privacy: &ClipboardPrivacyConfig,
) -> RawEvent {
    snapshot_to_raw_event(ts, bytes, privacy)
}

fn build_payload(summary: ClipboardSummary, privacy: &ClipboardPrivacyConfig) -> Value {
    let capture = capture_fields(&summary, privacy);

    json!({
        "kind": "clipboard_change",
        "capture_mode": privacy.mode,
        "content_type": summary.content_type,
        "category": summary.category,
        "content_length": summary.content_length,
        "line_count": summary.line_count,
        "word_count": summary.word_count,
        "contains_whitespace": summary.contains_whitespace,
        "truncated": capture.truncated,
        "captured": capture.captured,
        "redacted_preview": capture.redacted_preview,
        "content_preview": capture.content_preview,
    })
}

struct CaptureFields {
    captured: bool,
    truncated: bool,
    redacted_preview: Option<String>,
    content_preview: Option<String>,
}

fn capture_fields(summary: &ClipboardSummary, privacy: &ClipboardPrivacyConfig) -> CaptureFields {
    match (&summary.text, privacy.mode) {
        (_, ClipboardCaptureMode::MetadataOnly) | (None, _) => CaptureFields {
            captured: false,
            truncated: false,
            redacted_preview: None,
            content_preview: None,
        },
        (Some(text), ClipboardCaptureMode::Redacted) => {
            let truncated = truncate_to_boundary(text, privacy.max_capture_bytes);
            CaptureFields {
                captured: true,
                truncated: truncated.was_truncated,
                redacted_preview: Some(redact_preview(truncated.value)),
                content_preview: None,
            }
        }
        (Some(text), ClipboardCaptureMode::Content) => {
            let truncated = truncate_to_boundary(text, privacy.max_capture_bytes);
            CaptureFields {
                captured: true,
                truncated: truncated.was_truncated,
                redacted_preview: None,
                content_preview: Some(truncated.value.to_string()),
            }
        }
    }
}

struct TruncatedText<'a> {
    value: &'a str,
    was_truncated: bool,
}

fn truncate_to_boundary(value: &str, max_bytes: usize) -> TruncatedText<'_> {
    if max_bytes == 0 || value.len() <= max_bytes {
        return TruncatedText {
            value,
            was_truncated: false,
        };
    }

    let mut boundary = 0usize;
    for (index, ch) in value.char_indices() {
        let next = index + ch.len_utf8();
        if next > max_bytes {
            break;
        }
        boundary = next;
    }

    if boundary == 0 {
        return TruncatedText {
            value: "",
            was_truncated: !value.is_empty(),
        };
    }

    TruncatedText {
        value: &value[..boundary],
        was_truncated: boundary < value.len(),
    }
}

fn redact_preview(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_uppercase() {
                'X'
            } else if ch.is_ascii_lowercase() {
                'x'
            } else if ch.is_ascii_digit() {
                '0'
            } else {
                ch
            }
        })
        .collect()
}

#[derive(Debug, Clone)]
struct ClipboardSummary {
    text: Option<String>,
    content_type: &'static str,
    category: &'static str,
    content_length: usize,
    line_count: usize,
    word_count: usize,
    contains_whitespace: bool,
}

impl ClipboardSummary {
    fn from_bytes(bytes: &[u8]) -> Self {
        match std::str::from_utf8(bytes) {
            Ok(text) => {
                let trimmed = text.trim_matches('\0');
                let line_count = trimmed
                    .lines()
                    .count()
                    .max(usize::from(!trimmed.is_empty()));
                let word_count = trimmed.split_whitespace().count();

                Self {
                    text: Some(trimmed.to_string()),
                    content_type: "text",
                    category: classify_text(trimmed),
                    content_length: trimmed.len(),
                    line_count,
                    word_count,
                    contains_whitespace: trimmed.chars().any(char::is_whitespace),
                }
            }
            Err(_) => Self {
                text: None,
                content_type: "binary",
                category: "binary",
                content_length: bytes.len(),
                line_count: 0,
                word_count: 0,
                contains_whitespace: false,
            },
        }
    }
}

fn classify_text(value: &str) -> &'static str {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "empty"
    } else if looks_like_url(trimmed) {
        "url"
    } else if looks_like_path(trimmed) {
        "path"
    } else if looks_like_filename(trimmed) {
        "filename"
    } else if looks_like_json(trimmed) {
        "json"
    } else if trimmed.contains('\n') || trimmed.contains('\t') {
        "structured_text"
    } else {
        "text"
    }
}

fn looks_like_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

fn looks_like_path(value: &str) -> bool {
    value.starts_with("~/")
        || value.starts_with('/')
        || value.contains('\\')
        || value.split('/').count() >= 2
}

fn looks_like_filename(value: &str) -> bool {
    !value.contains(char::is_whitespace)
        && !value.contains('/')
        && !value.contains('\\')
        && value.split('.').count() >= 2
}

fn looks_like_json(value: &str) -> bool {
    (value.starts_with('{') || value.starts_with('['))
        && serde_json::from_str::<Value>(value).is_ok()
}

fn fnv1a(bytes: &[u8]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    bytes.iter().fold(FNV_OFFSET_BASIS, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::collections::VecDeque;

    struct FakeReader {
        values: VecDeque<Result<Option<Vec<u8>>, ClipboardReadError>>,
    }

    impl FakeReader {
        fn new(values: Vec<Result<Option<Vec<u8>>, ClipboardReadError>>) -> Self {
            Self {
                values: values.into(),
            }
        }
    }

    impl ClipboardReader for FakeReader {
        fn read_clipboard(&mut self) -> Result<Option<Vec<u8>>, ClipboardReadError> {
            self.values.pop_front().unwrap_or_else(|| Ok(None))
        }
    }

    #[test]
    fn metadata_only_mode_emits_only_summary_fields() {
        let raw = synthetic_clipboard_event(
            Utc.with_ymd_and_hms(2026, 3, 13, 9, 0, 0).unwrap(),
            b"/tmp/report.txt",
            &ClipboardPrivacyConfig::default(),
        );

        assert_eq!(raw.source, EventSource::Clipboard);
        assert_eq!(raw.payload["capture_mode"], "metadata_only");
        assert_eq!(raw.payload["category"], "path");
        assert_eq!(raw.payload["captured"], false);
        assert_eq!(raw.payload["redacted_preview"], Value::Null);
        assert_eq!(raw.payload["content_preview"], Value::Null);
    }

    #[test]
    fn redacted_mode_preserves_shape_without_plaintext() {
        let raw = synthetic_clipboard_event(
            Utc.with_ymd_and_hms(2026, 3, 13, 9, 1, 0).unwrap(),
            b"Invoice-1001.pdf",
            &ClipboardPrivacyConfig {
                mode: ClipboardCaptureMode::Redacted,
                max_capture_bytes: 64,
            },
        );

        assert_eq!(raw.payload["captured"], true);
        assert_eq!(raw.payload["redacted_preview"], "Xxxxxxx-0000.xxx");
        assert_eq!(raw.payload["content_preview"], Value::Null);
    }

    #[test]
    fn content_mode_truncates_at_utf8_boundary() {
        let raw = synthetic_clipboard_event(
            Utc.with_ymd_and_hms(2026, 3, 13, 9, 2, 0).unwrap(),
            "résumé.txt".as_bytes(),
            &ClipboardPrivacyConfig {
                mode: ClipboardCaptureMode::Content,
                max_capture_bytes: 5,
            },
        );

        assert_eq!(raw.payload["content_preview"], "résu");
        assert_eq!(raw.payload["truncated"], true);
    }

    #[test]
    fn observer_emits_only_when_clipboard_changes() {
        let reader = FakeReader::new(vec![
            Ok(Some(b"first".to_vec())),
            Ok(Some(b"first".to_vec())),
            Ok(Some(b"second".to_vec())),
        ]);
        let mut observer = ClipboardObserver::new(reader, ClipboardObservationConfig::default());

        let first = observer.poll().unwrap();
        let second = observer.poll().unwrap();
        let third = observer.poll().unwrap();

        assert!(first.is_some());
        assert!(second.is_none());
        assert!(third.is_some());
    }

    #[test]
    fn classifies_structured_text_deterministically() {
        let raw = synthetic_clipboard_event(
            Utc.with_ymd_and_hms(2026, 3, 13, 9, 3, 0).unwrap(),
            br#"{"name":"report","count":2}"#,
            &ClipboardPrivacyConfig::default(),
        );

        assert_eq!(raw.payload["category"], "json");
        assert_eq!(raw.payload["word_count"], 1);
        assert_eq!(raw.payload["line_count"], 1);
    }
}
