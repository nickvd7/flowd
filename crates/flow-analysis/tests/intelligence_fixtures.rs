use anyhow::Result;
use chrono::{TimeZone, Utc};
use flow_adapters::file_watcher::{synthetic_file_event, FileEventKind};
use flow_analysis::{
    intelligence_boundary::{
        display_stored_suggestions, rank_stored_suggestions, ExplainabilitySource,
        IntelligenceBoundary, IntelligenceCandidateInput, IntelligenceClient,
        IntelligenceDisplayDecision, IntelligenceExplanation, IntelligenceRankingFactor,
        IntelligenceRequest, IntelligenceResponse, IntelligenceScoreComponent,
        SuggestionDecisionAction, SuggestionDisplayResult,
    },
    refresh_analysis_state, refresh_analysis_state_with_intelligence,
};
use flow_db::{
    migrations::run_migrations,
    repo::{insert_normalized_event_record, list_suggestions, StoredSuggestion},
};
use flow_patterns::normalize::normalize;
use rusqlite::{params, Connection};
use std::cell::RefCell;

const INACTIVITY_SECS: i64 = 300;
const FIXED_CREATED_AT: &str = "2026-01-21T12:00:00+00:00";
const FEEDBACK_TS: &str = "2026-01-21T11:30:00+00:00";

#[derive(Debug, Clone, Copy)]
struct FixtureFeedback {
    shown_count: u32,
    accepted_count: u32,
    rejected_count: u32,
    snoozed_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
struct FixtureSnapshot {
    persisted: Vec<StoredSuggestion>,
    ranked: Vec<StoredSuggestion>,
    displayed: Vec<StoredSuggestion>,
    explainability: Vec<SuggestionDisplayResult>,
    requests: Vec<IntelligenceRequest>,
}

#[derive(Default)]
struct RecordingFixtureIntelligenceClient {
    requests: RefCell<Vec<IntelligenceRequest>>,
}

impl RecordingFixtureIntelligenceClient {
    fn recorded_requests(&self) -> Vec<IntelligenceRequest> {
        self.requests.borrow().clone()
    }

