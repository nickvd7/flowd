use flow_core::events::NormalizedEvent;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct PatternCandidate {
    pub signature: String,
    pub count: usize,
}

pub fn detect_repeated_patterns(sessions: &[Vec<NormalizedEvent>]) -> Vec<PatternCandidate> {
    let mut counts: HashMap<String, usize> = HashMap::new();

    for session in sessions {
        let signature = session
            .iter()
            .map(|e| format!("{:?}", e.action_type))
            .collect::<Vec<_>>()
            .join("->");
        *counts.entry(signature).or_insert(0) += 1;
    }

    counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(signature, count)| PatternCandidate { signature, count })
        .collect()
}
