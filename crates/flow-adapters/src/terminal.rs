use chrono::{DateTime, Utc};
use flow_core::events::{EventSource, RawEvent};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct TerminalHistoryRecord {
    pub ts: DateTime<Utc>,
    pub cwd: String,
    pub command: String,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub shell: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum TerminalCommandKind {
    Command,
    Copy,
    Mkdir,
    Move,
    Remove,
    Rename,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalHistoryError {
    InvalidJson(String),
    InvalidCommand(String),
}

impl std::fmt::Display for TerminalHistoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidJson(message) => write!(f, "invalid terminal history JSON: {message}"),
            Self::InvalidCommand(message) => write!(f, "invalid terminal command: {message}"),
        }
    }
}

impl std::error::Error for TerminalHistoryError {}

pub fn history_line_to_raw_event(line: &str) -> Result<RawEvent, TerminalHistoryError> {
    let record: TerminalHistoryRecord = serde_json::from_str(line)
        .map_err(|error| TerminalHistoryError::InvalidJson(error.to_string()))?;
    record_to_raw_event(&record)
}

pub fn record_to_raw_event(
    record: &TerminalHistoryRecord,
) -> Result<RawEvent, TerminalHistoryError> {
    let tokens = tokenize_command_line(&record.command)?;
    let command_name = tokens
        .first()
        .ok_or_else(|| TerminalHistoryError::InvalidCommand("missing command name".to_string()))?
        .to_string();
    let signal = classify_command(record, &tokens);

    Ok(RawEvent {
        ts: record.ts,
        source: EventSource::Terminal,
        payload: json!({
            "kind": signal.kind,
            "command_name": command_name,
            "cwd": record.cwd,
            "exit_code": record.exit_code,
            "shell": record.shell,
            "arg_count": tokens.len().saturating_sub(1),
            "succeeded": record.exit_code.unwrap_or_default() == 0,
            "redacted_command": redact_tokens(&tokens),
            "path": signal.path,
            "from_path": signal.from_path,
            "paths": signal.paths,
            "path_count": signal.path_count,
            "source_paths": signal.source_paths,
            "target_paths": signal.target_paths,
            "command_kinds": signal.command_kinds,
            "command_sequence_pattern": signal.command_sequence_pattern,
            "directory_structures": signal.directory_structures,
            "command_count": signal.command_count,
            "destructive": matches!(signal.kind, TerminalCommandKind::Remove),
        }),
    })
}

pub fn synthetic_terminal_history_event(
    ts: DateTime<Utc>,
    cwd: impl Into<String>,
    command: impl Into<String>,
    exit_code: Option<i32>,
) -> RawEvent {
    let record = TerminalHistoryRecord {
        ts,
        cwd: cwd.into(),
        command: command.into(),
        exit_code,
        shell: Some("test-shell".to_string()),
    };
    record_to_raw_event(&record).expect("synthetic terminal history event must be valid")
}

struct TerminalSignal {
    kind: TerminalCommandKind,
    path: Option<String>,
    from_path: Option<String>,
    paths: Vec<String>,
    path_count: usize,
    source_paths: Vec<String>,
    target_paths: Vec<String>,
    command_kinds: Vec<String>,
    command_sequence_pattern: String,
    directory_structures: Vec<String>,
    command_count: usize,
}

fn classify_command(record: &TerminalHistoryRecord, tokens: &[String]) -> TerminalSignal {
    let segments = split_command_segments(&record.command)
        .ok()
        .filter(|segments| !segments.is_empty())
        .unwrap_or_else(|| vec![record.command.clone()]);
    let mut classified_segments = Vec::with_capacity(segments.len());

    for segment in segments {
        let segment_tokens = tokenize_command_line(&segment).unwrap_or_else(|_| tokens.to_vec());
        classified_segments.push(classify_command_segment(&record.cwd, &segment_tokens));
    }

    aggregate_segment_signals(classified_segments)
}

#[derive(Debug, Clone)]
struct CommandSegmentSignal {
    kind: TerminalCommandKind,
    path: Option<String>,
    from_path: Option<String>,
    paths: Vec<String>,
    source_paths: Vec<String>,
    target_paths: Vec<String>,
}

