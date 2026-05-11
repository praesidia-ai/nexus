//! Hook points and execution types.
//!
//! Hooks allow plugins to intercept and modify the Nexus pipeline at
//! well-defined points. Each hook receives a context and returns a result
//! that can modify, pass through, or abort the pipeline.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Points in the Nexus pipeline where plugins can hook in.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HookPoint {
    /// Before intent analysis runs.
    PreIntent,
    /// After intent analysis completes.
    PostIntent,
    /// Before architecture decisions are made.
    PreDecision,
    /// After architecture decisions are made.
    PostDecision,
    /// Before code generation starts.
    PreGenerate,
    /// After code generation completes.
    PostGenerate,
    /// Before quality scoring runs.
    PreQualityScore,
    /// After quality scoring completes.
    PostQualityScore,
    /// Before the guarantee loop runs.
    PreGuarantee,
    /// After the guarantee loop completes.
    PostGuarantee,
    /// Before a build command runs.
    PreBuild,
    /// After a build command completes.
    PostBuild,
    /// Before deployment.
    PreDeploy,
    /// After deployment.
    PostDeploy,
    /// When an agent starts a task.
    AgentTaskStart,
    /// When an agent completes a task.
    AgentTaskComplete,
}

/// Context passed to a hook execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookContext {
    /// Project ID, if applicable.
    pub project_id: Option<String>,
    /// Tenant ID.
    pub tenant_id: String,
    /// The hook point being executed.
    pub hook_point: HookPoint,
    /// Data relevant to this hook point (varies by hook).
    pub data: serde_json::Value,
    /// Plugin configuration, if any.
    pub config: Option<serde_json::Value>,
}

/// Result from a hook execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResult {
    /// The action to take after this hook.
    pub action: HookAction,
    /// Modified data to pass forward (if action is Modify).
    pub data: Option<serde_json::Value>,
    /// Messages to log.
    pub messages: Vec<String>,
}

/// Action a hook can take.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HookAction {
    /// Continue with the original data unchanged.
    Continue,
    /// Continue with modified data.
    Modify,
    /// Abort the pipeline with an error.
    Abort,
    /// Skip the current pipeline step.
    Skip,
}

impl HookResult {
    /// Create a "continue" result (no changes).
    pub fn ok() -> Self {
        Self {
            action: HookAction::Continue,
            data: None,
            messages: Vec::new(),
        }
    }

    /// Create a "modify" result with updated data.
    pub fn modify(data: serde_json::Value) -> Self {
        Self {
            action: HookAction::Modify,
            data: Some(data),
            messages: Vec::new(),
        }
    }

    /// Create an "abort" result.
    pub fn abort(message: impl Into<String>) -> Self {
        Self {
            action: HookAction::Abort,
            data: None,
            messages: vec![message.into()],
        }
    }
}

/// Trait for plugin hook implementations.
///
/// Plugins implement this trait and register it with the hook registry.
#[async_trait]
pub trait PluginHook: Send + Sync {
    /// Execute the hook with the given context.
    async fn execute(
        &self,
        context: &HookContext,
    ) -> Result<HookResult, Box<dyn std::error::Error + Send + Sync>>;
}
