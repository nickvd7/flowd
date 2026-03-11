use chrono::{DateTime, Utc};
use flow_core::events::{EventSource, RawEvent};
use notify::{
    event::{CreateKind, ModifyKind, RenameMode},
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
#[cfg(test)]
use notify::{Config as NotifyConfig, PollWatcher};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};
use thiserror::Error;

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

#[derive(Debug, Error)]
pub enum FileWatcherError {
    #[error("notify watcher error: {0}")]
    Notify(#[from] notify::Error),
    #[error("file watcher channel closed")]
    ChannelClosed,
    #[error("timed out waiting for file event")]
    Timeout,
}

pub struct FileWatcherAdapter {
    _watcher: WatcherHandle,
    rx: Receiver<notify::Result<Event>>,
    pending_rename_from: Option<PathBuf>,
}

#[allow(dead_code)]
enum WatcherHandle {
    Recommended(RecommendedWatcher),
    #[cfg(test)]
    Poll(PollWatcher),
}

impl FileWatcherAdapter {
    pub fn watch_paths(paths: &[PathBuf]) -> Result<Self, FileWatcherError> {
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |result| {
            let _ = tx.send(result);
        })?;

        for path in paths {
            watcher.watch(path, RecursiveMode::Recursive)?;
        }

        Ok(Self {
            _watcher: WatcherHandle::Recommended(watcher),
            rx,
            pending_rename_from: None,
        })
    }

    #[cfg(test)]
    fn watch_paths_polling(
        paths: &[PathBuf],
        interval: Duration,
    ) -> Result<Self, FileWatcherError> {
        let (tx, rx) = mpsc::channel();
        let mut watcher = PollWatcher::new(
            move |result| {
                let _ = tx.send(result);
            },
            NotifyConfig::default().with_poll_interval(interval),
        )?;

        for path in paths {
            watcher.watch(path, RecursiveMode::Recursive)?;
        }

        Ok(Self {
            _watcher: WatcherHandle::Poll(watcher),
            rx,
            pending_rename_from: None,
        })
    }

    pub fn next_event(&mut self) -> Result<FileEvent, FileWatcherError> {
        loop {
            let result = self
                .rx
                .recv()
                .map_err(|_| FileWatcherError::ChannelClosed)?;
            if let Some(event) = convert_notify_result(result, &mut self.pending_rename_from)? {
                return Ok(event);
            }
        }
    }

    pub fn next_event_with_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<FileEvent>, FileWatcherError> {
        let deadline = Instant::now() + timeout;

        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(FileWatcherError::Timeout)?;
            let result = self
                .rx
                .recv_timeout(remaining)
                .map_err(|error| match error {
                    mpsc::RecvTimeoutError::Timeout => FileWatcherError::Timeout,
                    mpsc::RecvTimeoutError::Disconnected => FileWatcherError::ChannelClosed,
                })?;
            if let Some(event) = convert_notify_result(result, &mut self.pending_rename_from)? {
                return Ok(Some(event));
            }
        }
    }
}

fn convert_notify_result(
    result: notify::Result<Event>,
    pending_rename_from: &mut Option<PathBuf>,
) -> Result<Option<FileEvent>, FileWatcherError> {
    let event = result?;
    Ok(file_event_from_notify_event(event, pending_rename_from))
}

fn file_event_from_notify_event(
    event: Event,
    pending_rename_from: &mut Option<PathBuf>,
) -> Option<FileEvent> {
    match event.kind {
        EventKind::Create(CreateKind::Any | CreateKind::File) => {
            build_create_event(event.paths.into_iter().next()?)
        }
        EventKind::Modify(ModifyKind::Name(mode)) => {
            rename_event_from_paths(event.paths, mode, pending_rename_from)
        }
        _ => None,
    }
}

fn build_create_event(path: PathBuf) -> Option<FileEvent> {
    if should_ignore_path(&path) || is_directory_path(&path) {
        return None;
    }

    Some(FileEvent {
        ts: Utc::now(),
        kind: FileEventKind::Create,
        path: path.display().to_string(),
        from_path: None,
    })
}

fn rename_event_from_paths(
    mut paths: Vec<PathBuf>,
    mode: RenameMode,
    pending_rename_from: &mut Option<PathBuf>,
) -> Option<FileEvent> {
    let pair = match mode {
        RenameMode::From => {
            *pending_rename_from = paths.into_iter().next();
            return None;
        }
        RenameMode::To => {
            let to = paths.into_iter().next()?;
            let from = pending_rename_from.take()?;
            (from, to)
        }
        RenameMode::Both | RenameMode::Any => {
            let to = paths.pop()?;
            let from = paths.pop()?;
            pending_rename_from.take();
            (from, to)
        }
        _ => return None,
    };

    let (from_path, to_path) = pair;
    if should_ignore_path(&from_path) || should_ignore_path(&to_path) || is_directory_path(&to_path)
    {
        return None;
    }

    let kind = if from_path.parent() == to_path.parent() {
        FileEventKind::Rename
    } else {
        FileEventKind::Move
    };

    Some(FileEvent {
        ts: Utc::now(),
        kind,
        path: to_path.display().to_string(),
        from_path: Some(from_path.display().to_string()),
    })
}