fn classify_command_segment(cwd: &str, tokens: &[String]) -> CommandSegmentSignal {
    let command_name = tokens
        .first()
        .map(|value| value.as_str())
        .unwrap_or("command");
    let args = if tokens.is_empty() {
        &[][..]
    } else {
        &tokens[1..]
    };
    let path_args = collect_path_args(command_name, args);

    match command_name {
        "mv" => classify_move_command(cwd, &path_args),
        "cp" => classify_copy_command(cwd, &path_args),
        "mkdir" => classify_mkdir_command(cwd, &path_args),
        "rm" => classify_remove_command(cwd, &path_args),
        _ => CommandSegmentSignal {
            kind: TerminalCommandKind::Command,
            path: None,
            from_path: None,
            paths: Vec::new(),
            source_paths: Vec::new(),
            target_paths: Vec::new(),
        },
    }
}

fn aggregate_segment_signals(segments: Vec<CommandSegmentSignal>) -> TerminalSignal {
    let mut paths = Vec::new();
    let mut source_paths = Vec::new();
    let mut target_paths = Vec::new();
    let mut command_kinds = Vec::with_capacity(segments.len());
    let mut directory_structures = Vec::new();

    for segment in &segments {
        command_kinds.push(command_kind_name(&segment.kind).to_string());
        extend_unique(&mut paths, segment.paths.iter().cloned());
        extend_unique(&mut source_paths, segment.source_paths.iter().cloned());
        extend_unique(&mut target_paths, segment.target_paths.iter().cloned());
    }

    for path in source_paths
        .iter()
        .chain(target_paths.iter())
        .chain(paths.iter())
    {
        push_unique(&mut directory_structures, directory_structure(path));
    }

    let primary = segments
        .iter()
        .rev()
        .find(|segment| segment.kind != TerminalCommandKind::Command)
        .or_else(|| segments.last());

    TerminalSignal {
        kind: primary
            .map(|segment| segment.kind.clone())
            .unwrap_or(TerminalCommandKind::Command),
        path: primary.and_then(|segment| segment.path.clone()),
        from_path: primary.and_then(|segment| segment.from_path.clone()),
        path_count: paths.len(),
        paths,
        source_paths,
        target_paths,
        command_sequence_pattern: command_kinds.join(">"),
        command_kinds,
        directory_structures,
        command_count: segments.len(),
    }
}

fn classify_move_command(cwd: &str, path_args: &[String]) -> CommandSegmentSignal {
    if path_args.len() != 2 {
        return CommandSegmentSignal {
            kind: TerminalCommandKind::Command,
            path: None,
            from_path: None,
            paths: Vec::new(),
            source_paths: Vec::new(),
            target_paths: Vec::new(),
        };
    }

    let from_path = resolve_path(cwd, &path_args[0]);
    let to_path = resolve_path(cwd, &path_args[1]);
    let kind = if same_parent(&from_path, &to_path) {
        TerminalCommandKind::Rename
    } else {
        TerminalCommandKind::Move
    };

    CommandSegmentSignal {
        kind,
        path: Some(to_path.clone()),
        from_path: Some(from_path.clone()),
        paths: vec![from_path.clone(), to_path.clone()],
        source_paths: vec![from_path],
        target_paths: vec![to_path],
    }
}

fn classify_copy_command(cwd: &str, path_args: &[String]) -> CommandSegmentSignal {
    if path_args.len() != 2 {
        return CommandSegmentSignal {
            kind: TerminalCommandKind::Command,
            path: None,
            from_path: None,
            paths: Vec::new(),
            source_paths: Vec::new(),
            target_paths: Vec::new(),
        };
    }

    let from_path = resolve_path(cwd, &path_args[0]);
    let to_path = resolve_path(cwd, &path_args[1]);

    CommandSegmentSignal {
        kind: TerminalCommandKind::Copy,
        path: Some(to_path.clone()),
        from_path: Some(from_path.clone()),
        paths: vec![from_path.clone(), to_path.clone()],
        source_paths: vec![from_path],
        target_paths: vec![to_path],
    }
}

