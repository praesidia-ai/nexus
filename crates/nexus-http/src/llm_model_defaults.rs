//! Canonical default model IDs for OpenAI and Anthropic.
//!
//! Keep labels and availability aligned with [`crate::handlers::settings`] (`list_models`).
//! Avoid `gpt-4o-mini` — too weak for reliable codegen; use `gpt-4.1` / `gpt-4.1-mini` instead.

/// Default OpenAI model when no project/global override is set (`NEXUS_MODEL`, chat, fallbacks).
pub const OPENAI_DEFAULT_MODEL: &str = "gpt-4.1";

/// Default Anthropic model when no project/global override is set.
pub const ANTHROPIC_DEFAULT_MODEL: &str = "claude-sonnet-4-6";

/// Lower-latency / cheaper OpenAI option (still stronger than the old `gpt-4o-mini` tier).
pub const OPENAI_FAST_MODEL: &str = "gpt-4.1-mini";

/// Lower-latency Anthropic option (Haiku 4.5-class ID from settings list).
pub const ANTHROPIC_FAST_MODEL: &str = "claude-haiku-4-5-20251001";
