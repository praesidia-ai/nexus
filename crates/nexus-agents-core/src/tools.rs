//! Agent tool trait and types.
//!
//! Tools are the primary way agents interact with the outside world.
//! Each tool has a name, description, JSON Schema for parameters, and
//! an async execute method. Tools are registered in a [`ToolRegistry`]
//! and resolved by name at agent runtime.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Input to a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInput {
    /// Parameters as a JSON value matching the tool's parameter schema.
    pub parameters: serde_json::Value,
}

/// Output from a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    /// The result as a JSON value.
    pub result: serde_json::Value,
    /// Whether the tool executed successfully.
    pub success: bool,
    /// Error message if the tool failed.
    pub error: Option<String>,
}

/// Tool category for grouping and access control.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ToolCategory {
    /// File system operations (read, write, list).
    FileSystem,
    /// Code execution (run commands, build).
    CodeExecution,
    /// Web access (HTTP requests, scraping).
    WebAccess,
    /// Data processing (transform, aggregate).
    DataProcessing,
    /// Nexus-native tools (project state, generation).
    NexusNative,
    /// Inter-agent communication tools.
    Communication,
    /// External integrations (APIs, services).
    External,
}

/// Context provided to a tool during execution.
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// Project directory, if the tool operates on a project.
    pub project_dir: Option<std::path::PathBuf>,
    /// Tenant ID for multi-tenancy scoping.
    pub tenant_id: String,
    /// Agent ID that is invoking this tool.
    pub agent_id: String,
    /// Remaining budget in USD for this execution.
    pub budget_remaining: f32,
}

/// A tool that agents can invoke.
///
/// Implement this trait to create new tools. Register implementations
/// in the tool registry so agents can discover and use them.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name — must be unique within a registry.
    fn name(&self) -> &str;

    /// Human-readable description of what this tool does.
    fn description(&self) -> &str;

    /// JSON Schema for the tool's parameters.
    fn parameters_schema(&self) -> serde_json::Value;

    /// Tool category for grouping and access control.
    fn category(&self) -> ToolCategory;

    /// Execute the tool with the given input and context.
    async fn execute(&self, input: ToolInput, context: &ToolContext) -> ToolOutput;

    /// Whether this tool has side effects (writes files, makes HTTP requests, etc.).
    ///
    /// Side-effect-free tools can be safely retried and cached.
    fn is_side_effect(&self) -> bool {
        false
    }
}
