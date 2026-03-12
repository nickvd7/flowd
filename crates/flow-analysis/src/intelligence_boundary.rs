use anyhow::Result;
use flow_db::repo::StoredSuggestion;
use flow_patterns::detect::PatternCandidate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// `flowd` is the open-core system engine. It owns facts and actions:
/// observed events, stored history, sessions, patterns, baseline suggestions,
/// approval, execution, and undo.
///
/// `flowd-intelligence` is optional. If present, it may only influence which
/// already-detected suggestions are shown, when they are shown, and how they
/// are phrased. The integration direction is one-way: open-core may call an
/// intelligence client, but private intelligence must not own or pull facts,
/// storage, execution, or undo into itself.
///
/// This module is the one explicit intelligence client boundary inside
/// `flowd`. All future integration with `flowd-intelligence` must pass
/// through these local DTOs and adapters so the rest of the repo stays free
/// of private intelligence contracts.
pub trait IntelligenceClient {
    fn evaluate(&self, request: &IntelligenceRequest) -> Result<IntelligenceResponse>;
}

#[derive(Debug, Clone, Default)]
pub struct NoopIntelligenceClient;

impl IntelligenceClient for NoopIntelligenceClient {
    fn evaluate(&self, _request: &IntelligenceRequest) -> Result<IntelligenceResponse> {
        Ok(IntelligenceResponse::default())
    }
}

/// Open-core suggestion data that may be mapped into a narrow intelligence
/// request. This remains owned by `flowd` and deliberately excludes storage
/// rows and execution details.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InternalSuggestionRecord {
    pub pattern_signature: String,
    pub canonical_summary: String,
    pub baseline_proposal_text: String,
    pub usefulness_score: f64,
    pub count: usize,
    pub avg_duration_ms: i64,
    pub last_seen_at: String,
    pub created_at: Option<String>,
}

