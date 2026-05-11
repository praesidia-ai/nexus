//! `NexusAgent` — a named agent with a specialist role backed by a direct LLM call.
//!
//! Supports Anthropic, OpenAI (and any OpenAI-compatible endpoint), and Ollama.
//! No external runtime dependency: all LLM communication is done via `reqwest`.

use std::future::Future;

use serde::{Deserialize, Serialize};
use tracing::{debug, instrument, warn};

use crate::config::RosterConfig;
use crate::roster::{AgentName, AgentRole};

// ---------------------------------------------------------------------------
// Tool types
// ---------------------------------------------------------------------------

/// Definition of a tool the agent can call, expressed as a JSON Schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema describing the function parameters.
    pub parameters: serde_json::Value,
}

/// A tool invocation requested by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// The result of executing a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: String,
    pub output: String,
    pub success: bool,
}

// ---------------------------------------------------------------------------
// ChatMessage
// ---------------------------------------------------------------------------

/// A single conversation turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    /// Tool calls returned by the assistant (OpenAI format). Present only on
    /// assistant messages that request tool use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<serde_json::Value>,
    /// For OpenAI-style `"tool"` role messages, the id of the call this result
    /// corresponds to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".into(), content: content.into(), tool_calls: None, tool_call_id: None }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: "assistant".into(), content: content.into(), tool_calls: None, tool_call_id: None }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".into(), content: content.into(), tool_calls: None, tool_call_id: None }
    }

    /// Create an assistant message that includes tool calls (OpenAI wire format).
    pub fn assistant_with_tool_calls(content: impl Into<String>, tool_calls: serde_json::Value) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
            tool_calls: Some(tool_calls),
            tool_call_id: None,
        }
    }

    /// Create a tool-result message (OpenAI `"tool"` role).
    pub fn tool_result(call_id: impl Into<String>, output: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: output.into(),
            tool_calls: None,
            tool_call_id: Some(call_id.into()),
        }
    }
}

// ---------------------------------------------------------------------------
// NexusAgent
// ---------------------------------------------------------------------------

/// A named agent backed by direct LLM HTTP calls, with managed conversation history.
pub struct NexusAgent {
    name: AgentName,
    system_prompt: String,
    history: Vec<ChatMessage>,
    provider: String,
    api_key: Option<String>,
    model: String,
    api_base: String,
    max_history: usize,
    http: reqwest::Client,
    tools: Vec<ToolDefinition>,
}

/// Classify an LLM error string as transient (worth retrying) vs permanent.
/// Mirrors the semantics of `llm_client::is_retryable_error` but scoped to
/// ZeroClaw's direct provider calls — kept narrow so we don't retry 4xx
/// client errors that will never succeed.
fn is_transient_zeroclaw_error(error: &str) -> bool {
    let e = error.to_ascii_lowercase();
    e.contains("429")
        || e.contains("500")
        || e.contains("502")
        || e.contains("503")
        || e.contains("504")
        || e.contains("529")
        || e.contains("rate limit")
        || e.contains("overloaded")
        || e.contains("timed out")
        || e.contains("timeout")
        || e.contains("connection reset")
        || e.contains("connection refused")
        || e.contains("temporarily unavailable")
}

fn default_model(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "claude-3-5-haiku-20241022",
        "openai"    => "gpt-4.1",
        _           => "llama3.2",
    }
}

fn default_api_base(provider: &str) -> String {
    match provider {
        "anthropic" => "https://api.anthropic.com".into(),
        "openai"    => "https://api.openai.com".into(),
        "ollama"    => std::env::var("OLLAMA_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:11434".into()),
        _           => "https://api.openai.com".into(),
    }
}

impl NexusAgent {
    /// Create from a roster config and agent name.
    pub async fn from_roster_config(
        name: AgentName,
        cfg: &RosterConfig,
    ) -> anyhow::Result<Self> {
        let role: AgentRole = name.role();

        let workspace = cfg.workspace_dir.join(name.display_name().to_lowercase());
        std::fs::create_dir_all(&workspace)?;

        let model = cfg.model.clone()
            .unwrap_or_else(|| default_model(&cfg.provider).to_string());

        let api_base = match cfg.provider.as_str() {
            "ollama" => std::env::var("OLLAMA_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:11434".into()),
            _ => default_api_base(&cfg.provider),
        };

        let system_prompt = format!(
            "{}\n\nYour workspace directory is: {}",
            role.system_prompt_seed,
            workspace.display(),
        );

        tracing::info!(agent = %name, domain = role.domain, "Agent initialised");

        Ok(Self {
            name,
            system_prompt,
            history: Vec::new(),
            provider: cfg.provider.clone(),
            api_key: cfg.api_key.clone(),
            model,
            api_base,
            max_history: cfg.history_size,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            tools: Vec::new(),
        })
    }