    fn decide(candidate: &IntelligenceCandidateInput) -> IntelligenceDisplayDecision {
        let signature = candidate.pattern_signature.as_str();
        let baseline_score = candidate.suggestion.usefulness_score;

        if candidate.history.rejected_count >= 2 {
            return IntelligenceDisplayDecision {
                pattern_signature: candidate.pattern_signature.clone(),
                action: SuggestionDecisionAction::Suppress,
                proposal_text: None,
                usefulness_score: Some((baseline_score - 0.25).max(0.0)),
                rank_hint: None,
                explanation: Some(IntelligenceExplanation {
                    summary: Some("Recent rejections suppressed this suggestion.".to_string()),
                    score_breakdown: vec![IntelligenceScoreComponent {
                        label: "rejection_penalty".to_string(),
                        value: candidate.history.rejected_count as f64,
                    }],
                    timing_reason: None,
                    suppression_reason: Some(
                        "The user recently rejected similar suggestions.".to_string(),
                    ),
                    ranking_factors: vec![IntelligenceRankingFactor {
                        label: "feedback".to_string(),
                        detail: "Repeated rejections lowered display priority.".to_string(),
                    }],
                }),
            };
        }

        if candidate.history.snoozed_count >= 2 {
            return IntelligenceDisplayDecision {
                pattern_signature: candidate.pattern_signature.clone(),
                action: SuggestionDecisionAction::Delay,
                proposal_text: None,
                usefulness_score: Some((baseline_score - 0.05).max(0.0)),
                rank_hint: None,
                explanation: Some(IntelligenceExplanation {
                    summary: Some("Recent snoozes delayed this suggestion.".to_string()),
                    score_breakdown: vec![IntelligenceScoreComponent {
                        label: "snooze_penalty".to_string(),
                        value: candidate.history.snoozed_count as f64,
                    }],
                    timing_reason: Some(
                        "The user snoozed similar suggestions and asked to revisit later."
                            .to_string(),
                    ),
                    suppression_reason: None,
                    ranking_factors: vec![IntelligenceRankingFactor {
                        label: "timing".to_string(),
                        detail: "Recent snoozes delayed display for this workflow.".to_string(),
                    }],
                }),
            };
        }

        let stale_low_value = candidate
            .recency
            .seconds_since_last_seen
            .unwrap_or_default()
            > 7 * 24 * 60 * 60
            && baseline_score < 0.75;
        if stale_low_value {
            return IntelligenceDisplayDecision {
                pattern_signature: candidate.pattern_signature.clone(),
                action: SuggestionDecisionAction::Suppress,
                proposal_text: None,
                usefulness_score: Some((baseline_score - 0.1).max(0.0)),
                rank_hint: None,
                explanation: Some(IntelligenceExplanation {
                    summary: Some("Stale low-value work was suppressed.".to_string()),
                    score_breakdown: vec![IntelligenceScoreComponent {
                        label: "staleness_penalty".to_string(),
                        value: candidate
                            .recency
                            .seconds_since_last_seen
                            .unwrap_or_default() as f64,
                    }],
                    timing_reason: None,
                    suppression_reason: Some(
                        "This workflow is old and lower value than newer active suggestions."
                            .to_string(),
                    ),
                    ranking_factors: vec![IntelligenceRankingFactor {
                        label: "staleness".to_string(),
                        detail: "Older low-signal work was removed from display.".to_string(),
                    }],
                }),
            };
        }

        if signature.contains("screenshot") && candidate.history.accepted_count >= 2 {
            return IntelligenceDisplayDecision {
                pattern_signature: candidate.pattern_signature.clone(),
                action: SuggestionDecisionAction::Keep,
                proposal_text: Some(
                    "Prioritize screenshot cleanup into the archive folder.".to_string(),
                ),
                usefulness_score: Some((baseline_score + 0.2).min(1.5)),
                rank_hint: Some(0),
                explanation: Some(IntelligenceExplanation {
                    summary: Some(
                        "Accepted screenshot cleanups were promoted ahead of the baseline order."
                            .to_string(),
                    ),
                    score_breakdown: vec![
                        IntelligenceScoreComponent {
                            label: "baseline_score".to_string(),
                            value: baseline_score,
                        },
                        IntelligenceScoreComponent {
                            label: "accepted_feedback_boost".to_string(),
                            value: candidate.history.accepted_count as f64,
                        },
                    ],
                    timing_reason: None,
                    suppression_reason: None,
                    ranking_factors: vec![
                        IntelligenceRankingFactor {
                            label: "feedback".to_string(),
                            detail: "Repeated accepts increased confidence for screenshot cleanup."
                                .to_string(),
                        },
                        IntelligenceRankingFactor {
                            label: "ranking".to_string(),
                            detail: "This workflow was promoted ahead of baseline ordering."
                                .to_string(),
                        },
                    ],
                }),
            };
        }

        IntelligenceDisplayDecision {
            pattern_signature: candidate.pattern_signature.clone(),
            action: SuggestionDecisionAction::Keep,
            proposal_text: None,
            usefulness_score: Some(baseline_score),
            rank_hint: None,
            explanation: Some(IntelligenceExplanation {
                summary: Some(
                    "Baseline ordering remained appropriate for this workflow.".to_string(),
                ),
                score_breakdown: vec![IntelligenceScoreComponent {
                    label: "baseline_score".to_string(),
                    value: baseline_score,
                }],
                timing_reason: None,
                suppression_reason: None,
                ranking_factors: vec![IntelligenceRankingFactor {
                    label: "baseline".to_string(),
                    detail: "No stronger intelligence signal changed this suggestion.".to_string(),
                }],
            }),
        }
    }
}

impl IntelligenceClient for RecordingFixtureIntelligenceClient {
    fn evaluate(&self, request: &IntelligenceRequest) -> Result<IntelligenceResponse> {
        self.requests.borrow_mut().push(request.clone());
        Ok(IntelligenceResponse {
            decisions: request.candidates.iter().map(Self::decide).collect(),
        })
    }
}

fn invoice_events() -> Vec<flow_core::events::RawEvent> {
    repeated_create_rename_move_workflow(
        "invoice",
        "reviewed",
        "/tmp/inbox",
        "/tmp/archive",
        [
            Utc.with_ymd_and_hms(2026, 1, 20, 9, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 1, 20, 10, 0, 0).unwrap(),
        ],
    )
}

fn screenshot_events() -> Vec<flow_core::events::RawEvent> {
    repeated_create_move_workflow(
        "screenshot",
        "png",
        "/tmp/Desktop",
        "/tmp/Archive/Screenshots",
        [
            Utc.with_ymd_and_hms(2026, 1, 20, 7, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 1, 20, 8, 0, 0).unwrap(),
        ],
    )
}

fn rejected_report_events() -> Vec<flow_core::events::RawEvent> {
    repeated_create_move_workflow(
        "report",
        "pdf",
        "/tmp/reports/incoming",
        "/tmp/reports/archive",
        [
            Utc.with_ymd_and_hms(2026, 1, 17, 9, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 1, 17, 10, 0, 0).unwrap(),
        ],
    )
}

fn snoozed_receipt_events() -> Vec<flow_core::events::RawEvent> {
    repeated_create_rename_move_workflow(
        "receipt",
        "sorted",
        "/tmp/receipts/inbox",
        "/tmp/receipts/archive",
        [
            Utc.with_ymd_and_hms(2026, 1, 19, 8, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 1, 19, 9, 0, 0).unwrap(),
        ],
    )
}

fn stale_scratchpad_events() -> Vec<flow_core::events::RawEvent> {
    repeated_create_only_workflow(
        "scratchpad",
        "txt",
        "/tmp/scratch",
        [
            Utc.with_ymd_and_hms(2026, 1, 1, 9, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 1, 1, 10, 0, 0).unwrap(),
        ],
    )
}

fn combined_fixture_events() -> Vec<flow_core::events::RawEvent> {
    let mut events = Vec::new();
    events.extend(stale_scratchpad_events());
    events.extend(rejected_report_events());
    events.extend(snoozed_receipt_events());
    events.extend(screenshot_events());
    events.extend(invoice_events());
    events
}

fn repeated_create_rename_move_workflow(
    name: &str,
    rename_suffix: &str,
    source_dir: &str,
    archive_dir: &str,
    starts: [chrono::DateTime<Utc>; 2],
) -> Vec<flow_core::events::RawEvent> {
    starts
        .into_iter()
        .enumerate()
        .flat_map(|(index, start)| {
            let file_name = format!("{name}-{}.pdf", index + 1);
            let renamed = format!("{name}-{}-{rename_suffix}.pdf", index + 1);
            let source_path = format!("{source_dir}/{file_name}");
            let renamed_path = format!("{source_dir}/{renamed}");
            let archive_path = format!("{archive_dir}/{renamed}");

            vec![
                synthetic_file_event(start, FileEventKind::Create, &source_path, None),
                synthetic_file_event(
                    start + chrono::Duration::seconds(20),
                    FileEventKind::Rename,
                    &renamed_path,
                    Some(source_path),
                ),
                synthetic_file_event(
                    start + chrono::Duration::seconds(40),
                    FileEventKind::Move,
                    &archive_path,
                    Some(renamed_path),
                ),
            ]
        })
        .collect()
}

fn repeated_create_move_workflow(
    name: &str,
    extension: &str,
    source_dir: &str,
    archive_dir: &str,
    starts: [chrono::DateTime<Utc>; 2],
) -> Vec<flow_core::events::RawEvent> {
    starts
        .into_iter()
        .enumerate()
        .flat_map(|(index, start)| {
            let file_name = format!("{name}-{}.{}", index + 1, extension);
            let source_path = format!("{source_dir}/{file_name}");
            let archive_path = format!("{archive_dir}/{file_name}");

            vec![
                synthetic_file_event(start, FileEventKind::Create, &source_path, None),
                synthetic_file_event(
                    start + chrono::Duration::seconds(20),
                    FileEventKind::Move,
                    &archive_path,
                    Some(source_path),
                ),
            ]
        })
        .collect()
}

fn repeated_create_only_workflow(
    name: &str,
    extension: &str,
    dir: &str,
    starts: [chrono::DateTime<Utc>; 2],
) -> Vec<flow_core::events::RawEvent> {
    starts
        .into_iter()
        .enumerate()
        .map(|(index, start)| {
            let path = format!("{dir}/{name}-{}.{}", index + 1, extension);
            synthetic_file_event(start, FileEventKind::Create, &path, None)
        })
        .collect()
}

fn setup_combined_fixture_db() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();

