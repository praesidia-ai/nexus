//! MCP server registry — manages multiple MCP server connections.
//!
//! The [`McpServerRegistry`] is the central coordination point for all MCP
//! integrations. It stores server configurations, manages connection lifecycles,
//! and provides a unified view of all available tools across every connected server.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::client::McpClient;
use crate::error::McpError;
use crate::types::McpToolDefinition;

/// Configuration for a single MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Unique identifier for this server.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Transport configuration (how to reach the server).
    pub transport: McpTransportConfig,
    /// Whether to connect automatically when the registry starts.
    #[serde(default)]
    pub auto_connect: bool,
    /// Optional environment variables to pass to stdio processes.
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// Transport-specific connection details.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpTransportConfig {
    /// Spawn a child process and communicate via stdin/stdout.
    Stdio {
        /// The command to execute.
        command: String,
        /// Arguments passed to the command.
        #[serde(default)]
        args: Vec<String>,
    },
    /// Connect via HTTP/SSE.
    Sse {
        /// The server's base URL.
        url: String,
    },
}

/// Manages a collection of MCP server configurations and their active connections.
pub struct McpServerRegistry {
    /// Registered server configurations, keyed by server ID.
    configs: HashMap<String, McpServerConfig>,
    /// Active client connections, keyed by server ID.
    clients: HashMap<String, Arc<McpClient>>,
}

impl McpServerRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            configs: HashMap::new(),
            clients: HashMap::new(),
        }
    }

    /// Register an MCP server configuration without connecting to it.
    ///
    /// Returns an error if a server with the same ID is already registered.
    pub fn register(&mut self, config: McpServerConfig) -> Result<(), McpError> {
        if self.configs.contains_key(&config.id) {
            return Err(McpError::AlreadyRegistered(config.id.clone()));
        }
        info!(server_id = %config.id, name = %config.name, "Registered MCP server");
        self.configs.insert(config.id.clone(), config);
        Ok(())
    }

    /// Remove a server configuration and disconnect if connected.
    pub async fn unregister(&mut self, server_id: &str) -> Result<(), McpError> {
        if let Some(client) = self.clients.remove(server_id) {
            let _ = client.disconnect().await;
        }
        self.configs
            .remove(server_id)
            .ok_or_else(|| McpError::ServerNotFound(server_id.to_string()))?;
        info!(server_id = %server_id, "Unregistered MCP server");
        Ok(())
    }

    /// Connect to a registered server by its ID.
    ///
    /// If already connected, returns the existing client.
    pub async fn connect(&mut self, server_id: &str) -> Result<Arc<McpClient>, McpError> {
        if let Some(client) = self.clients.get(server_id) {
            return Ok(Arc::clone(client));
        }

        let config = self
            .configs
            .get(server_id)
            .ok_or_else(|| McpError::ServerNotFound(server_id.to_string()))?
            .clone();

        let env = if config.env.is_empty() {
            None
        } else {
            Some(&config.env)
        };

        let client = match &config.transport {
            McpTransportConfig::Stdio { command, args } => {
                McpClient::connect_stdio(&config.id, command, args, env).await?
            }
            McpTransportConfig::Sse { url } => {
                McpClient::connect_sse(&config.id, url).await?
            }
        };

        let client = Arc::new(client);
        self.clients.insert(server_id.to_string(), Arc::clone(&client));
        Ok(client)
    }

    /// Attempt to connect to all registered servers that have `auto_connect` enabled.
    ///
    /// Returns a list of `(server_id, result)` pairs indicating success or failure
    /// for each server.
    pub async fn connect_all(&mut self) -> Vec<(String, Result<(), McpError>)> {
        let auto_ids: Vec<String> = self
            .configs
            .values()
            .filter(|c| c.auto_connect)
            .map(|c| c.id.clone())
            .collect();

        let mut results = Vec::new();
        for id in auto_ids {
            let result = self.connect(&id).await.map(|_| ());
            if let Err(ref e) = result {
                warn!(server_id = %id, error = %e, "Failed to auto-connect MCP server");
            }
            results.push((id, result));
        }
        results
    }

    /// List all registered server configurations.
    pub fn list_servers(&self) -> Vec<&McpServerConfig> {
        self.configs.values().collect()
    }

    /// Get a connected client by server ID.
    pub fn get_client(&self, server_id: &str) -> Option<&Arc<McpClient>> {
        self.clients.get(server_id)
    }

    /// Check whether a server is currently connected.
    pub fn is_connected(&self, server_id: &str) -> bool {
        self.clients.contains_key(server_id)
    }

    /// Disconnect from a specific server.
    pub async fn disconnect(&mut self, server_id: &str) -> Result<(), McpError> {
        if let Some(client) = self.clients.remove(server_id) {
            client.disconnect().await?;
            info!(server_id = %server_id, "Disconnected from MCP server");
            Ok(())
        } else {
            Err(McpError::ServerNotFound(server_id.to_string()))
        }
    }

    /// Disconnect from all connected servers.
    pub async fn disconnect_all(&mut self) {
        let ids: Vec<String> = self.clients.keys().cloned().collect();
        for id in ids {
            let _ = self.disconnect(&id).await;
        }
    }

    /// Return all tools from all connected servers, each tagged with its server ID.
    pub fn all_tools(&self) -> Vec<(String, McpToolDefinition)> {
        let mut tools = Vec::new();
        for (server_id, client) in &self.clients {
            for tool in client.tools() {
                tools.push((server_id.clone(), tool.clone()));
            }
        }
        tools
    }

    /// Iterate over (server_id, client) pairs for all connected servers.
    pub fn clients_iter(&self) -> impl Iterator<Item = (&str, &Arc<McpClient>)> {
        self.clients
            .iter()
            .map(|(id, client)| (id.as_str(), client))
    }
}

