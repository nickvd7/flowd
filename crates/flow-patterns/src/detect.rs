use crate::sessions::EventSession;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct PatternCandidate {
    pub signature: String,
    pub count: usize,
    pub avg_duration_ms: i64,
    pub canonical_summary: String,
    pub proposal_text: String,
}

pub fn detect_repeated_patterns(sessions: &[EventSession]) -> Vec<PatternCandidate> {
    let mut counts: HashMap<String, (usize, i64, String, String)> = HashMap::new();

    for session in sessions {
        let signature = session
            .events
            .iter()
            .map(event_signature_part)
            .collect::<Vec<_>>()
            .join("->");
        let duration_ms = session
            .end_ts
            .signed_duration_since(session.start_ts)
            .num_milliseconds();
        let summary = session_summary(session);
        let proposal_text = format!(
            "Repeated {} file workflow detected: {}",
            primary_group(session),
            summary
        );
        let entry =
            counts
                .entry(signature)
                .or_insert((0, 0, summary.clone(), proposal_text.clone()));
        entry.0 += 1;
        entry.1 += duration_ms;
    }

    let mut patterns: Vec<PatternCandidate> = counts
        .into_iter()
        .filter(|(_, (count, _, _, _))| *count > 1)
        .map(
            |(signature, (count, total_duration_ms, canonical_summary, proposal_text))| {
                PatternCandidate {
                    signature,
                    count,
                    avg_duration_ms: total_duration_ms / count as i64,
                    canonical_summary,
                    proposal_text,
                }
            },
        )
        .collect();
    patterns.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.signature.cmp(&right.signature))
    });
    patterns
}

fn event_signature_part(event: &flow_core::events::NormalizedEvent) -> String {
    let group = event
        .metadata
        .get("file_group")
        .and_then(|value| value.as_str())
        .unwrap_or("file");
    format!("{:?}:{group}", event.action_type)
}

fn session_summary(session: &EventSession) -> String {
    session
        .events
        .iter()
        .map(|event| format!("{:?}", event.action_type))
        .collect::<Vec<_>>()
        .join(" -> ")
}

fn primary_group(session: &EventSession) -> &str {
    session
        .events
        .first()
        .and_then(|event| event.metadata.get("file_group"))
        .and_then(|value| value.as_str())
        .unwrap_or("file")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{normalize::normalize, sessions::split_into_sessions};
    use chrono::{TimeZone, Utc};
    use flow_adapters::file_watcher::{synthetic_file_event, FileEventKind};

    #[test]
    fn detects_repeated_invoice_pattern() {
        let raw_events = vec![
            synthetic_file_event(
                Utc.with_ymd_and_hms(2026, 1, 15, 9, 0, 0).unwrap(),
                FileEventKind::Create,
                "/tmp/inbox/invoice-1001.pdf",
                None,
            ),
            synthetic_file_event(
                Utc.with_ymd_and_hms(2026, 1, 15, 9, 0, 20).unwrap(),
                FileEventKind::Rename,
                "/tmp/inbox/invoice-1001-reviewed.pdf",
                Some("/tmp/inbox/invoice-1001.pdf".to_string()),
            ),
            synthetic_file_event(
                Utc.with_ymd_and_hms(2026, 1, 15, 9, 0, 40).unwrap(),
                FileEventKind::Move,
                "/tmp/archive/invoice-1001-reviewed.pdf",
                Some("/tmp/inbox/invoice-1001-reviewed.pdf".to_string()),
            ),
            synthetic_file_event(
                Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 0).unwrap(),
                FileEventKind::Create,
                "/tmp/inbox/invoice-1002.pdf",
                None,
            ),
            synthetic_file_event(
                Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 20).unwrap(),
                FileEventKind::Rename,
                "/tmp/inbox/invoice-1002-reviewed.pdf",
                Some("/tmp/inbox/invoice-1002.pdf".to_string()),
            ),
            synthetic_file_event(
                Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 40).unwrap(),
                FileEventKind::Move,
                "/tmp/archive/invoice-1002-reviewed.pdf",
                Some("/tmp/inbox/invoice-1002-reviewed.pdf".to_string()),
            ),
        ];

        let normalized: Vec<_> = raw_events.iter().filter_map(normalize).collect();
        let sessions = split_into_sessions(&normalized, 300);
        let patterns = detect_repeated_patterns(&sessions);

        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].count, 2);
        assert_eq!(
            patterns[0].signature,
            "CreateFile:invoice->RenameFile:invoice_reviewed->MoveFile:invoice_reviewed"
        );
        assert!(patterns[0].proposal_text.contains("invoice"));
    }
}
