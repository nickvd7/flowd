use flow_core::events::NormalizedEvent;

pub fn split_into_sessions(events: &[NormalizedEvent], inactivity_secs: i64) -> Vec<Vec<NormalizedEvent>> {
    if events.is_empty() {
        return Vec::new();
    }

    let mut sessions: Vec<Vec<NormalizedEvent>> = Vec::new();
    let mut current: Vec<NormalizedEvent> = vec![events[0].clone()];

    for pair in events.windows(2) {
        let prev = &pair[0];
        let next = &pair[1];
        let gap = next.ts.signed_duration_since(prev.ts).num_seconds();

        if gap > inactivity_secs {
            sessions.push(current);
            current = vec![next.clone()];
        } else {
            current.push(next.clone());
        }
    }

    sessions.push(current);
    sessions
}
