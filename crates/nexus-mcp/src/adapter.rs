//! MCP tool adapter — wraps MCP tools as generic async callables.
//!
//! This module bridges the gap between the MCP protocol and Nexus's internal tool
//! abstraction. Rather than directly depending on `nexus-agents-core` (which would
//! create a circular dependency), the adapter exposes tools as a generic
//! [`McpToolCallable`] trait that higher-level crates can integrate.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::client::McpClient;
use crate::error::McpError;
use crate::types::McpToolDefinition;

/// A generic async callable that mirrors how Nexus tools work, without depending
/// on the `nexus-agents-core` crate.
#[async_trait]
pub trait McpToolCallable: Send + Sync {
    /// The tool name.
    fn name(&self) -> &str;

    /// Human-readable description.
    fn description(&self) -> &str;

    /// JSON Schema for the tool's input.
    fn input_schema(&self) -> &serde_json::Value;

    /// Invoke the tool with the given JSON arguments.
    async fn call(&self, arguments: serde_json::Value) -> Result<serde_json::Value, McpError>;
}

/// Wraps a single MCP tool definition together with a reference to its parent
/// [`McpClient`], making it independently callable.
pub struct McpToolAdapter {
    /// The MCP client that owns this tool.
    client: Arc<McpClient>,
    /// The tool's definition (name, description, schema).
    tool_def: McpToolDefinition,
    /// The MCP server ID this tool comes from.
    server_id: String,
}

impl McpToolAdapter {
    /// Create a new adapter wrapping one tool from the given client.
    pub fn new(client: Arc<McpClient>, tool_def: McpToolDefinition) -> Self {
        let server_id = client.server_id().to_string();
        Self {
            client,
            tool_def,
            server_id,
        }
    }

    /// Return the server ID this tool belongs to.
    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    /// Return the underlying tool definition.
    pub fn tool_definition(&self) -> &McpToolDefinition {
        &self.tool_def
    }
}

#[async_trait]
impl McpToolCallable for McpToolAdapter {
    fn name(&self) -> &str {
        &self.tool_def.name
    }

    fn description(&self) -> &str {
        &self.tool_def.description
    }

    fn input_schema(&self) -> &serde_json::Value {
        &self.tool_def.input_schema
    }

    async fn call(&self, arguments: serde_json::Value) -> Result<serde_json::Value, McpError> {
        debug!(
            server = %self.server_id,
            tool = %self.tool_def.name,
            "McpToolAdapter: calling tool"
        );
        self.client.call_tool(&self.tool_def.name, arguments).await
    }
}

/// Convenience: convert all tools from an [`McpClient`] into a vec of adapters.
pub fn adapt_all_tools(client: Arc<McpClient>) -> Vec<McpToolAdapter> {
    client
        .tools()
        .iter()
        .map(|t| McpToolAdapter::new(Arc::clone(&client), t.clone()))
        .collect()
}

/// Metadata about an adapted MCP tool, useful for serializing tool lists
/// without carrying the live client reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    /// The MCP server this tool comes from.
    pub server_id: String,
    /// Tool name.
    pub name: String,
    /// Tool description.
    pub description: String,
    /// JSON Schema for the tool's input.
    pub input_schema: serde_json::Value,
}

impl From<&McpToolAdapter> for McpToolInfo {
    fn from(adapter: &McpToolAdapter) -> Self {
        Self {
            server_id: adapter.server_id.clone(),
            name: adapter.tool_def.name.clone(),
            description: adapter.tool_def.description.clone(),
            input_schema: adapter.tool_def.input_schema.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_info_serialization() {
        let info = McpToolInfo {
            server_id: "fs-server".into(),
            name: "read_file".into(),
            description: "Read a file".into(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["server_id"], "fs-server");
        assert_eq!(json["name"], "read_file");
    }
}
