//! Stub implementation of [`LlmProvider`] for testing and offline use.
//!
//! Returns canned responses for completion requests, echoes back
//! model info, and reports healthy. Useful as a fallback when no
//! real provider credentials are available.

use async_trait::async_trait;
use tokio::sync::mpsc;

use super::provider::LlmProvider;
use super::types::{CompletionRequest, CompletionResponse, ModelInfo, StreamChunk, TokenUsage};

/// Stub LLM provider that returns deterministic placeholder responses.
///
/// Use this for integration tests, offline development, or as a
/// circuit-breaker fallback when live providers are unavailable.
pub struct StubProvider {
    provider_name: String,
}

impl StubProvider {
    /// Create a new stub provider with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            provider_name: name.into(),
        }
    }
}

impl Default for StubProvider {
    fn default() -> Self {
        Self::new("stub")
    }
}

#[async_trait]
impl LlmProvider for StubProvider {
    fn name(&self) -> &str {
        &self.provider_name
    }

    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, Box<dyn std::error::Error + Send + Sync>> {
        let prompt_tokens = request
            .messages
            .iter()
            .map(|m| m.content.split_whitespace().count() as u32)
            .sum::<u32>();

        let content = format!(
            "[stub-provider] Received {} message(s) for model '{}'. \
             This is a placeholder response from the stub provider.",
            request.messages.len(),
            request.model,
        );

        let completion_tokens = content.split_whitespace().count() as u32;

        Ok(CompletionResponse {
            content,
            tool_calls: Vec::new(),
            usage: TokenUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
            model: request.model.clone(),
            finish_reason: Some("stop".to_string()),
        })
    }

    async fn stream(
        &self,
        request: &CompletionRequest,
        tx: mpsc::Sender<StreamChunk>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let message = format!(
            "[stub-provider] Streaming placeholder for model '{}'.",
            request.model
        );

        for (i, word) in message.split_whitespace().enumerate() {
            let is_last = i == message.split_whitespace().count() - 1;
            let chunk = StreamChunk {
                content: Some(format!("{word} ")),
                tool_call_delta: None,
                done: is_last,
            };
            if tx.send(chunk).await.is_err() {
                break; // receiver dropped
            }
        }

        Ok(())
    }

    async fn list_models(
        &self,
    ) -> Result<Vec<ModelInfo>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![ModelInfo {
            id: "stub-model".to_string(),
            provider: self.provider_name.clone(),
            display_name: "Stub Model (placeholder)".to_string(),
            context_window: 128_000,
            max_output_tokens: Some(4_096),
            cost_per_m_input: 0.0,
            cost_per_m_output: 0.0,
            supports_tools: true,
            supports_streaming: true,
        }])
    }

    async fn model_info(
        &self,
        model_id: &str,
    ) -> Result<Option<ModelInfo>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Some(ModelInfo {
            id: model_id.to_string(),
            provider: self.provider_name.clone(),
            display_name: format!("Stub: {model_id}"),
            context_window: 128_000,
            max_output_tokens: Some(4_096),
            cost_per_m_input: 0.0,
            cost_per_m_output: 0.0,
            supports_tools: true,
            supports_streaming: true,
        }))
    }

    async fn health_check(&self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(true)
    }
}
