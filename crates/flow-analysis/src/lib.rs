pub mod intelligence_boundary;

use anyhow::{Context, Result};
use flow_db::repo::{
    clear_session_state, insert_normalized_events_for_raw_events, insert_session,
    list_normalized_events, list_pending_file_raw_events, mark_stale_patterns_and_suggestions,
    suppress_suggestions_for_pattern, sync_suggestion_for_pattern, upsert_pattern,
};
use flow_patterns::{
    detect::detect_repeated_patterns, normalize::normalize, sessions::split_into_sessions,
};
use intelligence_boundary::{
    IntelligenceBoundary, IntelligenceClient, NoopIntelligenceClient, SuggestionDecisionAction,
};
use rusqlite::Connection;

/// The open-core analysis layer owns normalization, session building, pattern
/// detection, and baseline suggestion generation. Optional private
/// intelligence can only refine presentation decisions through the narrow
/// boundary in this crate; storage, approval, execution, and undo remain in
/// open-core regardless of whether an intelligence client exists.
pub fn refresh_analysis_state(conn: &mut Connection, inactivity_secs: i64) -> Result<()> {
    refresh_analysis_state_with_intelligence(conn, inactivity_secs, &NoopIntelligenceClient)
}

pub fn refresh_analysis_state_with_intelligence(
    conn: &mut Connection,
    inactivity_secs: i64,
    intelligence_client: &dyn IntelligenceClient,
) -> Result<()> {
    let stored_events = list_normalized_events(conn).context("failed to read normalized events")?;
    let normalized_events: Vec<_> = stored_events
        .iter()
        .map(|stored| stored.event.clone())
        .collect();
    let sessions = split_into_sessions(&normalized_events, inactivity_secs);
    let patterns = detect_repeated_patterns(&sessions);
    let created_at = chrono::Utc::now().to_rfc3339();

    let presentations = IntelligenceBoundary::new(intelligence_client)
        .evaluate_patterns(&patterns)
        .context("failed to evaluate intelligence boundary")?;

    let tx = conn
        .transaction()
        .context("failed to start analysis refresh transaction")?;
    clear_session_state(&tx).context("failed to clear session state")?;

    let mut offset = 0usize;
    for session in &sessions {
        let next_offset = offset + session.events.len();
        let event_ids: Vec<_> = stored_events[offset..next_offset]
            .iter()
            .map(|stored| stored.id)
            .collect();
        insert_session(
            &tx,
            &session.start_ts.to_rfc3339(),
            &session.end_ts.to_rfc3339(),
            &event_ids,
        )
        .context("failed to store rebuilt session")?;
        offset = next_offset;
    }

    let mut active_pattern_ids = Vec::new();
    for pattern in &patterns {
        let pattern_id = upsert_pattern(
            &tx,
            &pattern.signature,
            pattern.count,
            pattern.avg_duration_ms,
            &pattern.canonical_summary,
            &pattern.last_seen_at.to_rfc3339(),
            pattern.safety_score,
            pattern.usefulness_score,
        )
        .context("failed to upsert pattern")?;
        let presentation = presentations
            .iter()
            .find(|value| value.pattern_signature == pattern.signature)
            .expect("presentation must exist for every detected pattern");
        if presentation.action == SuggestionDecisionAction::Suppress {
            suppress_suggestions_for_pattern(&tx, pattern_id, presentation.usefulness_score)
                .context("failed to suppress suggestion")?;
        } else {
            sync_suggestion_for_pattern(
                &tx,
                pattern_id,
                &presentation.proposal_text,
                &created_at,
                presentation.usefulness_score,
            )
            .context("failed to sync suggestion")?;
        }
        active_pattern_ids.push(pattern_id);
    }

    mark_stale_patterns_and_suggestions(&tx, &active_pattern_ids)
        .context("failed to mark stale patterns and suggestions")?;
    tx.commit()
        .context("failed to commit analysis refresh transaction")?;

    Ok(())
}

pub fn normalize_pending_raw_events(conn: &mut Connection) -> Result<()> {
    let mut normalized_events = Vec::new();

    for raw_event in
        list_pending_file_raw_events(conn).context("failed to load pending raw file events")?
    {
        let Some(normalized_event) = normalize(&raw_event.event) else {
            continue;
        };

        normalized_events.push((raw_event.id, normalized_event));
    }

    insert_normalized_events_for_raw_events(conn, &normalized_events)
        .context("failed to insert normalized events")?;

    Ok(())
}

