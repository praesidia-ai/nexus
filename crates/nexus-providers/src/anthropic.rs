//! Anthropic Claude `LlmProvider` implementation.
//!
//! Targets the Messages API at `/v1/messages`. Notable differences from
//! OpenAI: `system` is a top-level field (not a message role), tool calls
//! are content blocks, and prompt-caching uses `cache_control` markers.
//! The implementation here covers the standard non-cached path; the more
//! exotic prompt-caching machinery lives in `nexus-http::anthropic_cache`
//! for now and will move once tool-call paths consolidate.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::provider::LlmProvider;
use crate::types::{
    CompletionRequest, CompletionResponse, ModelInfo, Role, StreamChunk, TokenUsage, ToolCall,
    ToolCallDelta, ToolDefinition,
};
#[cfg(test)]
use crate::types::Message;

const DEFAULT_BASE: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";

#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    pub provider_id: String,
    pub base_url: String,
    pub api_key: String,
}

impl AnthropicConfig {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            provider_id: "anthropic".into(),
            base_url: DEFAULT_BASE.into(),
            api_key: api_key.into(),
        }
    }
}

pub struct AnthropicProvider {
    cfg: AnthropicConfig,
    client: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(cfg: AnthropicConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { cfg, client }
    }

    fn build_body(&self, req: &CompletionRequest, stream: bool) -> Value {
        // Pull system messages out of the messages array — Anthropic puts
        // `system` at the top level.
        let mut system: Option<String> = None;
        let mut messages: Vec<Value> = Vec::with_capacity(req.messages.len());
        for m in &req.messages {
            match m.role {
                Role::System => {
                    let next = match system.take() {
                        Some(prev) => format!("{prev}\n\n{}", m.content),
                        None => m.content.clone(),
                    };
                    system = Some(next);
                }
                Role::Tool => messages.push(serde_json::json!({
                    "role": "user",
                    "content": [{ "type": "tool_result", "content": m.content }],
                })),
                _ => messages.push(serde_json::json!({
                    "role": role_to_str(&m.role),
                    "content": m.content,
                })),
            }
        }

        let mut body = serde_json::json!({
            "model": req.model,
            "messages": messages,
            "max_tokens": req.max_tokens.unwrap_or(4096),
            "stream": stream,
        });
        if let Some(sys) = system {
            body["system"] = Value::String(sys);
        }
        if let Some(t) = req.temperature {
            body["temperature"] = serde_json::json!(t);
        }
        if let Some(stop) = &req.stop {
            body["stop_sequences"] = serde_json::json!(stop);
        }
        if let Some(tools) = &req.tools {
            body["tools"] = serde_json::json!(
                tools.iter().map(serialize_tool).collect::<Vec<_>>()
            );
        }
        body
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        &self.cfg.provider_id
    }

    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/v1/messages", self.cfg.base_url);
        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.cfg.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&self.build_body(request, false))
            .send()
            .await?;
        let status = resp.status();
        let body: Value = resp.json().await?;
        if !status.is_success() {
            return Err(format!("Anthropic {status}: {body}").into());
        }

        // Content is an array of blocks; concatenate text blocks, collect
        // tool_use blocks separately.
        let mut content_text = String::new();
        let mut tool_calls = Vec::new();
        if let Some(blocks) = body.get("content").and_then(|c| c.as_array()) {
            for block in blocks {
                let kind = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match kind {
                    "text" => {
                        if let Some(t) = block.get("text").and_then(|s| s.as_str()) {
                            content_text.push_str(t);
                        }
                    }
                    "tool_use" => {
                        let id = block
                            .get("id")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string();
                        let name = block
                            .get("name")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string();
                        let arguments = block.get("input").cloned().unwrap_or(Value::Null);
                        tool_calls.push(ToolCall {
                            id,
                            name,
                            arguments,
                        });
                    }
                    _ => {}
                }
            }
        }

        let usage = body
            .get("usage")
            .map(|u| TokenUsage {
                prompt_tokens: u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                completion_tokens: u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0)
                    as u32,
                total_tokens: (u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0)
                    + u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0))
                    as u32,
            })
            .unwrap_or_default();

        Ok(CompletionResponse {
            content: content_text,
            tool_calls,
            usage,
            model: body
                .get("model")
                .and_then(|m| m.as_str())
                .map(String::from)
                .unwrap_or_else(|| request.model.clone()),
            finish_reason: body
                .get("stop_reason")
                .and_then(|f| f.as_str())
                .map(String::from),
        })
    }

    async fn stream(
        &self,
        request: &CompletionRequest,
        tx: mpsc::Sender<StreamChunk>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Anthropic SSE event types: `content_block_delta`, `message_delta`,
        // `message_stop`. Text deltas appear under `delta.text`; tool_use
        // deltas appear under `delta.partial_json` paired with an opening
        // `content_block_start`.
        let url = format!("{}/v1/messages", self.cfg.base_url);
        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.cfg.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&self.build_body(request, true))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Anthropic {status}: {body}").into());
        }

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
                let v: Value = match serde_json::from_str(payload) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let kind = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match kind {
                    "content_block_delta" => {
                        if let Some(d) = v.get("delta") {
                            if let Some(t) = d.get("text").and_then(|s| s.as_str()) {
                                let _ = tx
                                    .send(StreamChunk {
                                        content: Some(t.to_string()),
                                        tool_call_delta: None,
                                        done: false,
                                    })
                                    .await;
                            } else if let Some(j) = d.get("partial_json").and_then(|s| s.as_str()) {
                                let _ = tx
                                    .send(StreamChunk {
                                        content: None,
                                        tool_call_delta: Some(ToolCallDelta {
                                            index: 0,
                                            id: None,
                                            name: None,
                                            arguments: Some(j.to_string()),
                                        }),
                                        done: false,
                                    })
                                    .await;
                            }
                        }
                    }
                    "message_stop" => {
                        let _ = tx
                            .send(StreamChunk {
                                content: None,
                                tool_call_delta: None,
                                done: true,
                            })
                            .await;
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
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
        // Static, conservative model list — Anthropic's discovery endpoint
        // is gated. Pricing reflects publicly listed Claude 3.5/4.x rates.
        Ok(vec![
            ModelInfo {
                id: "claude-opus-4-20250514".into(),
                provider: self.cfg.provider_id.clone(),
                display_name: "Claude Opus 4".into(),
                context_window: 200_000,
                max_output_tokens: Some(8_192),
                cost_per_m_input: 15.0,
                cost_per_m_output: 75.0,
                supports_tools: true,
                supports_streaming: true,
            },
            ModelInfo {
                id: "claude-sonnet-4-20250514".into(),
                provider: self.cfg.provider_id.clone(),
                display_name: "Claude Sonnet 4".into(),
                context_window: 200_000,
                max_output_tokens: Some(8_192),
                cost_per_m_input: 3.0,
                cost_per_m_output: 15.0,
                supports_tools: true,
                supports_streaming: true,
            },
        ])
    }

    async fn model_info(
        &self,
        model_id: &str,
    ) -> Result<Option<ModelInfo>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self
            .list_models()
            .await?
            .into_iter()
            .find(|m| m.id == model_id))
    }

    async fn health_check(&self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        // Anthropic doesn't expose a free-of-charge health endpoint; the
        // cheapest reliable check is a 1-token call. Skip the network call
        // and treat configured key as healthy — failures will surface on
        // the next real `complete`.
        Ok(!self.cfg.api_key.is_empty())
    }
}