impl Default for McpServerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config(id: &str) -> McpServerConfig {
        McpServerConfig {
            id: id.to_string(),
            name: format!("Test Server {id}"),
            transport: McpTransportConfig::Sse {
                url: "http://localhost:9999".to_string(),
            },
            auto_connect: false,
            env: HashMap::new(),
        }
    }

    #[test]
    fn register_and_list() {
        let mut reg = McpServerRegistry::new();
        reg.register(sample_config("a")).unwrap();
        reg.register(sample_config("b")).unwrap();
        assert_eq!(reg.list_servers().len(), 2);
    }

    #[test]
    fn duplicate_register_fails() {
        let mut reg = McpServerRegistry::new();
        reg.register(sample_config("a")).unwrap();
        let err = reg.register(sample_config("a")).unwrap_err();
        assert!(matches!(err, McpError::AlreadyRegistered(_)));
    }

    #[test]
    fn get_client_when_not_connected() {
        let reg = McpServerRegistry::new();
        assert!(reg.get_client("nonexistent").is_none());
    }

    #[test]
    fn all_tools_empty_when_no_connections() {
        let reg = McpServerRegistry::new();
        assert!(reg.all_tools().is_empty());
    }

    #[test]
    fn config_serialization_stdio() {
        let config = McpServerConfig {
            id: "fs".into(),
            name: "Filesystem".into(),
            transport: McpTransportConfig::Stdio {
                command: "npx".into(),
                args: vec![
                    "-y".into(),
                    "@modelcontextprotocol/server-filesystem".into(),
                ],
            },
            auto_connect: true,
            env: HashMap::new(),
        };
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["transport"]["type"], "stdio");
        assert_eq!(json["transport"]["command"], "npx");
    }

    #[test]
    fn config_serialization_sse() {
        let config = McpServerConfig {
            id: "api".into(),
            name: "API Server".into(),
            transport: McpTransportConfig::Sse {
                url: "http://localhost:3000/mcp".into(),
            },
            auto_connect: false,
            env: HashMap::new(),
        };
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["transport"]["type"], "sse");
        assert_eq!(json["transport"]["url"], "http://localhost:3000/mcp");
    }
}