    for raw_event in combined_fixture_events() {
        let normalized = normalize(&raw_event).unwrap();
        insert_normalized_event_record(&mut conn, &normalized).unwrap();
    }

    conn
}

fn signature_for_group(group: &str) -> String {
    format!("CreateFile:{group}")
}

fn suggestion_signature(conn: &Connection, prefix: &str) -> String {
    list_suggestions(conn)
        .unwrap()
        .into_iter()
        .map(|suggestion| suggestion.signature)
        .find(|signature| signature.starts_with(prefix))
        .unwrap()
}

fn apply_feedback(conn: &Connection, signature: &str, feedback: FixtureFeedback) {
    conn.execute(
        r#"
        UPDATE suggestions
        SET shown_count = ?1,
            accepted_count = ?2,
            rejected_count = ?3,
            snoozed_count = ?4,
            last_shown_ts = CASE WHEN ?1 > 0 THEN ?5 ELSE NULL END,
            last_accepted_ts = CASE WHEN ?2 > 0 THEN ?5 ELSE NULL END,
            last_rejected_ts = CASE WHEN ?3 > 0 THEN ?5 ELSE NULL END,
            last_snoozed_ts = CASE WHEN ?4 > 0 THEN ?5 ELSE NULL END
        WHERE id = (
            SELECT suggestions.id
            FROM suggestions
            INNER JOIN patterns ON patterns.id = suggestions.pattern_id
            WHERE patterns.signature = ?6
            ORDER BY suggestions.id ASC
            LIMIT 1
        )
        "#,
        params![
            feedback.shown_count,
            feedback.accepted_count,
            feedback.rejected_count,
            feedback.snoozed_count,
            FEEDBACK_TS,
            signature,
        ],
    )
    .unwrap();
}