fn is_directory_path(path: &Path) -> bool {
    path.is_dir()
}

fn should_ignore_path(path: &Path) -> bool {
    if path
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .any(|component| component.starts_with('.'))
    {
        return true;
    }

    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return true;
    };

    name.starts_with("~$")
        || name.ends_with('~')
        || [
            ".tmp",
            ".temp",
            ".part",
            ".crdownload",
            ".download",
            ".swp",
            ".swx",
        ]
        .iter()
        .any(|suffix| name.ends_with(suffix))
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
        event::{CreateKind, ModifyKind, RenameMode},
        Event,
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
    fn converts_notify_create_rename_and_move_events() {
        let mut pending = None;

        let create = file_event_from_notify_event(
            Event {
                kind: EventKind::Create(CreateKind::File),
                paths: vec![PathBuf::from("/tmp/invoice.pdf")],
                attrs: Default::default(),
            },
            &mut pending,
        )
        .unwrap();
        assert_eq!(create.kind, FileEventKind::Create);

        let rename = file_event_from_notify_event(
            Event {
                kind: EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
                paths: vec![
                    PathBuf::from("/tmp/invoice.pdf"),
                    PathBuf::from("/tmp/invoice-reviewed.pdf"),
                ],
                attrs: Default::default(),
            },
            &mut pending,
        )
        .unwrap();
        assert_eq!(rename.kind, FileEventKind::Rename);

        let movement = file_event_from_notify_event(
            Event {
                kind: EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
                paths: vec![
                    PathBuf::from("/tmp/invoice-reviewed.pdf"),
                    PathBuf::from("/tmp/archive/invoice-reviewed.pdf"),
                ],
                attrs: Default::default(),
            },
            &mut pending,
        )
        .unwrap();
        assert_eq!(movement.kind, FileEventKind::Move);
    }

    #[test]
    fn ignores_hidden_temp_and_directory_events() {
        let dir = tempdir().unwrap();
        let folder = dir.path().join("folder");
        let hidden_folder = dir.path().join(".hidden-folder");
        fs::create_dir_all(&folder).unwrap();
        fs::create_dir_all(&hidden_folder).unwrap();
        let mut pending = None;

        let hidden = file_event_from_notify_event(
            Event {
                kind: EventKind::Create(CreateKind::File),
                paths: vec![dir.path().join(".hidden")],
                attrs: Default::default(),
            },
            &mut pending,
        );
        assert!(hidden.is_none());

        let temporary = file_event_from_notify_event(
            Event {
                kind: EventKind::Create(CreateKind::File),
                paths: vec![dir.path().join("draft.tmp")],
                attrs: Default::default(),
            },
            &mut pending,
        );
        assert!(temporary.is_none());

        let directory = file_event_from_notify_event(
            Event {
                kind: EventKind::Create(CreateKind::File),
                paths: vec![folder],
                attrs: Default::default(),
            },
            &mut pending,
        );
        assert!(directory.is_none());

        let nested_hidden = file_event_from_notify_event(
            Event {
                kind: EventKind::Create(CreateKind::File),
                paths: vec![hidden_folder.join("report.txt")],
                attrs: Default::default(),
            },
            &mut pending,
        );
        assert!(nested_hidden.is_none());
    }

    #[test]
    fn combines_split_rename_events_into_one_file_event() {
        let mut pending = None;

        let rename_from = file_event_from_notify_event(
            Event {
                kind: EventKind::Modify(ModifyKind::Name(RenameMode::From)),
                paths: vec![PathBuf::from("/tmp/invoice.pdf")],
                attrs: Default::default(),
            },
            &mut pending,
        );
        assert!(rename_from.is_none());

        let rename_to = file_event_from_notify_event(
            Event {
                kind: EventKind::Modify(ModifyKind::Name(RenameMode::To)),
                paths: vec![PathBuf::from("/tmp/invoice-reviewed.pdf")],
                attrs: Default::default(),
            },
            &mut pending,
        )
        .unwrap();

        assert_eq!(rename_to.kind, FileEventKind::Rename);
        assert_eq!(rename_to.path, "/tmp/invoice-reviewed.pdf");
        assert_eq!(rename_to.from_path.as_deref(), Some("/tmp/invoice.pdf"));
    }

    #[test]
    fn starts_watcher_for_temp_directory() {
        let dir = tempdir().unwrap();
        let watcher = FileWatcherAdapter::watch_paths_polling(
            &[dir.path().to_path_buf()],
            Duration::from_millis(100),
        );

        assert!(watcher.is_ok());
    }
}
