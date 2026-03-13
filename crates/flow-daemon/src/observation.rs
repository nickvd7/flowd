use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use flow_adapters::file_watcher::FileEvent;
use flow_core::events::RawEvent;
use flow_db::repo::insert_raw_event;
use rusqlite::Connection;
use std::collections::VecDeque;

/// The observation layer only accepts adapter events, deduplicates them, and
/// persists raw events. Analysis and execution stay outside this module.
pub struct ObservationPipeline {
    deduper: RecentFileEventDeduper,
}

impl ObservationPipeline {
    pub fn new(window: Duration) -> Self {
        Self {
            deduper: RecentFileEventDeduper::new(window),
        }
    }

    pub fn accept(&mut self, conn: &Connection, file_event: FileEvent) -> Result<Option<RawEvent>> {
        if !self.deduper.should_emit(&file_event) {
            return Ok(None);
        }

        let raw_event = file_event.into_raw_event();
        self.accept_raw_event(conn, raw_event.clone())?;
        Ok(Some(raw_event))
    }

    pub fn accept_raw_event(&self, conn: &Connection, raw_event: RawEvent) -> Result<()> {
        insert_raw_event(conn, &raw_event).context("failed to insert raw event")?;
        Ok(())
    }
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
pub struct RecentFileEventDeduper {
    window: Duration,
    recent_events: VecDeque<RecentFileEvent>,
}

impl RecentFileEventDeduper {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            recent_events: VecDeque::new(),
        }
    }

    pub fn should_emit(&mut self, event: &FileEvent) -> bool {
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