fn role_to_str(role: &Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
        // System is hoisted out before this maps; Tool is rewritten as user.
        Role::System => "system",
        Role::Tool => "user",
    }
}

fn serialize_tool(t: &ToolDefinition) -> Value {
    serde_json::json!({
        "name": t.name,
        "description": t.description,
        "input_schema": t.parameters,
    })
}

impl From<AnthropicProvider> for Arc<dyn LlmProvider> {
    fn from(p: AnthropicProvider) -> Self {
        Arc::new(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_body_hoists_system_messages() {
        let p = AnthropicProvider::new(AnthropicConfig::new("dummy"));
        let req = CompletionRequest {
            model: "claude-sonnet-4-20250514".into(),
            messages: vec![
                Message {
                    role: Role::System,
                    content: "Be concise.".into(),
                },
                Message {
                    role: Role::User,
                    content: "Hi".into(),
                },
            ],
            ..Default::default()
        };
        let body = p.build_body(&req, false);
        assert_eq!(body["system"], "Be concise.");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "Hi");
    }

    #[test]
    fn build_body_concatenates_multiple_system_messages() {
        let p = AnthropicProvider::new(AnthropicConfig::new("dummy"));
        let req = CompletionRequest {
            model: "x".into(),
            messages: vec![
                Message {
                    role: Role::System,
                    content: "Rule 1".into(),
                },
                Message {
                    role: Role::System,
                    content: "Rule 2".into(),
                },
                Message {
                    role: Role::User,
                    content: "go".into(),
                },
            ],
            ..Default::default()
        };
        let body = p.build_body(&req, false);
        assert!(body["system"].as_str().unwrap().contains("Rule 1"));
        assert!(body["system"].as_str().unwrap().contains("Rule 2"));
    }
}