fn stabilize_suggestion_timestamps(conn: &Connection) {
    conn.execute(
        "UPDATE suggestions SET created_at = ?1 WHERE status = 'pending'",
        [FIXED_CREATED_AT],
    )
    .unwrap();
}

fn prepare_fixture_with_feedback(conn: &mut Connection) -> Vec<StoredSuggestion> {
    refresh_analysis_state(conn, INACTIVITY_SECS).unwrap();
    let baseline = list_suggestions(conn).unwrap();
    assert_eq!(baseline.len(), 5);

    let screenshot_signature = suggestion_signature(conn, &signature_for_group("screenshot"));
    let report_signature = suggestion_signature(conn, &signature_for_group("report"));
    let receipt_signature = suggestion_signature(conn, &signature_for_group("receipt"));

    apply_feedback(
        conn,
        &screenshot_signature,
        FixtureFeedback {
            shown_count: 3,
            accepted_count: 2,
            rejected_count: 0,
            snoozed_count: 0,
        },
    );
    apply_feedback(
        conn,
        &report_signature,
        FixtureFeedback {
            shown_count: 3,
            accepted_count: 0,
            rejected_count: 2,
            snoozed_count: 0,
        },
    );
    apply_feedback(
        conn,
        &receipt_signature,
        FixtureFeedback {
            shown_count: 3,
            accepted_count: 0,
            rejected_count: 0,
            snoozed_count: 2,
        },
    );

    stabilize_suggestion_timestamps(conn);
    baseline
}