pub fn catch_up_analysis(conn: &mut Connection, inactivity_secs: i64) -> Result<()> {
    normalize_pending_raw_events(conn)?;
    refresh_analysis_state(conn, inactivity_secs)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intelligence_boundary::{
        build_intelligence_request, map_patterns_to_contexts, IntelligenceClient,
        IntelligenceDisplayDecision, IntelligenceRequest, IntelligenceResponse,
        SuggestionDecisionAction,
    };
    use chrono::Utc;
    use flow_adapters::file_watcher::{synthetic_file_event, FileEvent, FileEventKind};
    use flow_db::{
        migrations::run_migrations,
        repo::{
            get_suggestion, insert_automation, insert_normalized_event_record, insert_raw_event,
            list_automations, list_normalized_events, list_patterns, list_pending_file_raw_events,
            list_suggestions, set_suggestion_status, AUTOMATION_STATUS_ACTIVE,
        },
    };

    struct TestIntelligenceClient;

    impl IntelligenceClient for TestIntelligenceClient {
        fn evaluate(&self, request: &IntelligenceRequest) -> Result<IntelligenceResponse> {
            Ok(IntelligenceResponse {
                decisions: request
                    .candidates
                    .iter()
                    .map(|candidate| IntelligenceDisplayDecision {
                        pattern_signature: candidate.pattern_signature.clone(),
                        action: SuggestionDecisionAction::Keep,
                        proposal_text: Some(format!(
                            "Refined: {}",
                            candidate.suggestion.baseline_proposal_text
                        )),
                        usefulness_score: Some(candidate.suggestion.usefulness_score + 0.05),
                        rank_hint: None,
                    })
                    .collect(),
            })
        }
    }

    fn load_invoice_fixture_events() -> Vec<flow_core::events::RawEvent> {
        let fixture_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/invoice_file_events.json"
        );
        let raw_fixture = std::fs::read_to_string(fixture_path).unwrap();
        let file_events: Vec<FileEvent> = serde_json::from_str(&raw_fixture).unwrap();
        file_events
            .into_iter()
            .map(FileEvent::into_raw_event)
            .collect()
    }

    #[test]
    fn baseline_suggestion_flow_works_without_private_intelligence() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        for raw_event in load_invoice_fixture_events() {
            let normalized = normalize(&raw_event).unwrap();
            insert_normalized_event_record(&mut conn, &normalized).unwrap();
        }

        refresh_analysis_state(&mut conn, 300).unwrap();

        let suggestions = list_suggestions(&conn).unwrap();
        assert_eq!(suggestions.len(), 1);
        assert!(suggestions[0]
            .proposal_text
            .contains("Repeated invoice file workflow detected"));
        assert_eq!(list_patterns(&conn).unwrap().len(), 1);
    }

    #[test]
    fn optional_intelligence_isolated_behind_one_boundary() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        for raw_event in load_invoice_fixture_events() {
            let normalized = normalize(&raw_event).unwrap();
            insert_normalized_event_record(&mut conn, &normalized).unwrap();
        }

        refresh_analysis_state_with_intelligence(&mut conn, 300, &TestIntelligenceClient).unwrap();

        let baseline_score = build_intelligence_request(&map_patterns_to_contexts(
            &detect_repeated_patterns(&split_into_sessions(
                &list_normalized_events(&conn)
                    .unwrap()
                    .into_iter()
                    .map(|stored| stored.event)
                    .collect::<Vec<_>>(),
                300,
            )),
        ))
        .candidates[0]
            .suggestion
            .usefulness_score;
        let suggestion = list_suggestions(&conn).unwrap().remove(0);
        assert!(suggestion.proposal_text.starts_with("Refined:"));
        assert!(suggestion.usefulness_score > baseline_score);
    }

    #[test]
    fn open_core_execution_records_survive_analysis_refresh() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        for raw_event in load_invoice_fixture_events() {
            let normalized = normalize(&raw_event).unwrap();
            insert_normalized_event_record(&mut conn, &normalized).unwrap();
        }

        refresh_analysis_state(&mut conn, 300).unwrap();

        let suggestion = list_suggestions(&conn).unwrap().remove(0);
        set_suggestion_status(&conn, suggestion.suggestion_id, "approved").unwrap();
        insert_automation(
            &conn,
            suggestion.suggestion_id,
            "id: test\ntrigger: {}\nactions: []\n",
            AUTOMATION_STATUS_ACTIVE,
            &suggestion.proposal_text,
            "2026-01-15T10:00:00Z",
        )
        .unwrap();

        conn.execute("DELETE FROM normalized_events", []).unwrap();
        refresh_analysis_state(&mut conn, 300).unwrap();

        let approved = get_suggestion(&conn, suggestion.suggestion_id)
            .unwrap()
            .unwrap();
        assert_eq!(approved.status, "approved");
        assert_eq!(list_automations(&conn).unwrap().len(), 1);
        assert!(list_suggestions(&conn).unwrap().is_empty());
    }

    #[test]
    fn catch_up_analysis_keeps_raw_normalization_and_suggestion_generation_open_core() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        let now = Utc::now();

        for raw_event in [
            synthetic_file_event(now, FileEventKind::Create, "/tmp/inbox/invoice-1.pdf", None),
            synthetic_file_event(
                now + chrono::Duration::seconds(10),
                FileEventKind::Rename,
                "/tmp/inbox/invoice-1-reviewed.pdf",
                Some("/tmp/inbox/invoice-1.pdf".to_string()),
            ),
            synthetic_file_event(
                now + chrono::Duration::seconds(20),
                FileEventKind::Move,
                "/tmp/archive/invoice-1-reviewed.pdf",
                Some("/tmp/inbox/invoice-1-reviewed.pdf".to_string()),
            ),
            synthetic_file_event(
                now + chrono::Duration::hours(1),
                FileEventKind::Create,
                "/tmp/inbox/invoice-2.pdf",
                None,
            ),
            synthetic_file_event(
                now + chrono::Duration::hours(1) + chrono::Duration::seconds(10),
                FileEventKind::Rename,
                "/tmp/inbox/invoice-2-reviewed.pdf",
                Some("/tmp/inbox/invoice-2.pdf".to_string()),
            ),
            synthetic_file_event(
                now + chrono::Duration::hours(1) + chrono::Duration::seconds(20),
                FileEventKind::Move,
                "/tmp/archive/invoice-2-reviewed.pdf",
                Some("/tmp/inbox/invoice-2-reviewed.pdf".to_string()),
            ),
        ] {
            insert_raw_event(&conn, &raw_event).unwrap();
        }

        assert_eq!(list_pending_file_raw_events(&conn).unwrap().len(), 6);
        catch_up_analysis(&mut conn, 300).unwrap();

        assert_eq!(list_pending_file_raw_events(&conn).unwrap().len(), 0);
        assert_eq!(list_normalized_events(&conn).unwrap().len(), 6);
        assert_eq!(list_patterns(&conn).unwrap().len(), 1);
        assert_eq!(list_suggestions(&conn).unwrap().len(), 1);
    }
}
