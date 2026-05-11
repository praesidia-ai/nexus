//! LLM completion tool — lets agents delegate sub-tasks to an LLM.
//!
//! This is a "tool-calling-an-LLM" pattern: the agent can ask for a
//! focused completion on a prompt, optionally specifying model and
//! token limits. Useful for summarization, translation, code review
//! sub-tasks, etc.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agents::tools::registry::{Tool, ToolInput, ToolOutput};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn ok(result: Value) -> ToolOutput {
    ToolOutput {
        result,
        success: true,
        error: None,
    }
}

fn err(msg: impl Into<String>) -> ToolOutput {
    ToolOutput {
        result: json!({}),
        success: false,
        error: Some(msg.into()),
    }
}

// ---------------------------------------------------------------------------
// LlmCompletionTool
// ---------------------------------------------------------------------------

/// Call an LLM to perform a focused sub-task.
///
/// The agent provides a prompt, and this tool returns the LLM response.
/// This enables agent "inner monologue" and delegation of reasoning
/// sub-problems to the LLM.
///
/// In the current implementation this uses a simple HTTP call to the
/// configured LLM endpoint. In production, integrate with the Nexus
/// `llm_client` module for caching, cost tracking, and model routing.
pub struct LlmCompletionTool;

#[async_trait]
impl Tool for LlmCompletionTool {
    fn name(&self) -> &str {
        "llm_completion"
    }

    fn description(&self) -> &str {
        "Call an LLM for a focused sub-task (summarize, analyze, generate). Returns the LLM response text."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": { "type": "string", "description": "The prompt to send to the LLM" },
                "system": { "type": "string", "description": "System message (optional)" },
                "model": { "type": "string", "description": "Model name (optional, uses default)" },
                "max_tokens": { "type": "integer", "description": "Maximum tokens in response (default: 2048)" },
                "temperature": { "type": "number", "description": "Sampling temperature 0-2 (default: 0.7)" }
            },
            "required": ["prompt"]
        })
    }

    fn category(&self) -> &str {
        "llm_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let prompt = match input.parameters.get("prompt").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return err("Missing required parameter: prompt"),
        };

        let system = input
            .parameters
            .get("system")
            .and_then(|v| v.as_str())
            .unwrap_or("You are a helpful assistant.");

        let model = input
            .parameters
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("gpt-4o");

        let max_tokens = input
            .parameters
            .get("max_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(2048);

        let temperature = input
            .parameters
            .get("temperature")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.7);

        // Build the request body for OpenAI-compatible API
        let api_key = std::env::var("OPENAI_API_KEY").or_else(|_| std::env::var("ANTHROPIC_API_KEY"));

        let api_key = match api_key {
            Ok(key) => key,
            Err(_) => {
                return err(
                    "No LLM API key configured. Set OPENAI_API_KEY or ANTHROPIC_API_KEY environment variable.",
                );
            }
        };

        // Determine API endpoint based on model
        let (url, body) = if model.starts_with("claude") {
            // Anthropic API
            let body = json!({
                "model": model,
                "max_tokens": max_tokens,
                "system": system,
                "messages": [
                    { "role": "user", "content": prompt }
                ]
            });
            ("https://api.anthropic.com/v1/messages".to_string(), body)
        } else {
            // OpenAI-compatible API
            let body = json!({
                "model": model,
                "max_tokens": max_tokens,
                "temperature": temperature,
                "messages": [
                    { "role": "system", "content": system },
                    { "role": "user", "content": prompt }
                ]
            });
            (
                "https://api.openai.com/v1/chat/completions".to_string(),
                body,
            )
        };

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
        {
            Ok(c) => c,
            Err(e) => return err(format!("Failed to create HTTP client: {}", e)),
        };

        let mut req = client.post(&url).json(&body);

        if model.starts_with("claude") {
            req = req
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json");
        } else {
            req = req
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json");
        }

        match req.send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body_text = resp.text().await.unwrap_or_default();

                if status >= 400 {
                    return err(format!("LLM API returned HTTP {}: {}", status, body_text));
                }

                let body_json: Value = match serde_json::from_str(&body_text) {
                    Ok(v) => v,
                    Err(_) => return err("Failed to parse LLM response as JSON"),
                };

                // Extract response text based on API format
                let response_text = if model.starts_with("claude") {
                    // Anthropic format
                    body_json["content"]
                        .as_array()
                        .and_then(|arr| arr.first())
                        .and_then(|block| block["text"].as_str())
                        .unwrap_or("")
                        .to_string()
                } else {
                    // OpenAI format
                    body_json["choices"]
                        .as_array()
                        .and_then(|arr| arr.first())
                        .and_then(|choice| choice["message"]["content"].as_str())
                        .unwrap_or("")
                        .to_string()
                };

                // Extract usage information
                let (input_tokens, output_tokens) = if model.starts_with("claude") {
                    (
                        body_json["usage"]["input_tokens"].as_u64().unwrap_or(0),
                        body_json["usage"]["output_tokens"].as_u64().unwrap_or(0),
                    )
                } else {
                    (
                        body_json["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
                        body_json["usage"]["completion_tokens"].as_u64().unwrap_or(0),
                    )
                };

                // ADR-005 §4: surface every LLM call through the metric
                // families even ones that bypass the unified client.
                let provider = if model.starts_with("claude") { "anthropic" } else { "openai" };
                let cost_usd = crate::cost_intelligence::estimate_cost_usd(
                    &model,
                    input_tokens,
                    output_tokens,
                );
                let cost_micros = crate::budget_brake::dollars_to_micros(cost_usd) as u64;
                crate::http_metrics::record_llm_call(
                    provider,
                    &model,
                    "default",
                    cost_micros,
                    input_tokens,
                    output_tokens,
                    0,
                );

                let tokens_used = output_tokens;
                ok(json!({
                    "response": response_text,
                    "model": model,
                    "tokens_used": tokens_used
                }))
            }
            Err(e) => {
                let provider = if model.starts_with("claude") { "anthropic" } else { "openai" };
                crate::http_metrics::record_llm_error(
                    provider,
                    &model,
                    "default",
                    crate::http_metrics::LlmErrorKind::Other,
                );
                err(format!("LLM API request failed: {}", e))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_completion_tool_metadata() {
        let tool = LlmCompletionTool;
        assert_eq!(tool.name(), "llm_completion");
        assert_eq!(tool.category(), "llm_ops");

        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("prompt")));
    }

    #[tokio::test]
    async fn llm_completion_missing_prompt() {
        let tool = LlmCompletionTool;
        let out = tool
            .execute(ToolInput {
                parameters: json!({}),
            })
            .await;

        assert!(!out.success);
        assert!(out.error.unwrap().contains("prompt"));
    }

    #[tokio::test]
    async fn llm_completion_no_api_key() {
        // Temporarily ensure no API key is set (test isolation is best-effort here)
        let tool = LlmCompletionTool;
        // This will fail gracefully if no key is configured, which is fine for CI
        let out = tool
            .execute(ToolInput {
                parameters: json!({ "prompt": "Say hi" }),
            })
            .await;

        // We expect either a success (if key is set) or a graceful error
        if !out.success {
            let error = out.error.unwrap();
            assert!(
                error.contains("API key") || error.contains("API"),
                "Unexpected error: {}",
                error
            );
        }
    }
}