/// Open-core interaction state exposed to intelligence. The baseline open-core
/// implementation stays fully functional without private intelligence, so this
/// state is deterministic and local even when richer history does not exist.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InternalSuggestionHistory {
    pub shown_count: u32,
    pub accepted_count: u32,
    pub rejected_count: u32,
    pub snoozed_count: u32,
    pub last_shown_ts: Option<String>,
    pub last_accepted_ts: Option<String>,
    pub last_rejected_ts: Option<String>,
    pub last_snoozed_ts: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InternalSuggestionContext {
    pub suggestion: InternalSuggestionRecord,
    pub history: InternalSuggestionHistory,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntelligenceRequest {
    pub candidates: Vec<IntelligenceCandidateInput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntelligenceCandidateInput {
    pub pattern_signature: String,
    pub suggestion: InternalSuggestionRecord,
    pub history: InternalSuggestionHistory,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IntelligenceResponse {
    pub decisions: Vec<IntelligenceDisplayDecision>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntelligenceDisplayDecision {
    pub pattern_signature: String,
    pub action: SuggestionDecisionAction,
    pub proposal_text: Option<String>,
    pub usefulness_score: Option<f64>,
    pub rank_hint: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SuggestionDecisionAction {
    Keep,
    Delay,
    Suppress,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SuggestionPresentation {
    pub pattern_signature: String,
    pub action: SuggestionDecisionAction,
    pub proposal_text: String,
    pub usefulness_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SuggestionDisplayResult {
    pub suggestion: StoredSuggestion,
    pub action: SuggestionDecisionAction,
}

/// The analysis layer should only invoke intelligence through this adapter so
/// the rest of open-core remains sufficient when no private client exists.
pub struct IntelligenceBoundary<'a> {
    client: &'a dyn IntelligenceClient,
}

impl<'a> IntelligenceBoundary<'a> {
    pub fn new(client: &'a dyn IntelligenceClient) -> Self {
        Self { client }
    }

    pub fn evaluate_patterns(
        &self,
        patterns: &[PatternCandidate],
    ) -> Result<Vec<SuggestionPresentation>> {
        let contexts = map_patterns_to_contexts(patterns);
        self.evaluate_contexts(&contexts)
    }

    pub fn evaluate_contexts(
        &self,
        contexts: &[InternalSuggestionContext],
    ) -> Result<Vec<SuggestionPresentation>> {
        let request = build_intelligence_request(contexts);
        let response = self.client.evaluate(&request)?;
        Ok(apply_intelligence_response(contexts, &response))
    }

    pub fn rank_suggestions(
        &self,
        suggestions: &[StoredSuggestion],
    ) -> Result<Vec<StoredSuggestion>> {
        let contexts = map_stored_suggestions_to_contexts(suggestions);
        let request = build_intelligence_request(&contexts);
        let response = self.client.evaluate(&request)?;
        Ok(apply_intelligence_ranking(suggestions, &response))
    }

    pub fn evaluate_stored_suggestions_for_display(
        &self,
        suggestions: &[StoredSuggestion],
    ) -> Result<Vec<SuggestionDisplayResult>> {
        let contexts = map_stored_suggestions_to_contexts(suggestions);
        let request = build_intelligence_request(&contexts);
        let response = self.client.evaluate(&request)?;
        Ok(apply_intelligence_display(suggestions, &response))
    }
}

pub fn map_patterns_to_contexts(patterns: &[PatternCandidate]) -> Vec<InternalSuggestionContext> {
    patterns
        .iter()
        .map(|pattern| InternalSuggestionContext {
            suggestion: InternalSuggestionRecord {
                pattern_signature: pattern.signature.clone(),
                canonical_summary: pattern.canonical_summary.clone(),
                baseline_proposal_text: pattern.proposal_text.clone(),
                usefulness_score: pattern.usefulness_score,
                count: pattern.count,
                avg_duration_ms: pattern.avg_duration_ms,
                last_seen_at: pattern.last_seen_at.to_rfc3339(),
                created_at: None,
            },
            history: InternalSuggestionHistory::default(),
        })
        .collect()
}

pub fn build_intelligence_request(contexts: &[InternalSuggestionContext]) -> IntelligenceRequest {
    IntelligenceRequest {
        candidates: contexts
            .iter()
            .map(|context| IntelligenceCandidateInput {
                pattern_signature: context.suggestion.pattern_signature.clone(),
                suggestion: context.suggestion.clone(),
                history: context.history.clone(),
            })
            .collect(),
    }
}

pub fn map_stored_suggestions_to_contexts(
    suggestions: &[StoredSuggestion],
) -> Vec<InternalSuggestionContext> {
    suggestions
        .iter()
        .map(|suggestion| InternalSuggestionContext {
            suggestion: InternalSuggestionRecord {
                pattern_signature: suggestion.signature.clone(),
                canonical_summary: suggestion.canonical_summary.clone(),
                baseline_proposal_text: suggestion.proposal_text.clone(),
                usefulness_score: suggestion.usefulness_score,
                count: suggestion.count,
                avg_duration_ms: suggestion.avg_duration_ms,
                last_seen_at: suggestion.last_seen_at.clone(),
                created_at: Some(suggestion.created_at.clone()),
            },
            history: InternalSuggestionHistory {
                shown_count: suggestion.shown_count,
                accepted_count: suggestion.accepted_count,
                rejected_count: suggestion.rejected_count,
                snoozed_count: suggestion.snoozed_count,
                last_shown_ts: suggestion.last_shown_ts.clone(),
                last_accepted_ts: suggestion.last_accepted_ts.clone(),
                last_rejected_ts: suggestion.last_rejected_ts.clone(),
                last_snoozed_ts: suggestion.last_snoozed_ts.clone(),
            },
        })
        .collect()
}

pub fn apply_intelligence_response(
    contexts: &[InternalSuggestionContext],
    response: &IntelligenceResponse,
) -> Vec<SuggestionPresentation> {
    let decisions: HashMap<&str, &IntelligenceDisplayDecision> = response
        .decisions
        .iter()
        .map(|decision| (decision.pattern_signature.as_str(), decision))
        .collect();

    contexts
        .iter()
        .map(|context| {
            let suggestion = &context.suggestion;
            let decision = decisions
                .get(suggestion.pattern_signature.as_str())
                .copied();

            SuggestionPresentation {
                pattern_signature: suggestion.pattern_signature.clone(),
                action: decision
                    .map(|value| value.action)
                    .unwrap_or(SuggestionDecisionAction::Keep),
                proposal_text: decision
                    .and_then(|value| value.proposal_text.clone())
                    .unwrap_or_else(|| suggestion.baseline_proposal_text.clone()),
                usefulness_score: decision
                    .and_then(|value| value.usefulness_score)
                    .unwrap_or(suggestion.usefulness_score),
            }
        })
        .collect()
}

pub fn apply_intelligence_display(
    suggestions: &[StoredSuggestion],
    response: &IntelligenceResponse,
) -> Vec<SuggestionDisplayResult> {
    let decisions: HashMap<&str, &IntelligenceDisplayDecision> = response
        .decisions
        .iter()
        .map(|decision| (decision.pattern_signature.as_str(), decision))
        .collect();
    let mut displayed: Vec<(usize, usize, SuggestionDisplayResult)> = suggestions
        .iter()
        .enumerate()
        .map(|(index, suggestion)| {
            let decision = decisions.get(suggestion.signature.as_str()).copied();
            let rank_hint = decision.and_then(|value| value.rank_hint).unwrap_or(index);
            let action = decision
                .map(|value| value.action)
                .unwrap_or(SuggestionDecisionAction::Keep);

            let mut suggestion = suggestion.clone();
            if let Some(proposal_text) = decision.and_then(|value| value.proposal_text.clone()) {
                suggestion.proposal_text = proposal_text;
            }
            if let Some(usefulness_score) = decision.and_then(|value| value.usefulness_score) {
                suggestion.usefulness_score = usefulness_score;
            }

            (
                rank_hint,
                index,
                SuggestionDisplayResult { suggestion, action },
            )
        })
        .collect();

    displayed.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));

    displayed
        .into_iter()
        .map(|(_, _, suggestion)| suggestion)
        .collect()
}

pub fn apply_intelligence_ranking(
    suggestions: &[StoredSuggestion],
    response: &IntelligenceResponse,
) -> Vec<StoredSuggestion> {
    let decisions: HashMap<&str, &IntelligenceDisplayDecision> = response
        .decisions
        .iter()
        .map(|decision| (decision.pattern_signature.as_str(), decision))
        .collect();
    let mut ranked: Vec<(usize, usize, &StoredSuggestion)> = suggestions
        .iter()
        .enumerate()
        .map(|(index, suggestion)| {
            let rank_hint = decisions
                .get(suggestion.signature.as_str())
                .and_then(|decision| decision.rank_hint)
                .unwrap_or(index);
            (rank_hint, index, suggestion)
        })
        .collect();

    ranked.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));

    ranked
        .into_iter()
        .map(|(_, _, suggestion)| suggestion.clone())
        .collect()
}

pub fn rank_stored_suggestions(
    suggestions: &[StoredSuggestion],
    client: &dyn IntelligenceClient,
) -> Result<Vec<StoredSuggestion>> {
    IntelligenceBoundary::new(client).rank_suggestions(suggestions)
}

pub fn display_stored_suggestions(
    suggestions: &[StoredSuggestion],
    client: &dyn IntelligenceClient,
) -> Result<Vec<StoredSuggestion>> {
    Ok(IntelligenceBoundary::new(client)
        .evaluate_stored_suggestions_for_display(suggestions)?
        .into_iter()
        .filter(|result| result.action == SuggestionDecisionAction::Keep)
        .map(|result| result.suggestion)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn pattern(signature: &str, proposal_text: &str) -> PatternCandidate {
        PatternCandidate {
            signature: signature.to_string(),
            count: 3,
            avg_duration_ms: 12_000,
            canonical_summary: "CreateFile -> RenameFile -> MoveFile".to_string(),
            proposal_text: proposal_text.to_string(),
            last_seen_at: Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 0).unwrap(),
            safety_score: 1.0,
            usefulness_score: 0.812,
        }
    }

    struct RefineClient;

    impl IntelligenceClient for RefineClient {
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

    fn stored_suggestion(signature: &str, score: f64, created_at: &str) -> StoredSuggestion {
        StoredSuggestion {
            suggestion_id: if signature.ends_with('a') { 1 } else { 2 },
            pattern_id: if signature.ends_with('a') { 11 } else { 12 },
            signature: signature.to_string(),
            count: 3,
            avg_duration_ms: 12_000,
            canonical_summary: "CreateFile -> RenameFile -> MoveFile".to_string(),
            proposal_text: format!("Proposal for {signature}"),
            usefulness_score: score,
            freshness: "current".to_string(),
            last_seen_at: "2026-01-15T10:00:00+00:00".to_string(),
            created_at: created_at.to_string(),
            shown_count: 4,
            accepted_count: 1,
            rejected_count: 2,
            snoozed_count: 3,
            last_shown_ts: Some("2026-01-16T10:00:00+00:00".to_string()),
            last_accepted_ts: Some("2026-01-17T10:00:00+00:00".to_string()),
            last_rejected_ts: Some("2026-01-18T10:00:00+00:00".to_string()),
            last_snoozed_ts: Some("2026-01-19T10:00:00+00:00".to_string()),
        }
    }

    #[test]
    fn maps_internal_suggestion_data_into_narrow_client_inputs() {
        let contexts = map_patterns_to_contexts(&[pattern("CreateFile:invoice", "baseline")]);
        let request = build_intelligence_request(&contexts);

        assert_eq!(request.candidates.len(), 1);
        assert_eq!(
            request.candidates[0].pattern_signature,
            "CreateFile:invoice"
        );
        assert_eq!(
            request.candidates[0].suggestion.baseline_proposal_text,
            "baseline"
        );
        assert_eq!(
            request.candidates[0].suggestion.last_seen_at,
            "2026-01-15T10:00:00+00:00"
        );
        assert_eq!(request.candidates[0].suggestion.created_at, None);
        assert_eq!(
            request.candidates[0].history,
            InternalSuggestionHistory::default()
        );
    }

    #[test]
    fn mapping_stays_deterministic() {
        let patterns = vec![
            pattern("CreateFile:invoice", "baseline"),
            pattern("CreateFile:report", "baseline report"),
        ];

        let first = build_intelligence_request(&map_patterns_to_contexts(&patterns));
        let second = build_intelligence_request(&map_patterns_to_contexts(&patterns));

        assert_eq!(first, second);
    }

    #[test]
    fn maps_stored_suggestions_into_ranking_inputs() {
        let contexts = map_stored_suggestions_to_contexts(&[stored_suggestion(
            "CreateFile:invoice-a",
            0.812,
            "2026-01-15T10:00:00+00:00",
        )]);

        assert_eq!(contexts.len(), 1);
        assert_eq!(
            contexts[0].suggestion.pattern_signature,
            "CreateFile:invoice-a"
        );
        assert_eq!(
            contexts[0].suggestion.baseline_proposal_text,
            "Proposal for CreateFile:invoice-a"
        );
        assert_eq!(
            contexts[0].suggestion.created_at.as_deref(),
            Some("2026-01-15T10:00:00+00:00")
        );
        assert_eq!(
            contexts[0].history,
            InternalSuggestionHistory {
                shown_count: 4,
                accepted_count: 1,
                rejected_count: 2,
                snoozed_count: 3,
                last_shown_ts: Some("2026-01-16T10:00:00+00:00".to_string()),
                last_accepted_ts: Some("2026-01-17T10:00:00+00:00".to_string()),
                last_rejected_ts: Some("2026-01-18T10:00:00+00:00".to_string()),
                last_snoozed_ts: Some("2026-01-19T10:00:00+00:00".to_string()),
            }
        );
    }

    #[test]
    fn boundary_can_be_exercised_without_changing_baseline_behavior() {
        let contexts = map_patterns_to_contexts(&[pattern("CreateFile:invoice", "baseline")]);
        let boundary = IntelligenceBoundary::new(&NoopIntelligenceClient);

        let presentations = boundary.evaluate_contexts(&contexts).unwrap();

        assert_eq!(presentations.len(), 1);
        assert_eq!(presentations[0].proposal_text, "baseline");
        assert_eq!(presentations[0].usefulness_score, 0.812);
        assert_eq!(presentations[0].action, SuggestionDecisionAction::Keep);
    }

    #[test]
    fn applies_keep_and_suppress_decisions_in_one_place() {
        let contexts = map_patterns_to_contexts(&[
            pattern("keep", "baseline keep"),
            pattern("suppress", "baseline suppress"),
        ]);
        let response = IntelligenceResponse {
            decisions: vec![
                IntelligenceDisplayDecision {
                    pattern_signature: "keep".to_string(),
                    action: SuggestionDecisionAction::Keep,
                    proposal_text: Some("refined keep".to_string()),
                    usefulness_score: Some(0.945),
                    rank_hint: None,
                },
                IntelligenceDisplayDecision {
                    pattern_signature: "suppress".to_string(),
                    action: SuggestionDecisionAction::Suppress,
                    proposal_text: None,
                    usefulness_score: Some(0.1),
                    rank_hint: None,
                },
            ],
        };

        let applied = apply_intelligence_response(&contexts, &response);

        assert_eq!(applied[0].proposal_text, "refined keep");
        assert_eq!(applied[0].usefulness_score, 0.945);
        assert_eq!(applied[0].action, SuggestionDecisionAction::Keep);
        assert_eq!(applied[1].proposal_text, "baseline suppress");
        assert_eq!(applied[1].usefulness_score, 0.1);
        assert_eq!(applied[1].action, SuggestionDecisionAction::Suppress);
    }

    #[test]
    fn boundary_can_apply_optional_client_refinements() {
        let boundary = IntelligenceBoundary::new(&RefineClient);

        let presentations = boundary
            .evaluate_patterns(&[pattern("CreateFile:invoice", "baseline")])
            .unwrap();

        assert_eq!(presentations[0].proposal_text, "Refined: baseline");
        assert!((presentations[0].usefulness_score - 0.862).abs() < f64::EPSILON);
    }

    #[test]
    fn ranking_falls_back_to_baseline_order_without_intelligence() {
        let suggestions = vec![
            stored_suggestion("CreateFile:invoice-a", 0.9, "2026-01-15T10:00:00+00:00"),
            stored_suggestion("CreateFile:invoice-b", 0.8, "2026-01-15T10:05:00+00:00"),
        ];

        let ranked = rank_stored_suggestions(&suggestions, &NoopIntelligenceClient).unwrap();

        assert_eq!(ranked, suggestions);
    }

    #[test]
    fn ranking_uses_intelligence_rank_hints() {
        struct RankingClient;

        impl IntelligenceClient for RankingClient {
            fn evaluate(&self, request: &IntelligenceRequest) -> Result<IntelligenceResponse> {
                Ok(IntelligenceResponse {
                    decisions: request
                        .candidates
                        .iter()
                        .map(|candidate| IntelligenceDisplayDecision {
                            pattern_signature: candidate.pattern_signature.clone(),
                            action: SuggestionDecisionAction::Keep,
                            proposal_text: None,
                            usefulness_score: None,
                            rank_hint: Some(if candidate.pattern_signature.ends_with('b') {
                                0
                            } else {
                                1
                            }),
                        })
                        .collect(),
                })
            }
        }

        let suggestions = vec![
            stored_suggestion("CreateFile:invoice-a", 0.9, "2026-01-15T10:00:00+00:00"),
            stored_suggestion("CreateFile:invoice-b", 0.8, "2026-01-15T10:05:00+00:00"),
        ];

        let first = rank_stored_suggestions(&suggestions, &RankingClient).unwrap();
        let second = rank_stored_suggestions(&suggestions, &RankingClient).unwrap();

        assert_eq!(first, second);
        assert_eq!(first[0].signature, "CreateFile:invoice-b");
        assert_eq!(first[1].signature, "CreateFile:invoice-a");
    }

    #[test]
    fn display_evaluation_can_delay_and_reword_suggestions() {
        struct DisplayClient;

        impl IntelligenceClient for DisplayClient {
            fn evaluate(&self, request: &IntelligenceRequest) -> Result<IntelligenceResponse> {
                Ok(IntelligenceResponse {
                    decisions: request
                        .candidates
                        .iter()
                        .map(|candidate| {
                            let action = if candidate.pattern_signature.ends_with('b') {
                                SuggestionDecisionAction::Delay
                            } else {
                                SuggestionDecisionAction::Keep
                            };

                            IntelligenceDisplayDecision {
                                pattern_signature: candidate.pattern_signature.clone(),
                                action,
                                proposal_text: Some(format!(
                                    "Display: {}",
                                    candidate.suggestion.baseline_proposal_text
                                )),
                                usefulness_score: None,
                                rank_hint: Some(if candidate.pattern_signature.ends_with('b') {
                                    0
                                } else {
                                    1
                                }),
                            }
                        })
                        .collect(),
                })
            }
        }

        let suggestions = vec![
            stored_suggestion("CreateFile:invoice-a", 0.9, "2026-01-15T10:00:00+00:00"),
            stored_suggestion("CreateFile:invoice-b", 0.8, "2026-01-15T10:05:00+00:00"),
        ];

        let evaluated = IntelligenceBoundary::new(&DisplayClient)
            .evaluate_stored_suggestions_for_display(&suggestions)
            .unwrap();

        assert_eq!(evaluated[0].suggestion.signature, "CreateFile:invoice-b");
        assert_eq!(evaluated[0].action, SuggestionDecisionAction::Delay);
        assert_eq!(
            evaluated[0].suggestion.proposal_text,
            "Display: Proposal for CreateFile:invoice-b"
        );
        assert_eq!(evaluated[1].action, SuggestionDecisionAction::Keep);
    }

    #[test]
    fn display_stored_suggestions_filters_out_delayed_and_suppressed_entries() {
        struct DisplayClient;

        impl IntelligenceClient for DisplayClient {
            fn evaluate(&self, request: &IntelligenceRequest) -> Result<IntelligenceResponse> {
                Ok(IntelligenceResponse {
                    decisions: request
                        .candidates
                        .iter()
                        .map(|candidate| IntelligenceDisplayDecision {
                            pattern_signature: candidate.pattern_signature.clone(),
                            action: if candidate.pattern_signature.ends_with('a') {
                                SuggestionDecisionAction::Keep
                            } else if candidate.pattern_signature.ends_with('b') {
                                SuggestionDecisionAction::Delay
                            } else {
                                SuggestionDecisionAction::Suppress
                            },
                            proposal_text: Some(format!(
                                "Display: {}",
                                candidate.suggestion.baseline_proposal_text
                            )),
                            usefulness_score: None,
                            rank_hint: Some(if candidate.pattern_signature.ends_with('a') {
                                0
                            } else {
                                1
                            }),
                        })
                        .collect(),
                })
            }
        }

        let suggestions = vec![
            stored_suggestion("CreateFile:invoice-a", 0.9, "2026-01-15T10:00:00+00:00"),
            stored_suggestion("CreateFile:invoice-b", 0.8, "2026-01-15T10:05:00+00:00"),
            stored_suggestion("CreateFile:invoice-c", 0.7, "2026-01-15T10:10:00+00:00"),
        ];

        let displayed = display_stored_suggestions(&suggestions, &DisplayClient).unwrap();

        assert_eq!(displayed.len(), 1);
        assert_eq!(displayed[0].signature, "CreateFile:invoice-a");
        assert_eq!(
            displayed[0].proposal_text,
            "Display: Proposal for CreateFile:invoice-a"
        );
    }

    #[test]
    fn display_stored_suggestions_fall_back_without_intelligence() {
        let suggestions = vec![
            stored_suggestion("CreateFile:invoice-a", 0.9, "2026-01-15T10:00:00+00:00"),
            stored_suggestion("CreateFile:invoice-b", 0.8, "2026-01-15T10:05:00+00:00"),
        ];

        let displayed = display_stored_suggestions(&suggestions, &NoopIntelligenceClient).unwrap();

        assert_eq!(displayed, suggestions);
    }
}
