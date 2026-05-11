//! Anthropic-backed "LLM as judge" for token-bench grading.
//!
//! Takes `(task rubric, candidate output)` and returns a 0..=5
//! quality score plus a one-line justification. Uses the Messages
//! API directly — no `anthropic-sdk` dep — so this module stays
//! thin and testable without the full SDK.
//!
//! # Why in-tree
//!
//! The token-bench scoreboard on `/bench` renders `mean_quality`
//! as `—` until every sample is graded. Grading is the missing
//! piece that takes the page from "stub ratio" to "real claim".
//! Keeping the judge local to `nexus-eval` makes the benchmark
//! runner self-contained — CI runs the bench binary and gets a
//! complete scoreboard out.
//!
//! # Calibration
//!
//! The judge prompt is intentionally narrow. The model sees only:
//!   - the rubric (what 5/5 means for this task)
//!   - the candidate output (may be a full diff, a single reply, or
//!     a summary)
//!
//! It must respond with strict JSON `{"score": 0..5, "note": "..."}`
//! so parsing is trivial. The 0..5 range matches the existing
//! `TaskResult::quality: Option<u8>` field so the aggregated
//! scoreboard can consume it without a new type.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Configuration for the Anthropic judge.
#[derive(Debug, Clone)]
pub struct JudgeConfig {
    pub api_key: String,
    pub model: String,
    pub api_base: String,
    pub timeout: Duration,
}

impl JudgeConfig {
    /// Read standard env vars. Returns `None` if no key is set —
    /// callers then skip grading instead of erroring out so the
    /// bench still renders in offline CI.
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok()?;
        if api_key.trim().is_empty() {
            return None;
        }
        let model = std::env::var("NEXUS_JUDGE_MODEL")
            .unwrap_or_else(|_| "claude-sonnet-4-5-20250929".to_string());
        let api_base = std::env::var("ANTHROPIC_API_BASE")
            .unwrap_or_else(|_| "https://api.anthropic.com/v1".to_string());
        Some(Self {
            api_key,
            model,
            api_base,
            timeout: Duration::from_secs(90),
        })
    }
}

/// One grading decision from the judge.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Grade {
    /// 0..=5, where 5 means the rubric was fully satisfied.
    pub score: u8,
    /// One short sentence the judge wrote to justify the score.
    pub note: String,
}

impl Grade {
    /// Guard against model drift. Clamp to 0..=5 and truncate any
    /// novel-length justification to 280 chars.
    pub fn normalised(mut self) -> Self {
        if self.score > 5 {
            self.score = 5;
        }
        if self.note.chars().count() > 280 {
            self.note = self.note.chars().take(280).collect::<String>() + "…";
        }
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum JudgeError {
    #[error("http: {0}")]
    Http(String),
    #[error("bad response from judge: {0}")]
    BadResponse(String),
    #[error("parse: {0}")]
    Parse(String),
}

/// Ask the judge to grade a single `(rubric, output)` pair.
pub async fn grade(
    client: &reqwest::Client,
    cfg: &JudgeConfig,
    rubric: &str,
    candidate_output: &str,
) -> Result<Grade, JudgeError> {
    let prompt = build_prompt(rubric, candidate_output);

    let url = format!("{}/messages", cfg.api_base.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": cfg.model,
        "max_tokens": 200,
        "system": SYSTEM_PROMPT,
        "messages": [
            { "role": "user", "content": prompt }
        ],
    });
    let resp = client
        .post(&url)
        .header("x-api-key", &cfg.api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| JudgeError::Http(format!("{e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(JudgeError::Http(format!("HTTP {status}: {text}")));
    }
    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| JudgeError::BadResponse(format!("{e}")))?;
    let text = data
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.iter().find(|b| b["type"] == "text"))
        .and_then(|b| b["text"].as_str())
        .ok_or_else(|| JudgeError::BadResponse("no text block".into()))?;
    let grade = parse_grade(text)?;
    Ok(grade.normalised())
}

const SYSTEM_PROMPT: &str = "You are a coding-task grader. Respond ONLY with a single-line JSON object of the form {\"score\": <int 0-5>, \"note\": \"<=1 sentence>\"}. Do not include prose, markdown, or extra fields. 5 = rubric fully satisfied; 0 = totally off-target. Be strict.";

fn build_prompt(rubric: &str, candidate_output: &str) -> String {
    format!(
        "RUBRIC:\n{rubric}\n\nCANDIDATE OUTPUT:\n{candidate_output}\n\nGrade strictly. JSON only."
    )
}

/// Extract a `{score, note}` JSON object from a model reply that
/// might include leading whitespace, surrounding prose, or a code
/// fence. Strict JSON-first, with a narrow fallback that scans for
/// the first balanced `{...}` substring.
pub(crate) fn parse_grade(raw: &str) -> Result<Grade, JudgeError> {
    let trimmed = raw.trim();
    if let Ok(g) = serde_json::from_str::<Grade>(trimmed) {
        return Ok(g);
    }
    // Find the first `{` and the matching `}` — a JSON object
    // inside a model reply is almost always the grade we asked
    // for. We don't attempt to handle nested braces because the
    // rubric schema is flat.
    let start = trimmed
        .find('{')
        .ok_or_else(|| JudgeError::Parse("no JSON object found".into()))?;
    let end = trimmed[start..]
        .find('}')
        .ok_or_else(|| JudgeError::Parse("unterminated JSON object".into()))?;
    let slice = &trimmed[start..=start + end];
    serde_json::from_str::<Grade>(slice)
        .map_err(|e| JudgeError::Parse(format!("{e}: {slice}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_json_reply() {
        let g = parse_grade(r#"{"score": 4, "note": "all cases covered"}"#).unwrap();
        assert_eq!(g.score, 4);
        assert_eq!(g.note, "all cases covered");
    }

    #[test]
    fn parses_json_wrapped_in_code_fence() {
        let raw = "```json\n{\"score\": 5, \"note\": \"clean\"}\n```";
        let g = parse_grade(raw).unwrap();
        assert_eq!(g.score, 5);
    }

    #[test]
    fn rejects_missing_braces() {
        assert!(parse_grade("score: 5").is_err());
    }

    #[test]
    fn normalises_score_above_range() {
        let g = Grade {
            score: 9,
            note: "beyond perfect".into(),
        }
        .normalised();
        assert_eq!(g.score, 5);
    }

    #[test]
    fn normalises_truncates_long_notes() {
        let g = Grade {
            score: 3,
            note: "x".repeat(500),
        }
        .normalised();
        assert!(g.note.chars().count() <= 281);
    }

    #[test]
    fn from_env_none_when_no_key() {
        // The test harness doesn't guarantee the env is clean, so
        // only assert behaviour when the var is clearly empty.
        if std::env::var("ANTHROPIC_API_KEY").is_err() {
            assert!(JudgeConfig::from_env().is_none());
        }
    }
}
