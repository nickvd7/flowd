//! Optional local-only LLM labeling helpers.
//!
//! Labels are metadata for display. They never execute automations or change
//! safety decisions. Remote endpoints are rejected by config validation.

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalLlmLabelRequest {
    pub endpoint: String,
    pub model: String,
    pub pattern_summary: String,
    pub proposal_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalLlmLabelResponse {
    pub label: String,
    pub source: &'static str,
}

#[derive(Debug, Deserialize)]
struct OllamaGenerateResponse {
    response: String,
}

/// Ask a local Ollama-compatible endpoint for a short workflow label.
///
/// Falls back to a deterministic heuristic when the endpoint is unavailable.
pub fn label_workflow(request: &LocalLlmLabelRequest) -> Result<LocalLlmLabelResponse> {
    match request_local_label(request) {
        Ok(label) if is_safe_label(&label) => Ok(LocalLlmLabelResponse {
            label,
            source: "local_llm",
        }),
        Ok(_) => Ok(LocalLlmLabelResponse {
            label: heuristic_label(request),
            source: "heuristic_fallback_invalid_model_output",
        }),
        Err(_) => Ok(LocalLlmLabelResponse {
            label: heuristic_label(request),
            source: "heuristic_fallback",
        }),
    }
}

fn request_local_label(request: &LocalLlmLabelRequest) -> Result<String> {
    let endpoint = request.endpoint.trim_end_matches('/');
    let url = format!("{endpoint}/api/generate");
    let prompt = format!(
        "Return only a short 3-6 word label for this local file workflow. No quotes.\nSummary: {}\nProposal: {}",
        request.pattern_summary, request.proposal_text
    );
    let body = json!({
        "model": request.model,
        "prompt": prompt,
        "stream": false,
        "options": { "temperature": 0.1, "num_predict": 24 }
    });

    let response = ureq::post(&url)
        .set("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(3))
        .send_json(body)
        .map_err(|error| anyhow!("local llm request failed: {error}"))?;
    let parsed: OllamaGenerateResponse = response
        .into_json()
        .context("failed to parse local llm response")?;
    Ok(parsed.response.trim().lines().next().unwrap_or("").to_string())
}

fn heuristic_label(request: &LocalLlmLabelRequest) -> String {
    let source = if request.proposal_text.trim().is_empty() {
        request.pattern_summary.as_str()
    } else {
        request.proposal_text.as_str()
    };
    let words: Vec<_> = source
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .take(5)
        .collect();
    if words.is_empty() {
        "Local file workflow".to_string()
    } else {
        words.join(" ")
    }
}

fn is_safe_label(label: &str) -> bool {
    let trimmed = label.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 80
        && !trimmed.contains('\n')
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '-' | '_' | '/' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heuristic_label_keeps_short_words() {
        let label = heuristic_label(&LocalLlmLabelRequest {
            endpoint: "http://127.0.0.1:11434".to_string(),
            model: "llama3.2".to_string(),
            pattern_summary: "CreateFile -> RenameFile".to_string(),
            proposal_text: "Organize invoice PDFs into archive".to_string(),
        });
        assert!(label.to_ascii_lowercase().contains("organize"));
    }

    #[test]
    fn rejects_unsafe_labels() {
        assert!(!is_safe_label(""));
        assert!(!is_safe_label("rm -rf /\nscary"));
        assert!(is_safe_label("Organize invoice PDFs"));
    }
}
