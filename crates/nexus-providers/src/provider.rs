//! LLM provider trait — the unified interface for all LLM backends.

use async_trait::async_trait;
use tokio::sync::mpsc;

use super::types::{CompletionRequest, CompletionResponse, ModelInfo, StreamChunk};

/// Trait for LLM provider implementations.
///
/// Each provider (OpenAI, Anthropic, Ollama, etc.) implements this trait
/// to provide a unified interface for completion, streaming, and model queries.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Provider name (e.g., "openai", "anthropic", "ollama").
    fn name(&self) -> &str;

    /// Complete a request and return the full response.
    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, Box<dyn std::error::Error + Send + Sync>>;

    /// Stream a completion, sending chunks through the provided sender.
    async fn stream(
        &self,
        request: &CompletionRequest,
        tx: mpsc::Sender<StreamChunk>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// List available models for this provider.
    async fn list_models(
        &self,
    ) -> Result<Vec<ModelInfo>, Box<dyn std::error::Error + Send + Sync>>;

    /// Get info for a specific model.
    async fn model_info(
        &self,
        model_id: &str,
    ) -> Result<Option<ModelInfo>, Box<dyn std::error::Error + Send + Sync>>;

    /// Check if this provider is available (has valid credentials, can reach API).
    async fn health_check(&self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>>;
}
