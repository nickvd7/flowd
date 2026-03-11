use flow_core::events::NormalizedEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventSession {
    pub start_ts: chrono::DateTime<chrono::Utc>,
    pub end_ts: chrono::DateTime<chrono::Utc>,
    pub events: Vec<NormalizedEvent>,
}

pub fn split_into_sessions(events: &[NormalizedEvent], inactivity_secs: i64) -> Vec<EventSession> {
    if events.is_empty() || inactivity_secs < 0 {
        return Vec::new();
    }

    let mut ordered = events.to_vec();
    ordered.sort_by_key(|event| event.ts);

    let mut sessions: Vec<EventSession> = Vec::new();
    let mut current: Vec<NormalizedEvent> = vec![ordered[0].clone()];

    for pair in ordered.windows(2) {
        let prev = &pair[0];
        let next = &pair[1];
        let gap = next.ts.signed_duration_since(prev.ts).num_seconds();

        if gap > inactivity_secs {
            sessions.push(build_session(current));
            current = vec![next.clone()];
        } else {
            current.push(next.clone());
        }
    }

    sessions.push(build_session(current));
    sessions
}

fn build_session(events: Vec<NormalizedEvent>) -> EventSession {
    EventSession {
        start_ts: events.first().map(|event| event.ts).unwrap(),
        end_ts: events.last().map(|event| event.ts).unwrap(),
        events,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use flow_core::events::{ActionType, NormalizedEvent};
    use serde_json::json;

    fn event(second: u32) -> NormalizedEvent {
        NormalizedEvent {
            ts: Utc.with_ymd_and_hms(2026, 1, 15, 9, 0, second).unwrap(),
            action_type: ActionType::CreateFile,
            app: None,
            target: Some(format!("/tmp/{second}.txt")),
            metadata: json!({}),
        }
    }

    #[test]
    fn splits_sorted_events_into_sessions() {
        let sessions = split_into_sessions(&[event(50), event(0), event(10)], 20);

        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].events.len(), 2);
        assert_eq!(sessions[1].events.len(), 1);
        assert_eq!(
            sessions[0].start_ts,
            Utc.with_ymd_and_hms(2026, 1, 15, 9, 0, 0).unwrap()
        );
    }
}
