//! Optional adapter from open-core intelligence DTOs into `flowd-intelligence`.
//!
//! Enabled only with `--features intelligence`. Default builds keep using
//! [`NoopIntelligenceClient`] so public CI never depends on the private crate.

use crate::intelligence_boundary::{
    IntelligenceClient, IntelligenceDisplayDecision, IntelligenceExplanation,
    IntelligenceRankingFactor, IntelligenceRequest, IntelligenceResponse,
    IntelligenceScoreComponent, SuggestionDecisionAction,
};
use anyhow::Result;
use chrono::{DateTime, Utc};
use flowd_intelligence::contracts::{
    evaluate_for_display, ActionSafety, AdapterConfig, DisplayCandidate, DisplayDecision,
    DisplayedSuggestion, HiddenSuggestion, PersonalizationSubject, PreferenceSignalKind,
    ProposalTone, SuggestionCandidate, SuggestionContext, SuggestionSignals, SuppressionDecision,
    TimingDecision, UserPreferenceSignal,
};

#[derive(Debug, Default, Clone)]
pub struct PrivateIntelligenceClient {
    config: AdapterConfig,
}

impl PrivateIntelligenceClient {
    pub fn new(config: AdapterConfig) -> Self {
        Self { config }
    }
}

impl IntelligenceClient for PrivateIntelligenceClient {
    fn evaluate(&self, request: &IntelligenceRequest) -> Result<IntelligenceResponse> {
        if request.candidates.is_empty() {
            return Ok(IntelligenceResponse::default());
        }
        let (candidates, preference_signals) = map_request_to_display_inputs(request);
        let decision = evaluate_for_display(candidates, preference_signals, self.config);
        Ok(map_display_decision_to_response(decision, request))
    }
}

fn map_request_to_display_inputs(
    request: &IntelligenceRequest,
) -> (Vec<DisplayCandidate>, Vec<UserPreferenceSignal>) {
    let now_ts = resolve_now_ts(request);
    let mut candidates = Vec::with_capacity(request.candidates.len());
    let mut preference_signals = Vec::new();

    for candidate in &request.candidates {
        let action_sequence = action_sequence_from_signature(&candidate.pattern_signature);
        let action_key = action_sequence
            .first()
            .cloned()
            .unwrap_or_else(|| "workflow".to_string());
        let subject =
            PersonalizationSubject::new(candidate.pattern_signature.clone(), action_key.clone());

        push_preference_signal(
            &mut preference_signals,
            subject.clone(),
            PreferenceSignalKind::AcceptedPattern,
            candidate.history.accepted_count,
        );
        push_preference_signal(
            &mut preference_signals,
            subject.clone(),
            PreferenceSignalKind::RejectedPattern,
            candidate.history.rejected_count,
        );
        push_preference_signal(
            &mut preference_signals,
            subject.clone(),
            PreferenceSignalKind::SnoozedPattern,
            candidate.history.snoozed_count,
        );

        let repetition = candidate.suggestion.count.clamp(1, u32::MAX as usize) as u32;
        let duration_ms = candidate.suggestion.avg_duration_ms.max(0) as u64;
        let usefulness = candidate.suggestion.usefulness_score.clamp(0.0, 1.0);
        let estimated_savings_ms =
            ((duration_ms as f64) * (repetition as f64) * usefulness).round().max(0.0) as u64;

        let first_seen_ts = candidate
            .recency
            .seconds_since_created
            .map(|seconds| now_ts.saturating_sub(seconds.max(0)))
            .unwrap_or_else(|| now_ts.saturating_sub((repetition.saturating_sub(1) as i64) * 600));
        let last_seen_ts = candidate
            .recency
            .seconds_since_last_seen
            .map(|seconds| now_ts.saturating_sub(seconds.max(0)))
            .unwrap_or(now_ts);

        let label = if candidate.suggestion.canonical_summary.trim().is_empty() {
            candidate.pattern_signature.clone()
        } else {
            candidate.suggestion.canonical_summary.clone()
        };
        let summary = if candidate.suggestion.baseline_proposal_text.trim().is_empty() {
            label.clone()
        } else {
            candidate.suggestion.baseline_proposal_text.clone()
        };

        let suggestion = SuggestionCandidate::new(
            candidate.pattern_signature.clone(),
            candidate.pattern_signature.clone(),
            label,
            summary,
            SuggestionSignals::new(
                repetition,
                duration_ms,
                estimated_savings_ms,
                first_seen_ts,
                last_seen_ts,
                map_action_safety(candidate.pattern.safety_score),
                usefulness as f32,
            ),
        );
        let context = SuggestionContext::new(
            now_ts,
            candidate.history.shown_count,
            candidate.history.accepted_count,
            candidate.history.rejected_count,
            candidate.history.snoozed_count,
            candidate
                .recency
                .seconds_since_last_shown
                .map(|seconds| now_ts.saturating_sub(seconds.max(0))),
            candidate
                .recency
                .seconds_since_last_rejected
                .map(|seconds| now_ts.saturating_sub(seconds.max(0))),
            candidate
                .recency
                .seconds_since_last_snoozed
                .map(|seconds| now_ts.saturating_sub(seconds.max(0))),
        );

        candidates.push(DisplayCandidate::new(
            suggestion,
            context,
            subject,
            action_sequence,
            None::<String>,
            None::<String>,
            None::<String>,
            None::<String>,
            ProposalTone::ActionOriented,
        ));
    }

    (candidates, preference_signals)
}

