//! nexus-providers — LLM provider abstraction for Nexus.
//!
//! This crate defines traits and types for interacting with LLM providers
//! (OpenAI, Anthropic, Ollama, etc.) through a unified interface. The
//! abstraction supports completion, streaming, and model info queries.

pub mod provider;
pub mod registry;
pub mod types;

pub mod anthropic;
pub mod ollama;
pub mod openai;
pub mod stub_provider;

pub use anthropic::{AnthropicConfig, AnthropicProvider};
pub use ollama::{OllamaConfig, OllamaProvider};
pub use openai::{OpenAiConfig, OpenAiProvider};
pub use provider::LlmProvider;
pub use registry::{register_builtins, BuiltinProviderConfig, ProviderId, ProviderRegistry};
pub use types::{
    CompletionRequest, CompletionResponse, Message, ModelInfo, Role, StreamChunk, TokenUsage,
    ToolCall, ToolCallDelta, ToolDefinition,
};

#[cfg(test)]
mod tests;