fn classify_mkdir_command(cwd: &str, path_args: &[String]) -> CommandSegmentSignal {
    let paths: Vec<_> = path_args.iter().map(|arg| resolve_path(cwd, arg)).collect();
    CommandSegmentSignal {
        kind: TerminalCommandKind::Mkdir,
        path: paths.first().cloned(),
        from_path: None,
        source_paths: Vec::new(),
        target_paths: paths.clone(),
        paths,
    }
}

fn classify_remove_command(cwd: &str, path_args: &[String]) -> CommandSegmentSignal {
    let paths: Vec<_> = path_args.iter().map(|arg| resolve_path(cwd, arg)).collect();
    CommandSegmentSignal {
        kind: TerminalCommandKind::Remove,
        path: paths.first().cloned(),
        from_path: None,
        source_paths: paths.clone(),
        target_paths: Vec::new(),
        paths,
    }
}

fn collect_path_args(command_name: &str, args: &[String]) -> Vec<String> {
    args.iter()
        .filter(|arg| !is_option_token(command_name, arg))
        .cloned()
        .collect()
}

fn is_option_token(command_name: &str, token: &str) -> bool {
    if token == "--" {
        return false;
    }

    if let Some(stripped) = token.strip_prefix("--") {
        return !stripped.is_empty();
    }

    if let Some(stripped) = token.strip_prefix('-') {
        if stripped.is_empty() {
            return false;
        }

        if command_name == "rm" && stripped.chars().all(|ch| ch.is_ascii_digit()) {
            return false;
        }

        return !looks_like_path(token);
    }

    false
}

fn redact_tokens(tokens: &[String]) -> String {
    let mut redacted = Vec::with_capacity(tokens.len());

    for (index, token) in tokens.iter().enumerate() {
        if index == 0 {
            redacted.push(token.clone());
        } else if is_sensitive_assignment(token) || looks_like_secret_value(token) {
            redacted.push("<redacted>".to_string());
        } else if is_option_token(tokens[0].as_str(), token) {
            redacted.push(token.clone());
        } else if looks_like_path(token) {
            redacted.push("<path>".to_string());
        } else {
            redacted.push("<arg>".to_string());
        }
    }

    redacted.join(" ")
}

fn split_command_segments(command: &str) -> Result<Vec<String>, TerminalHistoryError> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut chars = command.chars().peekable();
    let mut quote: Option<char> = None;

    while let Some(ch) = chars.next() {
        match quote {
            Some(active_quote) => {
                if ch == active_quote {
                    quote = None;
                } else if ch == '\\' && active_quote == '"' {
                    let escaped = chars.next().ok_or_else(|| {
                        TerminalHistoryError::InvalidCommand(
                            "unfinished escape sequence".to_string(),
                        )
                    })?;
                    current.push(ch);
                    current.push(escaped);
                } else {
                    current.push(ch);
                }
            }
            None => match ch {
                '\'' | '"' => {
                    quote = Some(ch);
                    current.push(ch);
                }
                '\\' => {
                    let escaped = chars.next().ok_or_else(|| {
                        TerminalHistoryError::InvalidCommand(
                            "unfinished escape sequence".to_string(),
                        )
                    })?;
                    current.push(ch);
                    current.push(escaped);
                }
                ';' => {
                    push_segment(&mut segments, &mut current);
                }
                '&' | '|' => {
                    if chars.peek() == Some(&ch) {
                        chars.next();
                        push_segment(&mut segments, &mut current);
                    } else {
                        current.push(ch);
                    }
                }
                _ => current.push(ch),
            },
        }
    }

    if quote.is_some() {
        return Err(TerminalHistoryError::InvalidCommand(
            "unterminated quoted string".to_string(),
        ));
    }

    push_segment(&mut segments, &mut current);
    if segments.is_empty() {
        return Err(TerminalHistoryError::InvalidCommand(
            "empty command".to_string(),
        ));
    }
    Ok(segments)
}

fn push_segment(segments: &mut Vec<String>, current: &mut String) {
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        segments.push(trimmed.to_string());
    }
    current.clear();
}

fn command_kind_name(kind: &TerminalCommandKind) -> &'static str {
    match kind {
        TerminalCommandKind::Command => "command",
        TerminalCommandKind::Copy => "copy",
        TerminalCommandKind::Mkdir => "mkdir",
        TerminalCommandKind::Move => "move",
        TerminalCommandKind::Remove => "remove",
        TerminalCommandKind::Rename => "rename",
    }
}