fn map_display_decision_to_response(
    decision: DisplayDecision,
    request: &IntelligenceRequest,
) -> IntelligenceResponse {
    let mut decisions = Vec::new();

    for shown in decision.shown_suggestions {
        decisions.push(map_shown_suggestion(shown));
    }
    for hidden in decision.hidden_suggestions {
        decisions.push(map_hidden_suggestion(hidden));
    }

    // Preserve open-core fallback behavior for any missing candidate.
    for candidate in &request.candidates {
        if !decisions
            .iter()
            .any(|decision| decision.pattern_signature == candidate.pattern_signature)
        {
            decisions.push(IntelligenceDisplayDecision {
                pattern_signature: candidate.pattern_signature.clone(),
                action: SuggestionDecisionAction::Keep,
                proposal_text: Some(candidate.suggestion.baseline_proposal_text.clone()),
                usefulness_score: Some(candidate.suggestion.usefulness_score),
                rank_hint: None,
                explanation: None,
            });
        }
    }

    IntelligenceResponse { decisions }
}

fn map_shown_suggestion(shown: DisplayedSuggestion) -> IntelligenceDisplayDecision {
    IntelligenceDisplayDecision {
        pattern_signature: shown.suggestion_id.clone(),
        action: SuggestionDecisionAction::Keep,
        proposal_text: Some(shown.wording.short_description.clone()),
        usefulness_score: Some(shown.final_score as f64),
        rank_hint: Some(shown.final_rank.saturating_sub(1) as usize),
        explanation: Some(map_explanation(
            &shown.explanation,
            shown.final_rank,
            shown.final_score,
            None,
            None,
        )),
    }
}

fn map_hidden_suggestion(hidden: HiddenSuggestion) -> IntelligenceDisplayDecision {
    let (action, timing_reason, suppression_reason) = classify_hidden(&hidden);
    IntelligenceDisplayDecision {
        pattern_signature: hidden.suggestion_id.clone(),
        action,
        proposal_text: None,
        usefulness_score: Some(hidden.final_score as f64),
        rank_hint: Some(hidden.final_rank.saturating_sub(1) as usize),
        explanation: Some(map_explanation(
            &hidden.explanation,
            hidden.final_rank,
            hidden.final_score,
            timing_reason,
            suppression_reason,
        )),
    }
}

fn classify_hidden(
    hidden: &HiddenSuggestion,
) -> (
    SuggestionDecisionAction,
    Option<String>,
    Option<String>,
) {
    match hidden.suppression {
        SuppressionDecision::NotSuppressed => match hidden.timing {
            TimingDecision::ShowNow => (
                SuggestionDecisionAction::Suppress,
                None,
                Some("hidden_without_explicit_reason".to_string()),
            ),
            TimingDecision::Delay { reason, .. } => (
                SuggestionDecisionAction::Delay,
                Some(format!("{reason:?}")),
                None,
            ),
        },
        SuppressionDecision::SuppressUntil { reason, .. }
        | SuppressionDecision::SuppressIndefinitely { reason } => (
            SuggestionDecisionAction::Suppress,
            None,
            Some(format!("{reason:?}")),
        ),
    }
}

fn map_explanation(
    explanation: &flowd_intelligence::contracts::DisplayExplanation,
    final_rank: u32,
    final_score: f32,
    timing_reason: Option<String>,
    suppression_reason: Option<String>,
) -> IntelligenceExplanation {
    IntelligenceExplanation {
        summary: Some(format!(
            "rank {final_rank}; score {final_score:.3}; confidence {:.3}",
            explanation.final_confidence
        )),
        score_breakdown: vec![
            IntelligenceScoreComponent {
                label: "final_score".to_string(),
                value: final_score as f64,
            },
            IntelligenceScoreComponent {
                label: "base_score".to_string(),
                value: explanation.base_score as f64,
            },
            IntelligenceScoreComponent {
                label: "final_confidence".to_string(),
                value: explanation.final_confidence as f64,
            },
            IntelligenceScoreComponent {
                label: "preference_priority_delta".to_string(),
                value: explanation.preference_adjustment.priority_delta as f64,
            },
        ],
        timing_reason,
        suppression_reason,
        ranking_factors: vec![
            IntelligenceRankingFactor {
                label: "base_rank".to_string(),
                detail: explanation.base_rank.to_string(),
            },
            IntelligenceRankingFactor {
                label: "cluster_id".to_string(),
                detail: explanation.cluster_assignment.cluster_id.clone(),
            },
            IntelligenceRankingFactor {
                label: "preference_timing_delta".to_string(),
                detail: format!(
                    "{:.3}",
                    explanation.preference_adjustment.timing_confidence_delta
                ),
            },
        ],
    }
}