fn run_integrated_fixture_snapshot() -> FixtureSnapshot {
    let mut conn = setup_combined_fixture_db();
    prepare_fixture_with_feedback(&mut conn);

    let client = RecordingFixtureIntelligenceClient::default();
    refresh_analysis_state_with_intelligence(&mut conn, INACTIVITY_SECS, &client).unwrap();
    stabilize_suggestion_timestamps(&conn);

    let persisted = list_suggestions(&conn).unwrap();
    let ranked = rank_stored_suggestions(&persisted, &client).unwrap();
    let displayed = display_stored_suggestions(&persisted, &client).unwrap();
    let explainability = IntelligenceBoundary::new(&client)
        .evaluate_stored_suggestions_for_display(&persisted)
        .unwrap();

    FixtureSnapshot {
        persisted,
        ranked,
        displayed,
        explainability,
        requests: client.recorded_requests(),
    }
}

#[test]
fn fixture_generates_realistic_baseline_suggestions_before_intelligence() {
    let mut conn = setup_combined_fixture_db();
    let baseline = prepare_fixture_with_feedback(&mut conn);

    let signatures: Vec<_> = baseline
        .iter()
        .map(|suggestion| suggestion.signature.as_str())
        .collect();
    assert_eq!(signatures.len(), 5);
    assert!(baseline[0].signature.starts_with("CreateFile:invoice"));
    assert!(signatures
        .iter()
        .any(|value| value.starts_with("CreateFile:screenshot")));
    assert!(signatures
        .iter()
        .any(|value| value.starts_with("CreateFile:receipt")));
    assert!(signatures
        .iter()
        .any(|value| value.starts_with("CreateFile:report")));
    assert!(signatures
        .iter()
        .any(|value| value == &"CreateFile:scratchpad"));
}

#[test]
fn intelligence_enabled_fixture_reorders_displayed_suggestions_from_baseline() {
    let mut conn = setup_combined_fixture_db();
    let baseline = prepare_fixture_with_feedback(&mut conn);

    let client = RecordingFixtureIntelligenceClient::default();
    refresh_analysis_state_with_intelligence(&mut conn, INACTIVITY_SECS, &client).unwrap();
    stabilize_suggestion_timestamps(&conn);

    let persisted = list_suggestions(&conn).unwrap();
    let ranked = rank_stored_suggestions(&persisted, &client).unwrap();
    let displayed = display_stored_suggestions(&persisted, &client).unwrap();

    assert!(baseline[0].signature.starts_with("CreateFile:invoice"));
    assert!(ranked[0].signature.starts_with("CreateFile:screenshot"));
    assert!(displayed[0].signature.starts_with("CreateFile:screenshot"));
    assert_eq!(
        displayed[0].proposal_text,
        "Prioritize screenshot cleanup into the archive folder."
    );
}

#[test]
fn intelligence_fixture_respects_delayed_and_suppressed_decisions() {
    let snapshot = run_integrated_fixture_snapshot();

    let persisted_signatures: Vec<_> = snapshot
        .persisted
        .iter()
        .map(|suggestion| suggestion.signature.as_str())
        .collect();
    let displayed_signatures: Vec<_> = snapshot
        .displayed
        .iter()
        .map(|suggestion| suggestion.signature.as_str())
        .collect();

    assert_eq!(persisted_signatures.len(), 3);
    assert!(persisted_signatures
        .iter()
        .any(|value| value.starts_with("CreateFile:invoice")));
    assert!(persisted_signatures
        .iter()
        .any(|value| value.starts_with("CreateFile:screenshot")));
    assert!(persisted_signatures
        .iter()
        .any(|value| value.starts_with("CreateFile:receipt")));
    assert!(!persisted_signatures
        .iter()
        .any(|value| value.starts_with("CreateFile:report")));
    assert!(!persisted_signatures
        .iter()
        .any(|value| value == &"CreateFile:scratchpad"));
    assert_eq!(displayed_signatures.len(), 2);
    assert!(!displayed_signatures
        .iter()
        .any(|value| value.starts_with("CreateFile:receipt")));
}

