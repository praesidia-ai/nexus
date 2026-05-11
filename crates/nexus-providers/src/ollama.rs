//! Ollama `LlmProvider` implementation.
//!
//! Local-first, no API key. Targets the `/api/chat` endpoint of an Ollama
//! daemon (default `http://localhost:11434`). Streaming uses Ollama's
//! newline-delimited JSON format (one JSON object per line, terminated by
//! a `done: true` object).

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::provider::LlmProvider;
use crate::types::{
    CompletionRequest, CompletionResponse, Message, ModelInfo, Role, StreamChunk, TokenUsage,
};

const DEFAULT_BASE: &str = "http://localhost:11434";

#[derive(Debug, Clone)]
pub struct OllamaConfig {
    pub base_url: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE.into(),
        }
    }
}

pub struct OllamaProvider {
    cfg: OllamaConfig,
    client: reqwest::Client,
}

impl OllamaProvider {
    pub fn new(cfg: OllamaConfig) -> Self {
        // Ollama runs locally; longer timeout for large models cold-loading.
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { cfg, client }
    }

    fn build_body(&self, req: &CompletionRequest, stream: bool) -> Value {
        serde_json::json!({
            "model": req.model,
            "messages": req.messages.iter().map(serialize_message).collect::<Vec<_>>(),
            "stream": stream,
            "options": {
                "temperature": req.temperature,
                "num_predict": req.max_tokens,
                "stop": req.stop,
            }
        })
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/api/chat", self.cfg.base_url);
        let resp = self
            .client
            .post(&url)
            .json(&self.build_body(request, false))
            .send()
            .await?;
        let status = resp.status();
        let body: Value = resp.json().await?;
        if !status.is_success() {
            return Err(format!("Ollama {status}: {body}").into());
        }
        let content = body
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        let usage = TokenUsage {
            prompt_tokens: body
                .get("prompt_eval_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            completion_tokens: body.get("eval_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            total_tokens: (body.get("prompt_eval_count").and_then(|v| v.as_u64()).unwrap_or(0)
                + body.get("eval_count").and_then(|v| v.as_u64()).unwrap_or(0)) as u32,
        };
        Ok(CompletionResponse {
            content,
            tool_calls: Vec::new(),
            usage,
            model: body
                .get("model")
                .and_then(|m| m.as_str())
                .map(String::from)
                .unwrap_or_else(|| request.model.clone()),
            finish_reason: body
                .get("done_reason")
                .and_then(|f| f.as_str())
                .map(String::from),
        })
    }

    async fn stream(
        &self,
        request: &CompletionRequest,
        tx: mpsc::Sender<StreamChunk>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/api/chat", self.cfg.base_url);
        let resp = self
            .client
            .post(&url)
            .json(&self.build_body(request, true))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Ollama {status}: {body}").into());
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
                if line.trim().is_empty() {
                    continue;
                }
                let v: Value = match serde_json::from_str(&line) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let content = v
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_str())
                    .map(String::from);
                let done = v.get("done").and_then(|d| d.as_bool()).unwrap_or(false);
                if content.is_some() || done {
                    let send_done = done;
                    if tx
                        .send(StreamChunk {
                            content,
                            tool_call_delta: None,
                            done,
                        })
                        .await
                        .is_err()
                    {
                        return Ok(());
                    }
                    if send_done {
                        return Ok(());
                    }
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
        let url = format!("{}/api/tags", self.cfg.base_url);
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Ok(Vec::new());
        }
        let body: Value = resp.json().await?;
        let arr = body
            .get("models")
            .and_then(|m| m.as_array())
            .cloned()
            .unwrap_or_default();
        let mut out = Vec::with_capacity(arr.len());
        for m in arr {
            if let Some(name) = m.get("name").and_then(|n| n.as_str()) {
                out.push(ModelInfo {
                    id: name.to_string(),
                    provider: "ollama".into(),
                    display_name: name.to_string(),
                    context_window: 32_768,
                    max_output_tokens: Some(8_192),
                    // Local execution → no per-token cost.
                    cost_per_m_input: 0.0,
                    cost_per_m_output: 0.0,
                    supports_tools: false,
                    supports_streaming: true,
                });
            }
        }
        Ok(out)
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
        let url = format!("{}/api/tags", self.cfg.base_url);
        match self.client.get(&url).send().await {
            Ok(r) => Ok(r.status().is_success()),
            Err(_) => Ok(false),
        }
    }
}

fn role_to_str(role: &Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "user",
    }
}

fn serialize_message(m: &Message) -> Value {
    serde_json::json!({
        "role": role_to_str(&m.role),
        "content": m.content,
    })
}

impl From<OllamaProvider> for Arc<dyn LlmProvider> {
    fn from(p: OllamaProvider) -> Self {
        Arc::new(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_body_carries_options() {
        let p = OllamaProvider::new(OllamaConfig::default());
        let req = CompletionRequest {
            model: "llama3".into(),
            messages: vec![Message {
                role: Role::User,
                content: "hi".into(),
            }],
            temperature: Some(0.4),
            max_tokens: Some(256),
            ..Default::default()
        };
        let body = p.build_body(&req, true);
        assert_eq!(body["model"], "llama3");
        assert_eq!(body["stream"], true);
        let temp = body["options"]["temperature"].as_f64().unwrap();
        assert!((temp - 0.4).abs() < 1e-6, "temperature: {temp}");
        assert_eq!(body["options"]["num_predict"], 256);
    }
}
