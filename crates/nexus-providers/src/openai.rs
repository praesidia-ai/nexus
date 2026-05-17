//! OpenAI-compatible `LlmProvider` implementation.
//!
//! Talks to the standard `/v1/chat/completions` endpoint (OpenAI, Groq,
//! Together, Cerebras, OpenRouter, and any other OpenAI-API-compatible
//! upstream all share this wire format).
//!
//! The stream endpoint emits `data: {...}` lines per SSE; this module
//! consumes those and yields [`StreamChunk`]s on the channel passed by
//! the caller.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::provider::LlmProvider;
use crate::types::{
    CompletionRequest, CompletionResponse, Message, ModelInfo, Role, StreamChunk, TokenUsage,
    ToolCall, ToolCallDelta, ToolDefinition,
};

const DEFAULT_BASE: &str = "https://api.openai.com/v1";

/// Configuration for an OpenAI-compatible provider.
#[derive(Debug, Clone)]
pub struct OpenAiConfig {
    /// Stable identifier surfaced via [`LlmProvider::name`]. Use a distinct
    /// value per upstream — e.g. `"openai"`, `"groq"`, `"openrouter"`.
    pub provider_id: String,
    /// API base URL. Defaults to `https://api.openai.com/v1`.
    pub base_url: String,
    /// Bearer token. Reads from the user's saved settings, NOT env at boot
    /// (per project memory rule on API keys).
    pub api_key: String,
    /// Optional `Organization` header.
    pub organization: Option<String>,
}

impl OpenAiConfig {
    pub fn new(provider_id: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            base_url: DEFAULT_BASE.into(),
            api_key: api_key.into(),
            organization: None,
        }
    }

    pub fn with_base_url(mut self, base: impl Into<String>) -> Self {
        self.base_url = base.into();
        self
    }
}

/// `LlmProvider` impl over an OpenAI-compatible chat completions endpoint.
pub struct OpenAiProvider {
    cfg: OpenAiConfig,
    client: reqwest::Client,
}

impl OpenAiProvider {
    pub fn new(cfg: OpenAiConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { cfg, client }
    }

    /// Convenience constructor for a default OpenAI client.
    pub fn openai(api_key: impl Into<String>) -> Self {
        Self::new(OpenAiConfig::new("openai", api_key))
    }

    fn build_body(&self, req: &CompletionRequest, stream: bool) -> Value {
        serde_json::json!({
            "model": req.model,
            "messages": req.messages.iter().map(serialize_message).collect::<Vec<_>>(),
            "max_tokens": req.max_tokens,
            "temperature": req.temperature,
            "stream": stream,
            "stop": req.stop,
            "tools": req.tools.as_ref().map(|ts| ts.iter().map(serialize_tool).collect::<Vec<_>>()),
        })
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        &self.cfg.provider_id
    }

    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/chat/completions", self.cfg.base_url);
        let mut req = self
            .client
            .post(&url)
            .bearer_auth(&self.cfg.api_key)
            .json(&self.build_body(request, false));
        if let Some(org) = &self.cfg.organization {
            req = req.header("OpenAI-Organization", org);
        }
        let resp = req.send().await?;
        let status = resp.status();
        let body: Value = resp.json().await?;
        if !status.is_success() {
            return Err(format!("OpenAI {status}: {body}").into());
        }

        let choice = body
            .get("choices")
            .and_then(|c| c.get(0))
            .ok_or_else(|| Box::<dyn std::error::Error + Send + Sync>::from(
                "OpenAI response missing choices[0]",
            ))?;
        let msg = choice.get("message").cloned().unwrap_or(Value::Null);
        let content = msg
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        let tool_calls = msg
            .get("tool_calls")
            .and_then(|tc| tc.as_array())
            .map(|arr| arr.iter().filter_map(parse_tool_call).collect())
            .unwrap_or_default();

