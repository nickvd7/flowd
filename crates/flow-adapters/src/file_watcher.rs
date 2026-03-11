use chrono::{DateTime, Utc};
use flow_core::events::{EventSource, RawEvent};
use notify::{
    event::{CreateKind, DataChange, ModifyKind, RenameMode},
    Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileEventKind {
    Create,
    Rename,
    Move,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEvent {
    pub ts: DateTime<Utc>,
    pub kind: FileEventKind,
    pub path: String,
    #[serde(default)]
    pub from_path: Option<String>,
}

impl FileEvent {
    pub fn into_raw_event(self) -> RawEvent {
        RawEvent {
            ts: self.ts,
            source: EventSource::FileWatcher,
            payload: json!({
                "kind": self.kind,
                "path": self.path,
                "from_path": self.from_path,
            }),
        }
    }
}

pub fn notify_channel() -> notify::Result<(RecommendedWatcher, Receiver<notify::Result<Event>>)> {
    let (tx, rx) = mpsc::channel();
    let watcher = RecommendedWatcher::new(
        move |result| {
            let _ = tx.send(result);
        },
        Config::default(),
    )?;

    Ok((watcher, rx))
}

pub fn watch_path(watcher: &mut RecommendedWatcher, path: &Path) -> notify::Result<()> {
    watcher.watch(path, RecursiveMode::Recursive)
}

pub fn event_to_file_events(event: &Event) -> Vec<FileEvent> {
    let ts = Utc::now();

    match &event.kind {
        kind if is_create_like_kind(kind) => create_events_from_paths(ts, &event.paths),
        EventKind::Modify(ModifyKind::Name(rename_mode)) => {
            rename_event_to_file_event(ts, rename_mode, &event.paths)
                .into_iter()
                .collect()
        }
        _ => Vec::new(),
    }
}

fn create_events_from_paths(ts: DateTime<Utc>, paths: &[PathBuf]) -> Vec<FileEvent> {
    paths
        .iter()
        .filter(|path| is_file_like_path(path))
        .filter(|path| !should_ignore_path(path))
        .map(|path| FileEvent {
            ts,
            kind: FileEventKind::Create,
            path: path.display().to_string(),
            from_path: None,
        })
        .collect()
}

fn is_create_like_kind(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(CreateKind::File)
            | EventKind::Create(CreateKind::Any)
            | EventKind::Modify(ModifyKind::Any)
            | EventKind::Modify(ModifyKind::Data(DataChange::Any))
            | EventKind::Modify(ModifyKind::Data(DataChange::Content))
            | EventKind::Modify(ModifyKind::Data(DataChange::Size))
            | EventKind::Modify(ModifyKind::Data(DataChange::Other))
    )
}

fn rename_event_to_file_event(
    ts: DateTime<Utc>,
    rename_mode: &RenameMode,
    paths: &[PathBuf],
) -> Option<FileEvent> {
    if !matches!(
        rename_mode,
        RenameMode::Any | RenameMode::Both | RenameMode::From | RenameMode::To | RenameMode::Other
    ) {
        return None;
    }

    match paths {
        [from_path, to_path] => {
            if !is_file_like_path(to_path)
                || should_ignore_path(from_path)
                || should_ignore_path(to_path)
            {
                return None;
            }

            let kind = if same_parent(from_path, to_path) {
                FileEventKind::Rename
            } else {
                FileEventKind::Move
            };

            Some(FileEvent {
                ts,
                kind,
                path: to_path.display().to_string(),
                from_path: Some(from_path.display().to_string()),
            })
        }
        [path]
            if matches!(
                rename_mode,
                RenameMode::Any | RenameMode::To | RenameMode::Other
            ) =>
        {
            if !is_file_like_path(path) || should_ignore_path(path) {
                return None;
            }

            Some(FileEvent {
                ts,
                kind: FileEventKind::Rename,
                path: path.display().to_string(),
                from_path: None,
            })
        }
        _ => None,
    }
}

fn same_parent(from_path: &Path, to_path: &Path) -> bool {
    from_path.parent() == to_path.parent()
}

fn is_file_like_path(path: &Path) -> bool {
    match path.metadata() {
        Ok(metadata) => !metadata.is_dir(),
        Err(_) => true,
    }
}

fn should_ignore_path(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return true;
    };

    is_hidden_file_name(name) || is_temporary_file_name(name)
}

fn is_hidden_file_name(name: &str) -> bool {
    name.starts_with('.')
}

fn is_temporary_file_name(name: &str) -> bool {
    const TEMP_SUFFIXES: [&str; 6] = [".tmp", ".temp", ".swp", ".swx", ".part", "~"];
    TEMP_SUFFIXES.iter().any(|suffix| name.ends_with(suffix))
}

pub fn synthetic_create_event(path: &Path) -> RawEvent {
    FileEvent {
        ts: Utc::now(),
        kind: FileEventKind::Create,
        path: path.display().to_string(),
        from_path: None,
    }
    .into_raw_event()
}

