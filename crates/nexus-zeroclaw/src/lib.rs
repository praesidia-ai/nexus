//! `nexus-zeroclaw` — Nexus agent layer: named agents, roles, roster, inter-agent messaging.
//!
//! This crate provides:
//!
//! - **`AgentName`** / **`AgentRole`** — the 10 named specialist agents
//!   (Nova, Atlas, Kai, Luna, Orion, Sage, Ivy, Rex, Leo, Mia) with their
//!   domain descriptions, system-prompt seeds, and tool allowlists.
//!
//! - **`NexusAgent`** — a single agent instance bound to a role, backed by a
//!   direct LLM HTTP call (Anthropic / OpenAI / Ollama). No external runtime
//!   dependency required.
//!
//! - **`AgentPool`** — a lazily-initialised pool of `NexusAgent` instances
//!   kept alive for the duration of a workflow run so conversation history
//!   is shared across steps.
//!
//! - **`AgentMessage`** / **`AgentReply`** — typed envelope for inter-agent
//!   communication, with context-snippet injection.
//!
//! - **`RosterConfig`** — configuration for the whole agent pool (provider,
//!   API key, workspace directory).
//!
//! # Quick start
//! ```rust,ignore
//! use nexus_zeroclaw::{AgentPool, AgentName, RosterConfig};
//!
//! let config = RosterConfig::from_env();
//! let pool = AgentPool::new(config);
//!
//! let reply = pool.task(AgentName::Nova, "Scaffold a REST API in Rust").await?;
//! println!("{}", reply);
//! ```

pub mod agent;
pub mod config;
pub mod message;
pub mod persistence;
pub mod pool;
pub mod roster;

pub use agent::{ChatMessage, NexusAgent, ToolCall, ToolDefinition, ToolResult};
pub use config::RosterConfig;
pub use message::{AgentMessage, AgentReply, ContextSnippet, MessageKind};
pub use persistence::{AgentPersistence, AgentStats, InteractionTimer};
pub use pool::AgentPool;
pub use roster::{AgentName, AgentRole};