fn resolve_now_ts(request: &IntelligenceRequest) -> i64 {
    if let Some(reference) = request
        .context
        .reference_ts
        .as_deref()
        .and_then(parse_rfc3339_epoch)
    {
        return reference;
    }

    Utc::now().timestamp()
}

fn parse_rfc3339_epoch(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.timestamp())
}

fn map_action_safety(score: Option<f64>) -> ActionSafety {
    match score {
        Some(score) if score >= 0.75 => ActionSafety::Safe,
        Some(score) if score >= 0.40 => ActionSafety::Cautious,
        Some(_) => ActionSafety::Risky,
        None => ActionSafety::Safe,
    }
}

fn push_preference_signal(
    signals: &mut Vec<UserPreferenceSignal>,
    subject: PersonalizationSubject,
    kind: PreferenceSignalKind,
    count: u32,
) {
    if count > 0 {
        signals.push(UserPreferenceSignal::new(subject, kind, count));
    }
}

fn action_sequence_from_signature(signature: &str) -> Vec<String> {
    let actions: Vec<_> = signature
        .split("->")
        .filter_map(|part| part.split(':').next())
        .map(normalize_action_token)
        .filter(|action| !action.is_empty())
        .collect();

    if actions.is_empty() {
        vec!["workflow".to_string()]
    } else {
        actions
    }
}

fn normalize_action_token(token: &str) -> String {
    let mut normalized = String::new();
    let mut previous_was_separator = false;

    for ch in token.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() && !normalized.is_empty() && !previous_was_separator {
                normalized.push('-');
            }
            normalized.push(ch.to_ascii_lowercase());
            previous_was_separator = false;
        } else if !normalized.is_empty() && !previous_was_separator {
            normalized.push('-');
            previous_was_separator = true;
        }
    }

    normalized.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intelligence_boundary::{
        IntelligenceCandidateInput, IntelligenceEvaluationContext, InternalPatternMetadata,
        InternalRecencySignals, InternalSuggestionHistory, InternalSuggestionRecord,
    };

    fn sample_request() -> IntelligenceRequest {
        IntelligenceRequest {
            context: IntelligenceEvaluationContext {
                reference_ts: Some("2026-03-13T12:00:00Z".to_string()),
                candidate_count: 1,
                ..Default::default()
            },
            candidates: vec![IntelligenceCandidateInput {
                pattern_signature: "CreateFile:invoice->RenameFile:invoice->MoveFile:invoice"
                    .to_string(),
                suggestion: InternalSuggestionRecord {
                    pattern_signature: "CreateFile:invoice->RenameFile:invoice->MoveFile:invoice"
                        .to_string(),
                    canonical_summary: "CreateFile -> RenameFile -> MoveFile".to_string(),
                    baseline_proposal_text: "Organize invoice PDFs".to_string(),
                    usefulness_score: 0.82,
                    count: 4,
                    avg_duration_ms: 45_000,
                    last_seen_at: "2026-03-13T11:00:00Z".to_string(),
                    created_at: Some("2026-03-10T09:00:00Z".to_string()),
                },
                history: InternalSuggestionHistory {
                    shown_count: 1,
                    accepted_count: 0,
                    rejected_count: 0,
                    snoozed_count: 0,
                    ..Default::default()
                },
                pattern: InternalPatternMetadata {
                    canonical_summary: "CreateFile -> RenameFile -> MoveFile".to_string(),
                    count: 4,
                    avg_duration_ms: 45_000,
                    safety_score: Some(0.9),
                },
                recency: InternalRecencySignals {
                    reference_ts: Some("2026-03-13T12:00:00Z".to_string()),
                    seconds_since_last_seen: Some(3_600),
                    seconds_since_created: Some(259_200),
                    seconds_since_last_shown: Some(86_400),
                    ..Default::default()
                },
            }],
        }
    }

    #[test]
    fn private_client_returns_decisions_for_all_candidates() {
        let client = PrivateIntelligenceClient::default();
        let response = client.evaluate(&sample_request()).unwrap();
        assert_eq!(response.decisions.len(), 1);
        assert_eq!(
            response.decisions[0].pattern_signature,
            "CreateFile:invoice->RenameFile:invoice->MoveFile:invoice"
        );
        assert!(response.decisions[0].explanation.is_some());
    }
}