pub fn synthetic_file_event(
    ts: DateTime<Utc>,
    kind: FileEventKind,
    path: impl Into<String>,
    from_path: Option<String>,
) -> RawEvent {
    FileEvent {
        ts,
        kind,
        path: path.into(),
        from_path,
    }
    .into_raw_event()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use notify::{
        event::{DataChange, ModifyKind, RenameMode},
        Event, EventKind,
    };
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn file_event_serializes_into_raw_event_payload() {
        let raw = synthetic_file_event(
            Utc.with_ymd_and_hms(2026, 1, 15, 9, 0, 0).unwrap(),
            FileEventKind::Rename,
            "/tmp/invoice-1001-reviewed.pdf",
            Some("/tmp/invoice-1001.pdf".to_string()),
        );

        assert_eq!(raw.source, EventSource::FileWatcher);
        assert_eq!(raw.payload["kind"], "rename");
        assert_eq!(raw.payload["path"], "/tmp/invoice-1001-reviewed.pdf");
        assert_eq!(raw.payload["from_path"], "/tmp/invoice-1001.pdf");
    }

    #[test]
    fn converts_create_event_for_visible_files() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("report.txt");
        fs::write(&path, "ok").unwrap();
        let event = Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![path.clone()],
            attrs: Default::default(),
        };

        let file_events = event_to_file_events(&event);

        assert_eq!(file_events.len(), 1);
        assert_eq!(file_events[0].kind, FileEventKind::Create);
        assert_eq!(file_events[0].path, path.display().to_string());
    }

    #[test]
    fn converts_create_event_when_metadata_is_not_yet_available() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("report.txt");
        let event = Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![path.clone()],
            attrs: Default::default(),
        };

        let file_events = event_to_file_events(&event);

        assert_eq!(file_events.len(), 1);
        assert_eq!(file_events[0].kind, FileEventKind::Create);
        assert_eq!(file_events[0].path, path.display().to_string());
    }

    #[test]
    fn ignores_hidden_and_temporary_files() {
        let dir = tempdir().unwrap();
        let hidden_path = dir.path().join(".report.txt");
        let temp_path = dir.path().join("report.txt.tmp");
        fs::write(&hidden_path, "hidden").unwrap();
        fs::write(&temp_path, "temp").unwrap();

        let hidden_event = Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![hidden_path],
            attrs: Default::default(),
        };
        let temp_event = Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![temp_path],
            attrs: Default::default(),
        };

        assert!(event_to_file_events(&hidden_event).is_empty());
        assert!(event_to_file_events(&temp_event).is_empty());
    }

    #[test]
    fn converts_modify_data_event_for_visible_files() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("report.txt");
        let event = Event {
            kind: EventKind::Modify(ModifyKind::Data(DataChange::Content)),
            paths: vec![path.clone()],
            attrs: Default::default(),
        };

        let file_events = event_to_file_events(&event);

        assert_eq!(file_events.len(), 1);
        assert_eq!(file_events[0].kind, FileEventKind::Create);
        assert_eq!(file_events[0].path, path.display().to_string());
    }

    #[test]
    fn classifies_rename_with_same_parent() {
        let dir = tempdir().unwrap();
        let from_path = dir.path().join("report.txt");
        let to_path = dir.path().join("report-final.txt");
        fs::write(&to_path, "renamed").unwrap();

        let event = Event {
            kind: EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            paths: vec![from_path.clone(), to_path.clone()],
            attrs: Default::default(),
        };

        let file_events = event_to_file_events(&event);

        assert_eq!(file_events.len(), 1);
        assert_eq!(file_events[0].kind, FileEventKind::Rename);
        assert_eq!(
            file_events[0].from_path.as_deref(),
            Some(from_path.to_str().unwrap())
        );
        assert_eq!(file_events[0].path, to_path.display().to_string());
    }

    #[test]
    fn classifies_single_path_rename_event_as_rename() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("report-final.txt");
        let event = Event {
            kind: EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
            paths: vec![path.clone()],
            attrs: Default::default(),
        };

        let file_events = event_to_file_events(&event);

        assert_eq!(file_events.len(), 1);
        assert_eq!(file_events[0].kind, FileEventKind::Rename);
        assert_eq!(file_events[0].from_path, None);
        assert_eq!(file_events[0].path, path.display().to_string());
    }

    #[test]
    fn classifies_move_with_different_parent() {
        let dir = tempdir().unwrap();
        let archive_dir = dir.path().join("archive");
        fs::create_dir(&archive_dir).unwrap();
        let from_path = dir.path().join("report.txt");
        let to_path = archive_dir.join("report.txt");
        fs::write(&to_path, "moved").unwrap();

        let event = Event {
            kind: EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            paths: vec![from_path.clone(), to_path.clone()],
            attrs: Default::default(),
        };

        let file_events = event_to_file_events(&event);

        assert_eq!(file_events.len(), 1);
        assert_eq!(file_events[0].kind, FileEventKind::Move);
        assert_eq!(
            file_events[0].from_path.as_deref(),
            Some(from_path.to_str().unwrap())
        );
        assert_eq!(file_events[0].path, to_path.display().to_string());
    }
}
