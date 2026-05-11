//! Shared types for LLM provider interactions.

use serde::{Deserialize, Serialize};

/// A message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Role of the message sender.
    pub role: Role,
    /// Message content.
    pub content: String,
}

/// Role in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System message — sets the context and behavior.
    System,
    /// User message — the human's input.
    User,
    /// Assistant message — the LLM's response.
    Assistant,
    /// Tool message — result from a tool call.
    Tool,
}

/// A request for LLM completion.
///
/// Per ADR-003 §2: in addition to model parameters, every request carries
/// the cross-cutting enforcement context (`deadline`, `tenant_id`,
/// `call_site`, `trace_id`) that the provider implementation MUST honour.
///
/// New cross-cutting fields are `Option`/serde-default so existing callers
/// don't need to change in lock-step; the dispatcher (`LlmClient`) fills in
/// defaults if they're absent. Construct test instances via `..Default::default()`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// Model identifier (e.g., "gpt-4o", "claude-sonnet-4-20250514").
    pub model: String,
    /// Conversation messages.
    pub messages: Vec<Message>,
    /// Maximum tokens to generate.
    pub max_tokens: Option<u32>,
    /// Temperature for sampling (0.0 to 2.0).
    pub temperature: Option<f32>,
    /// Tool definitions available for the model to call.
    pub tools: Option<Vec<ToolDefinition>>,
    /// Whether to stream the response.
    pub stream: bool,
    /// Optional stop sequences.
    pub stop: Option<Vec<String>>,

    // -----------------------------------------------------------------
    // Cross-cutting enforcement context (ADR-003 §2). Optional for source
    // compatibility; LlmClient fills defaults.
    // -----------------------------------------------------------------
    /// Hard wallclock deadline for this call, in unix-ms (UTC). Provider
    /// MUST `tokio::time::timeout_at` to this point and emit a terminal
    /// `StreamChunk { done: true }` (or `Err`) on expiry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_ms: Option<i64>,

    /// Tenant attribution for cost / rate limit / audit. Required at the
    /// dispatcher level; trait impls do not need to enforce this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,

    /// Project scope for per-project cost rollups.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,

    /// Stable static identifier of the call site (e.g.
    /// `"oneshot.intent_phase"`). Used as a metric label, so prefer a
    /// closed set of values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_site: Option<String>,

    /// Distributed trace id; passed through to provider headers when
    /// supported (e.g. `traceparent`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,

    /// Optional idempotency key — providers that support it dedupe on the
    /// (tenant_id, idempotency_key) pair.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

/// A tool definition provided to the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name.
    pub name: String,
    /// Tool description.
    pub description: String,
    /// JSON Schema for parameters.
    pub parameters: serde_json::Value,
}

/// A completion response from an LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// The generated text content.
    pub content: String,
    /// Tool calls requested by the model, if any.
    pub tool_calls: Vec<ToolCall>,
    /// Token usage statistics.
    pub usage: TokenUsage,
    /// The model that was actually used.
    pub model: String,
    /// Provider-specific finish reason.
    pub finish_reason: Option<String>,
}

/// A tool call requested by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool call identifier.
    pub id: String,
    /// Tool name to invoke.
    pub name: String,
    /// Arguments as a JSON value.
    pub arguments: serde_json::Value,
}

/// Token usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    /// Number of tokens in the prompt.
    pub prompt_tokens: u32,
    /// Number of tokens in the completion.
    pub completion_tokens: u32,
    /// Total tokens used.
    pub total_tokens: u32,
}

/// A chunk from a streaming completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    /// Incremental text content.
    pub content: Option<String>,
    /// Incremental tool call data.
    pub tool_call_delta: Option<ToolCallDelta>,
    /// Whether this is the final chunk.
    pub done: bool,
}

/// Incremental tool call data in a stream chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallDelta {
    /// Tool call index (for parallel tool calls).
    pub index: u32,
    /// Tool call ID (present in first chunk).
    pub id: Option<String>,
    /// Tool name (present in first chunk).
    pub name: Option<String>,
    /// Incremental arguments JSON string.
    pub arguments: Option<String>,
}

/// Information about a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model identifier.
    pub id: String,
    /// Provider name.
    pub provider: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Maximum context window in tokens.
    pub context_window: u32,
    /// Maximum output tokens.
    pub max_output_tokens: Option<u32>,
    /// Cost per million input tokens in USD.
    pub cost_per_m_input: f64,
    /// Cost per million output tokens in USD.
    pub cost_per_m_output: f64,
    /// Whether the model supports tool use.
    pub supports_tools: bool,
    /// Whether the model supports streaming.
    pub supports_streaming: bool,
}
