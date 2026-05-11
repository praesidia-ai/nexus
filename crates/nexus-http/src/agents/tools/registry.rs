//! Tool registry — manages available tools for agents.
//!
//! Tools are registered at startup and referenced by name from
//! [`super::super::definition::AgentDefinition::tools`]. The registry resolves
//! names to concrete [`Tool`] implementations at execution time.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Input to a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInput {
    /// JSON parameters for the tool.
    pub parameters: Value,
}

/// Output from a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    /// JSON result from the tool.
    pub result: Value,
    /// Whether the tool call succeeded.
    pub success: bool,
    /// Error message, if any.
    pub error: Option<String>,
}

/// A tool that agents can invoke.
///
/// Implementors provide a schema (for LLM function calling), a category
/// (for grouping), and an async `execute` method.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name (used to reference from AgentDefinition.tools).
    fn name(&self) -> &str;
    /// Human-readable description.
    fn description(&self) -> &str;
    /// JSON Schema for the tool's parameters.
    fn schema(&self) -> Value;
    /// Execute the tool with the given input.
    async fn execute(&self, input: ToolInput) -> ToolOutput;
    /// Category for grouping (file_ops, build_ops, search_ops, project_ops, external_ops).
    fn category(&self) -> &str;
}

/// Registry of available tools. Tools are registered at startup and referenced by name.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool. Overwrites any existing tool with the same name.
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(name)
    }

    /// List all registered tool names.
    pub fn list_names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// List all registered tools grouped by category.
    pub fn by_category(&self) -> HashMap<String, Vec<String>> {
        let mut result: HashMap<String, Vec<String>> = HashMap::new();
        for tool in self.tools.values() {
            result
                .entry(tool.category().to_string())
                .or_default()
                .push(tool.name().to_string());
        }
        result
    }

    /// Resolve a list of tool names to actual tool references.
    ///
    /// Returns `(resolved_tools, missing_names)`.
    pub fn resolve(&self, names: &[String]) -> (Vec<Arc<dyn Tool>>, Vec<String>) {
        let mut resolved = Vec::new();
        let mut missing = Vec::new();
        for name in names {
            match self.tools.get(name) {
                Some(tool) => resolved.push(Arc::clone(tool)),
                None => missing.push(name.clone()),
            }
        }
        (resolved, missing)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
