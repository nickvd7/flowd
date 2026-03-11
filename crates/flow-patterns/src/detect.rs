use crate::sessions::EventSession;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct PatternCandidate {
    pub signature: String,
    pub count: usize,
    pub avg_duration_ms: i64,
    pub canonical_summary: String,
    pub proposal_text: String,
    pub last_seen_at: DateTime<Utc>,
    pub safety_score: f64,
    pub usefulness_score: f64,
}

pub fn detect_repeated_patterns(sessions: &[EventSession]) -> Vec<PatternCandidate> {
    let latest_observed_at = sessions.iter().map(|session| session.end_ts).max();
    let mut counts: HashMap<String, (usize, i64, String, String, DateTime<Utc>, f64)> =
        HashMap::new();

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
        let safety_score = action_safety_score(session);
        let entry =
            counts
                .entry(signature)
                .or_insert((
                    0,
                    0,
                    summary.clone(),
                    proposal_text.clone(),
                    session.end_ts,
                    safety_score,
                ));
        entry.0 += 1;
        entry.1 += duration_ms;
        if session.end_ts > entry.4 {
            entry.4 = session.end_ts;
        }
        if safety_score < entry.5 {
            entry.5 = safety_score;
        }
    }

    let mut patterns: Vec<PatternCandidate> = counts
        .into_iter()
        .filter(|(_, (count, _, _, _, _, _))| *count > 1)
        .map(
            |(
                signature,
                (
                    count,
                    total_duration_ms,
                    canonical_summary,
                    proposal_text,
                    last_seen_at,
                    safety_score,
                ),
            )| {
                let avg_duration_ms = total_duration_ms / count as i64;
                let usefulness_score = usefulness_score(
                    count,
                    avg_duration_ms,
                    last_seen_at,
                    latest_observed_at.unwrap_or(last_seen_at),
                    safety_score,
                );
                PatternCandidate {
                    signature,
                    count,
                    avg_duration_ms,
                    canonical_summary,
                    proposal_text,
                    last_seen_at,
                    safety_score,
                    usefulness_score,
                }
            },
        )
        .collect();
    patterns.sort_by(|left, right| {
        right
            .usefulness_score
            .total_cmp(&left.usefulness_score)
            .then_with(|| right.count.cmp(&left.count))
            .then_with(|| left.signature.cmp(&right.signature))
    });
    patterns
}

fn usefulness_score(
    count: usize,
    avg_duration_ms: i64,
    last_seen_at: DateTime<Utc>,
    latest_observed_at: DateTime<Utc>,
    safety_score: f64,
) -> f64 {
    let repetition_score = (count as f64 / 5.0).clamp(0.0, 1.0);
    let freshness_score =
        freshness_score(latest_observed_at.signed_duration_since(last_seen_at).num_seconds());
    let duration_score = duration_score(avg_duration_ms);
    let score =
        (0.45 * repetition_score) + (0.3 * freshness_score) + (0.15 * duration_score) + (0.1 * safety_score);
    (score * 1000.0).round() / 1000.0
}

fn freshness_score(age_secs: i64) -> f64 {
    match age_secs {
        i64::MIN..=-1 => 1.0,
        0..=3_600 => 1.0,
        3_601..=86_400 => 0.8,
        86_401..=604_800 => 0.55,
        _ => 0.3,
    }
}

fn duration_score(avg_duration_ms: i64) -> f64 {
    match avg_duration_ms {
        i64::MIN..=60_000 => 1.0,
        60_001..=300_000 => 0.8,
        300_001..=900_000 => 0.6,
        _ => 0.4,
    }
}

fn action_safety_score(session: &EventSession) -> f64 {
    let has_rename = session
        .events
        .iter()
        .any(|event| event.action_type == flow_core::events::ActionType::RenameFile);
    let has_move = session
        .events
        .iter()
        .any(|event| event.action_type == flow_core::events::ActionType::MoveFile);

    if has_rename || has_move {
        1.0
    } else {
        0.7
    }
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
        assert_eq!(patterns[0].last_seen_at, Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 40).unwrap());
        assert_eq!(patterns[0].safety_score, 1.0);
        assert!(patterns[0].usefulness_score > 0.7);
    }

    #[test]
    fn ranks_more_recent_repeated_patterns_first() {
        let raw_events = vec![
            synthetic_file_event(
                Utc.with_ymd_and_hms(2026, 1, 10, 9, 0, 0).unwrap(),
                FileEventKind::Create,
                "/tmp/inbox/report-1001.txt",
                None,
            ),
            synthetic_file_event(
                Utc.with_ymd_and_hms(2026, 1, 10, 9, 6, 0).unwrap(),
                FileEventKind::Create,
                "/tmp/inbox/report-1002.txt",
                None,
            ),
            synthetic_file_event(
                Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 0).unwrap(),
                FileEventKind::Create,
                "/tmp/inbox/invoice-1001.pdf",
                None,
            ),
            synthetic_file_event(
                Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 20).unwrap(),
                FileEventKind::Rename,
                "/tmp/inbox/invoice-1001-reviewed.pdf",
                Some("/tmp/inbox/invoice-1001.pdf".to_string()),
            ),
            synthetic_file_event(
                Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 40).unwrap(),
                FileEventKind::Move,
                "/tmp/archive/invoice-1001-reviewed.pdf",
                Some("/tmp/inbox/invoice-1001-reviewed.pdf".to_string()),
            ),
            synthetic_file_event(
                Utc.with_ymd_and_hms(2026, 1, 15, 11, 0, 0).unwrap(),
                FileEventKind::Create,
                "/tmp/inbox/invoice-1002.pdf",
                None,
            ),
            synthetic_file_event(
                Utc.with_ymd_and_hms(2026, 1, 15, 11, 0, 20).unwrap(),
                FileEventKind::Rename,
                "/tmp/inbox/invoice-1002-reviewed.pdf",
                Some("/tmp/inbox/invoice-1002.pdf".to_string()),
            ),
            synthetic_file_event(
                Utc.with_ymd_and_hms(2026, 1, 15, 11, 0, 40).unwrap(),
                FileEventKind::Move,
                "/tmp/archive/invoice-1002-reviewed.pdf",
                Some("/tmp/inbox/invoice-1002-reviewed.pdf".to_string()),
            ),
        ];

        let normalized: Vec<_> = raw_events.iter().filter_map(normalize).collect();
        let sessions = split_into_sessions(&normalized, 300);
        let patterns = detect_repeated_patterns(&sessions);

        assert_eq!(patterns.len(), 2);
        assert!(patterns[0].usefulness_score > patterns[1].usefulness_score);
        assert!(patterns[0].proposal_text.contains("invoice"));
        assert!(patterns[1].proposal_text.contains("report"));
    }
}