        let usage = body
            .get("usage")
            .map(|u| TokenUsage {
                prompt_tokens: u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                completion_tokens: u.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0)
                    as u32,
                total_tokens: u.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            })
            .unwrap_or_default();

        Ok(CompletionResponse {
            content,
            tool_calls,
            usage,
            model: body
                .get("model")
                .and_then(|m| m.as_str())
                .map(String::from)
                .unwrap_or_else(|| request.model.clone()),
            finish_reason: choice
                .get("finish_reason")
                .and_then(|f| f.as_str())
                .map(String::from),
        })
    }

    async fn stream(
        &self,
        request: &CompletionRequest,
        tx: mpsc::Sender<StreamChunk>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/chat/completions", self.cfg.base_url);
        let mut req = self
            .client
            .post(&url)
            .bearer_auth(&self.cfg.api_key)
            .json(&self.build_body(request, true));
        if let Some(org) = &self.cfg.organization {
            req = req.header("OpenAI-Organization", org);
        }
        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("OpenAI {status}: {body}").into());
        }

        // SSE: each line `data: {json}` ends with `data: [DONE]`.
        use futures_util::StreamExt;
        let mut byte_stream = resp.bytes_stream();
        let mut buf = String::new();
        while let Some(chunk) = byte_stream.next().await {
            let chunk = chunk?;
            buf.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(idx) = buf.find('\n') {
                let line = buf[..idx].to_string();
                buf.drain(..=idx);
                let line = line.trim();
                if line.is_empty() || !line.starts_with("data: ") {
                    continue;
                }
                let payload = line.trim_start_matches("data: ");
                if payload == "[DONE]" {
                    let _ = tx
                        .send(StreamChunk {
                            content: None,
                            tool_call_delta: None,
                            done: true,
                        })
                        .await;
                    return Ok(());
                }
                let v: Value = match serde_json::from_str(payload) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let delta = v
                    .get("choices")
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.get("delta"))
                    .cloned()
                    .unwrap_or(Value::Null);
                let content = delta
                    .get("content")
                    .and_then(|c| c.as_str())
                    .map(String::from);
                let tool_call_delta = delta
                    .get("tool_calls")
                    .and_then(|tc| tc.as_array())
                    .and_then(|a| a.first())
                    .and_then(parse_tool_call_delta);
                if (content.is_some() || tool_call_delta.is_some())
                    && tx
                        .send(StreamChunk {
                            content,
                            tool_call_delta,
                            done: false,
                        })
                        .await
                        .is_err()
                {
                    return Ok(()); // receiver dropped
                }
            }
        }
        // Stream closed without [DONE] — emit a final done so the consumer
        // doesn't hang.
        let _ = tx
            .send(StreamChunk {
                content: None,
                tool_call_delta: None,
                done: true,
            })
            .await;
        Ok(())
    }

    async fn list_models(
        &self,
    ) -> Result<Vec<ModelInfo>, Box<dyn std::error::Error + Send + Sync>> {
        // The catalog endpoint exists, but pricing isn't returned — we keep
        // a static minimum here. Callers that need richer model info should
        // consult their own pricing table.
        Ok(vec![ModelInfo {
            id: "gpt-4o-mini".into(),
            provider: self.cfg.provider_id.clone(),
            display_name: "GPT-4o mini".into(),
            context_window: 128_000,
            max_output_tokens: Some(16_384),
            cost_per_m_input: 0.150,
            cost_per_m_output: 0.600,
            supports_tools: true,
            supports_streaming: true,
        }])
    }

    async fn model_info(
        &self,
        model_id: &str,
    ) -> Result<Option<ModelInfo>, Box<dyn std::error::Error + Send + Sync>> {
        let all = self.list_models().await?;
        Ok(all.into_iter().find(|m| m.id == model_id))
    }

    async fn health_check(&self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        // List models is the cheapest no-op call OpenAI exposes.
        let url = format!("{}/models", self.cfg.base_url);
        let resp = self.client.get(&url).bearer_auth(&self.cfg.api_key).send().await?;
        Ok(resp.status().is_success())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn role_to_str(role: &Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

fn serialize_message(m: &Message) -> Value {
    serde_json::json!({
        "role": role_to_str(&m.role),
        "content": m.content,
    })
}

fn serialize_tool(t: &ToolDefinition) -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": t.name,
            "description": t.description,
            "parameters": t.parameters,
        }
    })
}

fn parse_tool_call(v: &Value) -> Option<ToolCall> {
    let id = v.get("id")?.as_str()?.to_string();
    let func = v.get("function")?;
    let name = func.get("name")?.as_str()?.to_string();
    let args_raw = func.get("arguments").and_then(|a| a.as_str()).unwrap_or("{}");
    let arguments = serde_json::from_str::<Value>(args_raw).unwrap_or(Value::Null);
    Some(ToolCall {
        id,
        name,
        arguments,
    })
}

fn parse_tool_call_delta(v: &Value) -> Option<ToolCallDelta> {
    let index = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
    let id = v.get("id").and_then(|s| s.as_str()).map(String::from);
    let func = v.get("function");
    let name = func
        .and_then(|f| f.get("name"))
        .and_then(|s| s.as_str())
        .map(String::from);
    let arguments = func
        .and_then(|f| f.get("arguments"))
        .and_then(|s| s.as_str())
        .map(String::from);
    Some(ToolCallDelta {
        index,
        id,
        name,
        arguments,
    })
}

// Make Arc<OpenAiProvider> usable as Arc<dyn LlmProvider>.
impl From<OpenAiProvider> for Arc<dyn LlmProvider> {
    fn from(p: OpenAiProvider) -> Self {
        Arc::new(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_to_openai_base() {
        let c = OpenAiConfig::new("openai", "sk-x");
        assert_eq!(c.base_url, DEFAULT_BASE);
        assert_eq!(c.provider_id, "openai");
    }

    #[test]
    fn parse_tool_call_round_trip() {
        let v = serde_json::json!({
            "id": "call_123",
            "function": { "name": "lookup", "arguments": "{\"q\":\"x\"}" }
        });
        let tc = parse_tool_call(&v).unwrap();
        assert_eq!(tc.id, "call_123");
        assert_eq!(tc.name, "lookup");
        assert_eq!(tc.arguments, serde_json::json!({"q": "x"}));
    }

    #[test]
    fn build_body_omits_unset_fields() {
        let p = OpenAiProvider::openai("sk-x");
        let req = CompletionRequest {
            model: "gpt-4o-mini".into(),
            messages: vec![Message {
                role: Role::User,
                content: "hi".into(),
            }],
            ..Default::default()
        };
        let body = p.build_body(&req, false);
        assert_eq!(body["model"], "gpt-4o-mini");
        assert_eq!(body["stream"], false);
    }
}