    /// Agent name.
    pub fn name(&self) -> AgentName {
        self.name
    }

    /// Read-only access to the current conversation history.
    pub fn history(&self) -> &[ChatMessage] {
        &self.history
    }

    /// Execute one turn: send `input` and receive the agent's full reply.
    ///
    /// Retries transient provider errors (rate limits, 5xx, timeouts) up to
    /// 3 times with exponential backoff. On permanent failure the user
    /// message is rolled back out of history so we never persist a user
    /// turn with no paired assistant reply — that would violate the
    /// alternating-role invariant and cause subsequent turns to 400.
    #[instrument(skip(self, input), fields(agent = %self.name))]
    pub async fn turn(&mut self, input: impl AsRef<str>) -> anyhow::Result<String> {
        let msg = input.as_ref();
        debug!(len = msg.len(), "-> turn");

        self.history.push(ChatMessage::user(msg));
        self.trim_history();

        const MAX_ATTEMPTS: u32 = 3;
        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 0..MAX_ATTEMPTS {
            if attempt > 0 {
                let delay_ms = 500u64 * 2u64.pow(attempt - 1);
                warn!(agent = %self.name, attempt, delay_ms, "retrying LLM call after transient error");
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
            let reply = match self.provider.as_str() {
                "anthropic" => self.call_anthropic().await,
                "ollama" => self.call_ollama().await,
                _ => self.call_openai_compatible().await,
            };
            match reply {
                Ok(text) => {
                    debug!(len = text.len(), "<- reply");
                    self.history.push(ChatMessage::assistant(&text));
                    return Ok(text);
                }
                Err(e) => {
                    let transient = is_transient_zeroclaw_error(&format!("{e:#}"));
                    if transient && attempt + 1 < MAX_ATTEMPTS {
                        last_err = Some(e);
                        continue;
                    }
                    last_err = Some(e);
                    break;
                }
            }
        }

        // Permanent failure — roll back the dangling user message so the
        // next `turn()` starts from a clean alternating-role history.
        self.history.pop();
        let err = last_err.unwrap_or_else(|| anyhow::anyhow!("agent LLM call failed"));
        warn!(agent = %self.name, err = %err, "LLM call failed after retries");
        Err(err)
    }

    /// Clear the agent's conversation history.
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Seed the agent with prior conversation messages.
    pub fn seed_history(&mut self, messages: &[ChatMessage]) {
        self.history = messages.to_vec();
    }

    /// Set the tools available to this agent for tool-calling turns.
    pub fn set_tools(&mut self, tools: Vec<ToolDefinition>) {
        self.tools = tools;
    }

    /// Run an agentic loop: the LLM can call tools repeatedly until it
    /// produces a final text response or `max_iterations` is reached.
    ///
    /// `tool_executor` is an async function that receives a [`ToolCall`] and
    /// must return a [`ToolResult`].
    #[instrument(skip(self, input, tool_executor), fields(agent = %self.name))]
    pub async fn run<F, Fut>(
        &mut self,
        input: impl AsRef<str>,
        tool_executor: F,
        max_iterations: u32,
    ) -> anyhow::Result<String>
    where
        F: Fn(ToolCall) -> Fut,
        Fut: Future<Output = ToolResult>,
    {
        let msg = input.as_ref();
        debug!(len = msg.len(), "-> run (max_iterations={})", max_iterations);

        self.history.push(ChatMessage::user(msg));
        self.trim_history();

        let mut last_text = String::new();

        for iteration in 0..max_iterations {
            debug!(iteration, "agent loop tick");

            let (text, tool_calls) = match self.provider.as_str() {
                "anthropic" => self.call_anthropic_with_tools().await?,
                "ollama"    => self.call_ollama_with_tools().await?,
                _           => self.call_openai_with_tools().await?,
            };

            last_text = text.clone();

            if tool_calls.is_empty() {
                debug!(len = last_text.len(), "<- final reply (iteration {})", iteration);
                self.history.push(ChatMessage::assistant(&last_text));
                return Ok(last_text);
            }

            debug!(count = tool_calls.len(), "tool calls requested");

            // Record the assistant message with embedded tool calls
            match self.provider.as_str() {
                "anthropic" => {
                    // For Anthropic we rebuild the full content array with text + tool_use blocks
                    let mut content_blocks = Vec::new();
                    if !text.is_empty() {
                        content_blocks.push(serde_json::json!({"type": "text", "text": text}));
                    }
                    for tc in &tool_calls {
                        content_blocks.push(serde_json::json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.name,
                            "input": tc.arguments,
                        }));
                    }
                    // Push a raw JSON message that the Anthropic call path will forward correctly
                    self.history.push(ChatMessage {
                        role: "assistant".into(),
                        content: serde_json::to_string(&content_blocks).unwrap_or_default(),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }
                _ => {
                    // OpenAI/Ollama wire format: tool_calls on the assistant message
                    let tc_json: Vec<serde_json::Value> = tool_calls
                        .iter()
                        .map(|tc| serde_json::json!({
                            "id": tc.id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": tc.arguments.to_string(),
                            }
                        }))
                        .collect();
                    self.history.push(ChatMessage::assistant_with_tool_calls(
                        &text,
                        serde_json::Value::Array(tc_json),
                    ));
                }
            }

            // Execute each tool call and push results into history
            for tc in tool_calls {
                let call_id = tc.id.clone();
                let result = tool_executor(tc).await;
                debug!(call_id = %call_id, success = result.success, "tool result");

                match self.provider.as_str() {
                    "anthropic" => {
                        // Anthropic expects role: "user" with a tool_result content block
                        self.history.push(ChatMessage {
                            role: "user".into(),
                            content: serde_json::to_string(&[serde_json::json!({
                                "type": "tool_result",
                                "tool_use_id": call_id,
                                "content": result.output,
                            })]).unwrap_or_default(),
                            tool_calls: None,
                            tool_call_id: None,
                        });
                    }
                    _ => {
                        self.history.push(ChatMessage::tool_result(call_id, result.output));
                    }
                }
            }
        }

        warn!("max iterations ({}) reached, returning last text", max_iterations);
        Ok(last_text)
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    fn trim_history(&mut self) {
        if self.history.len() > self.max_history {
            let excess = self.history.len() - self.max_history;
            self.history.drain(..excess);
        }
    }

    async fn call_anthropic(&self) -> anyhow::Result<String> {
        let url = format!("{}/v1/messages", self.api_base);
        let key = self.api_key.as_deref().unwrap_or("");

        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 4096,
            "system": self.system_prompt,
            "messages": self.history,
        });