#[test]
fn intelligence_fixture_passes_feedback_history_through_the_boundary() {
    let snapshot = run_integrated_fixture_snapshot();
    let refresh_request = &snapshot.requests[0];

    assert_eq!(refresh_request.context.candidate_count, 5);
    assert_eq!(refresh_request.context.session_summary.total_sessions, 10);
    assert_eq!(refresh_request.context.feedback_summary.shown_count, 9);
    assert_eq!(refresh_request.context.feedback_summary.accepted_count, 2);
    assert_eq!(refresh_request.context.feedback_summary.rejected_count, 2);
    assert_eq!(refresh_request.context.feedback_summary.snoozed_count, 2);
    assert_eq!(
        refresh_request
            .context
            .feedback_summary
            .candidates_with_feedback,
        3
    );

    let screenshot = refresh_request
        .candidates
        .iter()
        .find(|candidate| {
            candidate
                .pattern_signature
                .starts_with("CreateFile:screenshot")
        })
        .unwrap();
    assert_eq!(screenshot.history.accepted_count, 2);
    assert_eq!(screenshot.history.shown_count, 3);
    assert_eq!(
        screenshot.history.last_accepted_ts.as_deref(),
        Some(FEEDBACK_TS)
    );

    let report = refresh_request
        .candidates
        .iter()
        .find(|candidate| candidate.pattern_signature.starts_with("CreateFile:report"))
        .unwrap();
    assert_eq!(report.history.rejected_count, 2);
    assert_eq!(
        report.history.last_rejected_ts.as_deref(),
        Some(FEEDBACK_TS)
    );

    let receipt = refresh_request
        .candidates
        .iter()
        .find(|candidate| {
            candidate
                .pattern_signature
                .starts_with("CreateFile:receipt")
        })
        .unwrap();
    assert_eq!(receipt.history.snoozed_count, 2);
    assert_eq!(
        receipt.history.last_snoozed_ts.as_deref(),
        Some(FEEDBACK_TS)
    );
}

#[test]
fn explainability_survives_the_integrated_intelligence_path() {
    let snapshot = run_integrated_fixture_snapshot();

    let screenshot = snapshot
        .explainability
        .iter()
        .find(|result| {
            result
                .suggestion
                .signature
                .starts_with("CreateFile:screenshot")
        })
        .unwrap();
    assert_eq!(
        screenshot.explainability.source,
        ExplainabilitySource::Intelligence
    );
    assert_eq!(
        screenshot.explainability.action,
        SuggestionDecisionAction::Keep
    );
    assert_eq!(screenshot.explainability.rank_hint, Some(0));
    assert!(screenshot
        .explainability
        .summary
        .contains("Accepted screenshot cleanups"));
    assert!(screenshot
        .explainability
        .score_breakdown
        .iter()
        .any(|value| value.label == "accepted_feedback_boost"));

    let receipt = snapshot
        .explainability
        .iter()
        .find(|result| {
            result
                .suggestion
                .signature
                .starts_with("CreateFile:receipt")
        })
        .unwrap();
    assert_eq!(
        receipt.explainability.action,
        SuggestionDecisionAction::Delay
    );
    assert_eq!(
        receipt.explainability.timing_reason.as_deref(),
        Some("The user snoozed similar suggestions and asked to revisit later.")
    );
}

#[test]
fn open_core_fallback_still_works_without_intelligence() {
    let mut conn = setup_combined_fixture_db();
    let baseline = prepare_fixture_with_feedback(&mut conn);
    let displayed = display_stored_suggestions(
        &baseline,
        &flow_analysis::intelligence_boundary::NoopIntelligenceClient,
    )
    .unwrap();
    let explainability =
        IntelligenceBoundary::new(&flow_analysis::intelligence_boundary::NoopIntelligenceClient)
            .evaluate_stored_suggestions_for_display(&baseline)
            .unwrap();

    assert_eq!(displayed, baseline);
    assert!(explainability
        .iter()
        .all(|result| result.explainability.source == ExplainabilitySource::BaselineFallback));
}

#[test]
fn identical_fixtures_produce_identical_intelligence_outcomes() {
    let first = run_integrated_fixture_snapshot();
    let second = run_integrated_fixture_snapshot();

    assert_eq!(first, second);
}
