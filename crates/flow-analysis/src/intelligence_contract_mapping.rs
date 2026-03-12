use crate::intelligence_boundary::{
    IntelligenceCandidate, IntelligenceDecision, IntelligenceRequest, IntelligenceResponse,
    SuggestionDecisionAction,
};
use flow_patterns::detect::PatternCandidate;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct SuggestionPresentation {
    pub pattern_signature: String,
    pub proposal_text: String,
    pub usefulness_score: f64,
    pub suppressed: bool,
}

pub fn build_intelligence_request(patterns: &[PatternCandidate]) -> IntelligenceRequest {
    IntelligenceRequest {
        candidates: patterns
            .iter()
            .map(|pattern| IntelligenceCandidate {
                pattern_signature: pattern.signature.clone(),
                canonical_summary: pattern.canonical_summary.clone(),
                baseline_proposal_text: pattern.proposal_text.clone(),
                usefulness_score: pattern.usefulness_score,
                count: pattern.count,
                avg_duration_ms: pattern.avg_duration_ms,
                last_seen_at: pattern.last_seen_at.to_rfc3339(),
            })
            .collect(),
    }
}

pub fn apply_intelligence_response(
    patterns: &[PatternCandidate],
    response: &IntelligenceResponse,
) -> Vec<SuggestionPresentation> {
    let decisions: HashMap<&str, &IntelligenceDecision> = response
        .decisions
        .iter()
        .map(|decision| (decision.pattern_signature.as_str(), decision))
        .collect();

    patterns
        .iter()
        .map(|pattern| {
            let decision = decisions.get(pattern.signature.as_str()).copied();
            let suppressed = matches!(
                decision.map(|value| value.action),
                Some(SuggestionDecisionAction::Suppress)
            );

            SuggestionPresentation {
                pattern_signature: pattern.signature.clone(),
                proposal_text: decision
                    .and_then(|value| value.proposal_text.clone())
                    .unwrap_or_else(|| pattern.proposal_text.clone()),
                usefulness_score: decision
                    .and_then(|value| value.usefulness_score)
                    .unwrap_or(pattern.usefulness_score),
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

    #[test]
    fn builds_narrow_deterministic_contract_inputs() {
        let request = build_intelligence_request(&[pattern("CreateFile:invoice", "baseline")]);

        assert_eq!(request.candidates.len(), 1);
        assert_eq!(
            request.candidates[0].pattern_signature,
            "CreateFile:invoice"
        );
        assert_eq!(request.candidates[0].baseline_proposal_text, "baseline");
        assert_eq!(
            request.candidates[0].last_seen_at,
            "2026-01-15T10:00:00+00:00"
        );
    }

    #[test]
    fn applies_keep_and_suppress_decisions_in_one_place() {
        let patterns = vec![
            pattern("keep", "baseline keep"),
            pattern("suppress", "baseline suppress"),
        ];
        let response = IntelligenceResponse {
            decisions: vec![
                IntelligenceDecision {
                    pattern_signature: "keep".to_string(),
                    action: SuggestionDecisionAction::Keep,
                    proposal_text: Some("refined keep".to_string()),
                    usefulness_score: Some(0.945),
                },
                IntelligenceDecision {
                    pattern_signature: "suppress".to_string(),
                    action: SuggestionDecisionAction::Suppress,
                    proposal_text: None,
                    usefulness_score: Some(0.1),
                },
            ],
        };

        let applied = apply_intelligence_response(&patterns, &response);

        assert_eq!(applied[0].proposal_text, "refined keep");
        assert_eq!(applied[0].usefulness_score, 0.945);
        assert!(!applied[0].suppressed);
        assert_eq!(applied[1].proposal_text, "baseline suppress");
        assert_eq!(applied[1].usefulness_score, 0.1);
        assert!(applied[1].suppressed);
    }
}