fn extend_unique(values: &mut Vec<String>, iter: impl IntoIterator<Item = String>) {
    for value in iter {
        push_unique(values, value);
    }
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !value.is_empty() && !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn directory_structure(path: &str) -> String {
    let Some(parent) = Path::new(path).parent() else {
        return ".".to_string();
    };
    let mut components = Vec::new();
    for component in parent.components() {
        if let Component::Normal(value) = component {
            components.push(normalize_structure_component(&value.to_string_lossy()));
        }
    }

    if components.is_empty() {
        ".".to_string()
    } else {
        components.join("/")
    }
}

fn normalize_structure_component(component: &str) -> String {
    let lowered = component.to_ascii_lowercase();
    if lowered.chars().all(|ch| ch.is_ascii_digit()) {
        "#".to_string()
    } else if lowered
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        lowered
    } else {
        "*".to_string()
    }
}

fn is_sensitive_assignment(token: &str) -> bool {
    let Some((name, _)) = token.split_once('=') else {
        return false;
    };
    let lowered = name.to_ascii_lowercase();
    [
        "token",
        "secret",
        "password",
        "passwd",
        "key",
        "auth",
        "credential",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

fn looks_like_secret_value(token: &str) -> bool {
    token.len() >= 24
        && token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '='))
}

fn looks_like_path(token: &str) -> bool {
    token.starts_with('/')
        || token.starts_with("./")
        || token.starts_with("../")
        || token.starts_with("~/")
        || token.contains('/')
        || token.contains('.')
}

fn same_parent(from_path: &str, to_path: &str) -> bool {
    Path::new(from_path).parent() == Path::new(to_path).parent()
}

fn resolve_path(cwd: &str, raw_path: &str) -> String {
    if raw_path.starts_with("~/") {
        return raw_path.to_string();
    }

    let joined = if Path::new(raw_path).is_absolute() {
        PathBuf::from(raw_path)
    } else {
        Path::new(cwd).join(raw_path)
    };

    normalize_path(joined)
}

fn normalize_path(path: PathBuf) -> String {
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
        ".".to_string()
    } else {
        normalized.display().to_string()
    }
}

