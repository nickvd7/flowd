use anyhow::Result;
use serde::{Deserialize, Serialize};

/// `flowd` is the open-core system engine. It owns facts and actions:
/// observed events, stored history, sessions, patterns, baseline suggestions,
/// approval, execution, and undo.
///
/// `flowd-intelligence` is optional. If present, it may only influence which
/// already-detected suggestions are shown, when they are shown, and how they
/// are phrased. The integration direction is one-way: open-core may call an
/// intelligence client, but private intelligence must not own or pull facts,
/// storage, execution, or undo into itself.
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntelligenceRequest {
    pub candidates: Vec<IntelligenceCandidate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntelligenceCandidate {
    pub pattern_signature: String,
    pub canonical_summary: String,
    pub baseline_proposal_text: String,
    pub usefulness_score: f64,
    pub count: usize,
    pub avg_duration_ms: i64,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IntelligenceResponse {
    pub decisions: Vec<IntelligenceDecision>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntelligenceDecision {
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
