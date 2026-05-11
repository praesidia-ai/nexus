//! Agent runtime shim.
//!
//! Implements the `AgentRuntime` trait with a local in-process stub.
//! Extend or replace with a remote HTTP implementation as needed.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub id: String,
    pub name: String,
    pub system_prompt: String,
    pub model: String,
    pub provider: LLMProvider,
    pub tools: Vec<ToolDefinition>,
    pub memory: MemoryConfig,
    pub sandbox: SandboxConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema object for the tool parameters.
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub persistent: bool,
    pub storage_path: Option<String>,
    pub vector_collection: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    pub allow_network: bool,
    pub allow_fs: bool,
    pub allowed_domains: Vec<String>,
    pub allowed_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum LLMProvider {
    Anthropic,
    OpenAI,
    Ollama { base_url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentMode {
    /// Long-running, local, persistent memory.
    Personal,
    /// A2A protocol HTTP endpoint.
    Server,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Running,
    Idle,
    Stopped,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHandle {
    pub id: String,
    pub name: String,
    pub pid: Option<u32>,
    pub mode: DeploymentMode,
    pub status: AgentStatus,
    pub started_at: chrono::DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Runtime trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait AgentRuntime: Send + Sync {
    async fn spawn(
        &self,
        config: AgentConfig,
        mode: DeploymentMode,
    ) -> anyhow::Result<AgentHandle>;
    async fn stop(&self, handle: &AgentHandle) -> anyhow::Result<()>;
    async fn send(&self, handle: &AgentHandle, message: &str) -> anyhow::Result<String>;
    async fn list_running(&self) -> Vec<AgentHandle>;
}

// ---------------------------------------------------------------------------
// In-process implementation
// ---------------------------------------------------------------------------

/// In-process agent runtime. Manages agent handles in memory.
pub struct LocalAgentRuntime {
    handles: tokio::sync::RwLock<HashMap<String, AgentHandle>>,
}

impl LocalAgentRuntime {
    pub fn new() -> Self {
        Self {
            handles: tokio::sync::RwLock::new(HashMap::new()),
        }
    }
}

impl Default for LocalAgentRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentRuntime for LocalAgentRuntime {
    async fn spawn(
        &self,
        config: AgentConfig,
        mode: DeploymentMode,
    ) -> anyhow::Result<AgentHandle> {
        let handle = AgentHandle {
            id: config.id.clone(),
            name: config.name.clone(),
            pid: None,
            mode,
            status: AgentStatus::Idle,
            started_at: Utc::now(),
        };
        self.handles
            .write()
            .await
            .insert(config.id.clone(), handle.clone());
        tracing::info!(agent_id = %config.id, "LocalAgentRuntime: spawned agent");
        Ok(handle)
    }

    async fn stop(&self, handle: &AgentHandle) -> anyhow::Result<()> {
        self.handles.write().await.remove(&handle.id);
        tracing::info!(agent_id = %handle.id, "LocalAgentRuntime: stopped agent");
        Ok(())
    }

    async fn send(&self, handle: &AgentHandle, message: &str) -> anyhow::Result<String> {
        tracing::debug!(agent_id = %handle.id, msg_len = %message.len(), "LocalAgentRuntime: send");
        Ok(format!(
            "[Agent '{}' received message — wire to NexusAgent::turn() for LLM execution]",
            handle.name
        ))
    }

    async fn list_running(&self) -> Vec<AgentHandle> {
        self.handles.read().await.values().cloned().collect()
    }
}

// Backwards-compatible type alias.
pub type ZeroClawShim = LocalAgentRuntime;
#[allow(dead_code)]
pub type ZeroClawRuntime = dyn AgentRuntime;
