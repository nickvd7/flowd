use anyhow::Result;
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
}

/// Open-core interaction state exposed to intelligence. The baseline open-core
/// implementation stays fully functional without private intelligence, so this
/// state is deterministic and local even when richer history does not exist.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InternalSuggestionHistory {
    pub approved_count: u32,
    pub rejected_count: u32,
    pub snoozed_count: u32,
    pub last_action_at: Option<String>,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SuggestionDecisionAction {
    Keep,
    Suppress,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SuggestionPresentation {
    pub pattern_signature: String,
    pub proposal_text: String,
    pub usefulness_score: f64,
    pub suppressed: bool,
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
            let suppressed = matches!(
                decision.map(|value| value.action),
                Some(SuggestionDecisionAction::Suppress)
            );

            SuggestionPresentation {
                pattern_signature: suggestion.pattern_signature.clone(),
                proposal_text: decision
                    .and_then(|value| value.proposal_text.clone())
                    .unwrap_or_else(|| suggestion.baseline_proposal_text.clone()),
                usefulness_score: decision
                    .and_then(|value| value.usefulness_score)
                    .unwrap_or(suggestion.usefulness_score),
                suppressed,
            }
        })
        .collect()
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
                    })
                    .collect(),
            })
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
    fn boundary_can_be_exercised_without_changing_baseline_behavior() {
        let contexts = map_patterns_to_contexts(&[pattern("CreateFile:invoice", "baseline")]);
        let boundary = IntelligenceBoundary::new(&NoopIntelligenceClient);

        let presentations = boundary.evaluate_contexts(&contexts).unwrap();

        assert_eq!(presentations.len(), 1);
        assert_eq!(presentations[0].proposal_text, "baseline");
        assert_eq!(presentations[0].usefulness_score, 0.812);
        assert!(!presentations[0].suppressed);
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
                },
                IntelligenceDisplayDecision {
                    pattern_signature: "suppress".to_string(),
                    action: SuggestionDecisionAction::Suppress,
                    proposal_text: None,
                    usefulness_score: Some(0.1),
                },
            ],
        };

        let applied = apply_intelligence_response(&contexts, &response);

        assert_eq!(applied[0].proposal_text, "refined keep");
        assert_eq!(applied[0].usefulness_score, 0.945);
        assert!(!applied[0].suppressed);
        assert_eq!(applied[1].proposal_text, "baseline suppress");
        assert_eq!(applied[1].usefulness_score, 0.1);
        assert!(applied[1].suppressed);
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
}