        let resp = self.http
            .post(&url)
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API error {}: {}", status, text);
        }

        let json: serde_json::Value = resp.json().await?;
        let text = json["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();
        Ok(text)
    }

    async fn call_ollama(&self) -> anyhow::Result<String> {
        let url = format!("{}/api/chat", self.api_base);

        let mut messages = vec![ChatMessage::system(&self.system_prompt)];
        messages.extend(self.history.clone());

        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": false,
        });

        let resp = self.http.post(&url).json(&body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Ollama API error {}: {}", status, text);
        }

        let json: serde_json::Value = resp.json().await?;
        let text = json["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        Ok(text)
    }

    // -----------------------------------------------------------------------
    // Tool-aware LLM call helpers (used by `run`)
    // -----------------------------------------------------------------------

    /// Build the OpenAI-format tools array from `self.tools`.
    fn openai_tools_payload(&self) -> Vec<serde_json::Value> {
        self.tools.iter().map(|t| serde_json::json!({
            "type": "function",
            "function": {
                "name": t.name,
                "description": t.description,
                "parameters": t.parameters,
            }
        })).collect()
    }

    /// Build the Anthropic-format tools array from `self.tools`.
    fn anthropic_tools_payload(&self) -> Vec<serde_json::Value> {
        self.tools.iter().map(|t| serde_json::json!({
            "name": t.name,
            "description": t.description,
            "input_schema": t.parameters,
        })).collect()
    }

    async fn call_openai_with_tools(&self) -> anyhow::Result<(String, Vec<ToolCall>)> {
        let url = format!("{}/v1/chat/completions", self.api_base);

        let mut messages = vec![ChatMessage::system(&self.system_prompt)];
        messages.extend(self.history.clone());

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
        });

        if !self.tools.is_empty() {
            body["tools"] = serde_json::Value::Array(self.openai_tools_payload());
        }

        let mut req = self.http.post(&url).json(&body);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let resp = req.send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("LLM API error {}: {}", status, text);
        }

        let json: serde_json::Value = resp.json().await?;
        let choice = &json["choices"][0]["message"];

        let text = choice["content"].as_str().unwrap_or("").to_string();

        let mut calls = Vec::new();
        if let Some(tcs) = choice["tool_calls"].as_array() {
            for tc in tcs {
                let id = tc["id"].as_str().unwrap_or("").to_string();
                let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
                let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                let arguments: serde_json::Value =
                    serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));
                calls.push(ToolCall { id, name, arguments });
            }
        }

        Ok((text, calls))
    }

    async fn call_anthropic_with_tools(&self) -> anyhow::Result<(String, Vec<ToolCall>)> {
        let url = format!("{}/v1/messages", self.api_base);
        let key = self.api_key.as_deref().unwrap_or("");

        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": 4096,
            "system": self.system_prompt,
            "messages": self.history,
        });

        if !self.tools.is_empty() {
            body["tools"] = serde_json::Value::Array(self.anthropic_tools_payload());
        }

        let resp = self.http
            .post(&url)
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API error {}: {}", status, text);
        }

        let json: serde_json::Value = resp.json().await?;
        let content = json["content"].as_array();

        let mut text = String::new();
        let mut calls = Vec::new();

        if let Some(blocks) = content {
            for block in blocks {
                match block["type"].as_str() {
                    Some("text") => {
                        text.push_str(block["text"].as_str().unwrap_or(""));
                    }
                    Some("tool_use") => {
                        calls.push(ToolCall {
                            id: block["id"].as_str().unwrap_or("").to_string(),
                            name: block["name"].as_str().unwrap_or("").to_string(),
                            arguments: block["input"].clone(),
                        });
                    }
                    _ => {}
                }
            }
        }

        Ok((text, calls))
    }

    async fn call_ollama_with_tools(&self) -> anyhow::Result<(String, Vec<ToolCall>)> {
        let url = format!("{}/api/chat", self.api_base);

        let mut messages = vec![ChatMessage::system(&self.system_prompt)];
        messages.extend(self.history.clone());

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": false,
        });

        if !self.tools.is_empty() {
            body["tools"] = serde_json::Value::Array(self.openai_tools_payload());
        }

        let resp = self.http.post(&url).json(&body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Ollama API error {}: {}", status, text);
        }

        let json: serde_json::Value = resp.json().await?;
        let text = json["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let mut calls = Vec::new();
        if let Some(tcs) = json["message"]["tool_calls"].as_array() {
            for tc in tcs {
                let func = &tc["function"];
                let id = tc["id"]
                    .as_str()
                    .unwrap_or(&uuid::Uuid::new_v4().to_string())
                    .to_string();
                let name = func["name"].as_str().unwrap_or("").to_string();
                let arguments = func["arguments"].clone();
                calls.push(ToolCall { id, name, arguments });
            }
        }

        Ok((text, calls))
    }

    async fn call_openai_compatible(&self) -> anyhow::Result<String> {
        let url = format!("{}/v1/chat/completions", self.api_base);

        let mut messages = vec![ChatMessage::system(&self.system_prompt)];
        messages.extend(self.history.clone());

        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
        });

        let mut req = self.http.post(&url).json(&body);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let resp = req.send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("LLM API error {}: {}", status, text);
        }

        let json: serde_json::Value = resp.json().await?;
        let text = json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        Ok(text)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_name_display() {
        for name in AgentName::all() {
            assert!(!name.display_name().is_empty());
        }
    }

    #[test]
    fn chat_message_roles() {
        let u = ChatMessage::user("hello");
        assert_eq!(u.role, "user");
        assert_eq!(u.content, "hello");

        let a = ChatMessage::assistant("hi");
        assert_eq!(a.role, "assistant");

        let s = ChatMessage::system("you are helpful");
        assert_eq!(s.role, "system");
    }

    #[test]
    fn trim_history_keeps_last_n() {
        use crate::config::RosterConfig;
        use std::path::PathBuf;

        let cfg = RosterConfig {
            provider: "openai".into(),
            api_key: None,
            model: None,
            workspace_dir: PathBuf::from("/tmp"),
            history_size: 2,
            persistence_dir: None,
        };

        let mut agent = NexusAgent {
            name: AgentName::Nova,
            system_prompt: String::new(),
            history: vec![
                ChatMessage::user("a"),
                ChatMessage::assistant("b"),
                ChatMessage::user("c"),
            ],
            provider: cfg.provider.clone(),
            api_key: None,
            model: "gpt-4.1".into(),
            api_base: "https://api.openai.com".into(),
            max_history: cfg.history_size,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            tools: Vec::new(),
        };

        agent.trim_history();
        assert_eq!(agent.history.len(), 2);
        assert_eq!(agent.history[0].content, "b");
        assert_eq!(agent.history[1].content, "c");
    }
}