fn tokenize_command_line(command: &str) -> Result<Vec<String>, TerminalHistoryError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = command.chars().peekable();
    let mut quote: Option<char> = None;

    while let Some(ch) = chars.next() {
        match quote {
            Some(active_quote) => {
                if ch == active_quote {
                    quote = None;
                } else if ch == '\\' && active_quote == '"' {
                    let escaped = chars.next().ok_or_else(|| {
                        TerminalHistoryError::InvalidCommand(
                            "unfinished escape sequence".to_string(),
                        )
                    })?;
                    current.push(escaped);
                } else {
                    current.push(ch);
                }
            }
            None => match ch {
                '\'' | '"' => quote = Some(ch),
                '\\' => {
                    let escaped = chars.next().ok_or_else(|| {
                        TerminalHistoryError::InvalidCommand(
                            "unfinished escape sequence".to_string(),
                        )
                    })?;
                    current.push(escaped);
                }
                ch if ch.is_whitespace() => {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                }
                _ => current.push(ch),
            },
        }
    }

    if quote.is_some() {
        return Err(TerminalHistoryError::InvalidCommand(
            "unterminated quoted string".to_string(),
        ));
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    if tokens.is_empty() {
        return Err(TerminalHistoryError::InvalidCommand(
            "empty command".to_string(),
        ));
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_record(command: &str) -> TerminalHistoryRecord {
        TerminalHistoryRecord {
            ts: Utc.with_ymd_and_hms(2026, 3, 11, 9, 0, 0).unwrap(),
            cwd: "/tmp/workspace".to_string(),
            command: command.to_string(),
            exit_code: Some(0),
            shell: Some("zsh".to_string()),
        }
    }

    #[test]
    fn parses_terminal_history_json_into_raw_event() {
        let raw = history_line_to_raw_event(
            r#"{"ts":"2026-03-11T09:00:00Z","cwd":"/tmp/workspace","command":"mv inbox/report.txt archive/report.txt","exit_code":0,"shell":"zsh"}"#,
        )
        .unwrap();

        assert_eq!(raw.source, EventSource::Terminal);
        assert_eq!(raw.payload["kind"], "move");
        assert_eq!(raw.payload["command_name"], "mv");
        assert_eq!(raw.payload["from_path"], "/tmp/workspace/inbox/report.txt");
        assert_eq!(raw.payload["path"], "/tmp/workspace/archive/report.txt");
        assert_eq!(raw.payload["redacted_command"], "mv <path> <path>");
    }

    #[test]
    fn redacts_sensitive_assignments() {
        let raw = record_to_raw_event(&sample_record(
            "curl API_TOKEN=supersecretvalue https://example.test",
        ))
        .unwrap();

        assert_eq!(raw.payload["kind"], "command");
        assert_eq!(raw.payload["redacted_command"], "curl <redacted> <path>");
    }

    #[test]
    fn classifies_rename_when_parent_directory_is_unchanged() {
        let raw = record_to_raw_event(&sample_record("mv draft.txt report.txt")).unwrap();

        assert_eq!(raw.payload["kind"], "rename");
        assert_eq!(raw.payload["from_path"], "/tmp/workspace/draft.txt");
        assert_eq!(raw.payload["path"], "/tmp/workspace/report.txt");
    }

    #[test]
    fn keeps_failed_file_commands_as_observation_only_commands() {
        let mut record = sample_record("cp draft.txt archive/report.txt");
        record.exit_code = Some(1);

        let raw = record_to_raw_event(&record).unwrap();

        assert_eq!(raw.payload["kind"], "copy");
        assert_eq!(raw.payload["succeeded"], false);
    }

    #[test]
    fn rejects_unterminated_quotes() {
        let error = record_to_raw_event(&sample_record("mv \"draft.txt report.txt")).unwrap_err();

        assert!(matches!(error, TerminalHistoryError::InvalidCommand(_)));
    }

    #[test]
    fn captures_remove_metadata_without_raw_arguments() {
        let raw =
            record_to_raw_event(&sample_record("rm -rf secrets.txt build/output.log")).unwrap();

        assert_eq!(raw.payload["kind"], "remove");
        assert_eq!(raw.payload["destructive"], true);
        assert_eq!(raw.payload["path_count"], 2);
        assert_eq!(raw.payload["redacted_command"], "rm -rf <path> <path>");
    }

    #[test]
    fn captures_chained_directory_preparation_and_copy_workflow() {
        let raw = record_to_raw_event(&sample_record(
            "mkdir -p review/2026/03 && cp inbox/report.txt review/2026/03/report.txt",
        ))
        .unwrap();

        assert_eq!(raw.payload["kind"], "copy");
        assert_eq!(raw.payload["command_sequence_pattern"], "mkdir>copy");
        assert_eq!(raw.payload["command_count"], 2);
        assert_eq!(
            raw.payload["source_paths"][0],
            "/tmp/workspace/inbox/report.txt"
        );
        assert_eq!(
            raw.payload["target_paths"][0],
            "/tmp/workspace/review/2026/03"
        );
        assert_eq!(
            raw.payload["target_paths"][1],
            "/tmp/workspace/review/2026/03/report.txt"
        );
        assert!(raw.payload["directory_structures"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value.as_str() == Some("tmp/workspace/review/#/#")));
    }

    #[test]
    fn keeps_chained_move_sequences_stable_for_pattern_detection() {
        let raw = record_to_raw_event(&sample_record(
            "mkdir -p archive/reports; mv inbox/draft.txt archive/reports/report.txt",
        ))
        .unwrap();

        assert_eq!(raw.payload["kind"], "move");
        assert_eq!(raw.payload["command_sequence_pattern"], "mkdir>move");
        assert_eq!(
            raw.payload["command_kinds"],
            serde_json::json!(["mkdir", "move"])
        );
        assert_eq!(
            raw.payload["path"],
            "/tmp/workspace/archive/reports/report.txt"
        );
        assert_eq!(raw.payload["from_path"], "/tmp/workspace/inbox/draft.txt");
    }
}
