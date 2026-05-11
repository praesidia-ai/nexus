//! MCP client — connects to external MCP servers via stdio or SSE transports.
//!
//! The [`McpClient`] manages the lifecycle of a connection to a single MCP server:
//! initialization handshake, tool discovery, tool invocation, resource reading, and
//! graceful disconnect.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{debug, info};

use crate::error::McpError;
use crate::types::*;

/// Handle to the underlying transport, abstracted over stdio and SSE.
enum TransportInner {
    /// Communicates with a child process via stdin/stdout JSON-RPC lines.
    Stdio {
        child: Box<Child>,
        stdin: tokio::process::ChildStdin,
        reader: BufReader<tokio::process::ChildStdout>,
    },
    /// Communicates via HTTP POST (requests) and SSE stream (responses).
    Sse {
        base_url: String,
        http: reqwest::Client,
    },
}

/// Thread-safe handle to the transport layer.
struct McpTransportHandle {
    inner: Mutex<TransportInner>,
}

impl McpTransportHandle {
    /// Send a JSON-RPC request and wait for the matching response.
    async fn send_request(&self, request: &McpRequest) -> Result<McpResponse, McpError> {
        let mut inner = self.inner.lock().await;
        match &mut *inner {
            TransportInner::Stdio {
                stdin, reader, ..
            } => {
                let mut line = serde_json::to_string(request)?;
                line.push('\n');
                stdin
                    .write_all(line.as_bytes())
                    .await
                    .map_err(|e| McpError::Transport(format!("stdin write failed: {e}")))?;
                stdin
                    .flush()
                    .await
                    .map_err(|e| McpError::Transport(format!("stdin flush failed: {e}")))?;

                let mut buf = String::new();
                let n = reader
                    .read_line(&mut buf)
                    .await
                    .map_err(|e| McpError::Transport(format!("stdout read failed: {e}")))?;
                if n == 0 {
                    return Err(McpError::Disconnected);
                }
                let resp: McpResponse = serde_json::from_str(buf.trim())?;
                Ok(resp)
            }
            TransportInner::Sse { base_url, http } => {
                let resp = http
                    .post(base_url.as_str())
                    .json(request)
                    .send()
                    .await?;
                if !resp.status().is_success() {
                    return Err(McpError::Transport(format!(
                        "HTTP {} from MCP server",
                        resp.status()
                    )));
                }
                let body = resp.text().await?;
                let mcp_resp: McpResponse = serde_json::from_str(&body)?;
                Ok(mcp_resp)
            }
        }
    }

    /// Shut down the transport (kill child process or drop HTTP client).
    async fn shutdown(&self) -> Result<(), McpError> {
        let mut inner = self.inner.lock().await;
        match &mut *inner {
            TransportInner::Stdio { child, .. } => {
                let _ = child.kill().await;
                Ok(())
            }
            TransportInner::Sse { .. } => Ok(()),
        }
    }
}

/// Client for communicating with a single MCP server.
///
/// After construction via [`McpClient::connect_stdio`] or [`McpClient::connect_sse`],
/// call methods to discover and invoke tools, read resources, or list prompts.
pub struct McpClient {
    /// Identifier for this server connection (user-assigned).
    server_id: String,
    /// Server metadata received during initialization.
    server_info: Option<McpServerInfo>,
    /// Cached list of tools offered by this server.
    tools: Vec<McpToolDefinition>,
    /// Cached list of resources offered by this server.
    #[allow(dead_code)]
    resources: Vec<McpResource>,
    /// Transport handle for sending JSON-RPC messages.
    transport: Arc<McpTransportHandle>,
}

impl McpClient {
    /// Connect to an MCP server by spawning a child process and communicating via
    /// its stdin/stdout using newline-delimited JSON-RPC.
    ///
    /// # Arguments
    /// * `server_id` — A user-chosen identifier for this connection.
    /// * `command` — The executable to spawn (e.g. `"npx"`).
    /// * `args` — Arguments passed to the command.
    /// * `env` — Optional additional environment variables.
    pub async fn connect_stdio(
        server_id: impl Into<String>,
        command: &str,
        args: &[String],
        env: Option<&HashMap<String, String>>,
    ) -> Result<Self, McpError> {
        let server_id = server_id.into();
        info!(server_id = %server_id, command = %command, "Spawning MCP server via stdio");

        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(env_vars) = env {
            for (k, v) in env_vars {
                cmd.env(k, v);
            }
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| McpError::ConnectionFailed(format!("Failed to spawn {command}: {e}")))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::ConnectionFailed("No stdin on child".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::ConnectionFailed("No stdout on child".into()))?;
        let reader = BufReader::new(stdout);

        let transport = Arc::new(McpTransportHandle {
            inner: Mutex::new(TransportInner::Stdio {
                child: Box::new(child),
                stdin,
                reader,
            }),
        });

        let mut client = Self {
            server_id,
            server_info: None,
            tools: Vec::new(),
            resources: Vec::new(),
            transport,
        };

        client.initialize().await?;
        Ok(client)
    }

    /// Connect to an MCP server via HTTP/SSE transport.
    ///
    /// Requests are sent as HTTP POST to `url` and responses come back in the
    /// response body (or via a server-sent events stream for notifications).
    ///
    /// # Arguments
    /// * `server_id` — A user-chosen identifier for this connection.
    /// * `url` — The base URL of the MCP server's HTTP endpoint.
    pub async fn connect_sse(
        server_id: impl Into<String>,
        url: &str,
    ) -> Result<Self, McpError> {
        let server_id = server_id.into();
        info!(server_id = %server_id, url = %url, "Connecting to MCP server via SSE");

        let http = reqwest::Client::new();
        let transport = Arc::new(McpTransportHandle {
            inner: Mutex::new(TransportInner::Sse {
                base_url: url.to_string(),
                http,
            }),
        });

        let mut client = Self {
            server_id,
            server_info: None,
            tools: Vec::new(),
            resources: Vec::new(),
            transport,
        };

        client.initialize().await?;
        Ok(client)
    }

    /// Perform the MCP initialization handshake and cache server capabilities.
    async fn initialize(&mut self) -> Result<(), McpError> {
        debug!(server_id = %self.server_id, "Sending initialize request");

        let req = McpRequest::new(
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "nexus-mcp",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
        );

        let resp = self.transport.send_request(&req).await?;
        if let Some(err) = resp.error {
            return Err(McpError::Protocol(format!(
                "Initialize failed: {} (code {})",
                err.message, err.code
            )));
        }

        if let Some(result) = resp.result {
            self.server_info = serde_json::from_value(result).ok();
        }

        // Send initialized notification (no id — it is a notification).
        let notif = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::Value::Null,
            method: "notifications/initialized".to_string(),
            params: None,
        };
        // Best effort — some transports may not support fire-and-forget well.
        let _ = self.transport.send_request(&notif).await;

        // Discover tools.
        self.refresh_tools().await?;

        info!(
            server_id = %self.server_id,
            tools = self.tools.len(),
            "MCP server initialized"
        );
        Ok(())
    }

    /// Re-fetch the tool list from the server and update the local cache.
    pub async fn refresh_tools(&mut self) -> Result<(), McpError> {
        let req = McpRequest::new("tools/list", None);
        let resp = self.transport.send_request(&req).await?;
        if let Some(result) = resp.result {
            if let Some(tools) = result.get("tools") {
                self.tools = serde_json::from_value(tools.clone()).unwrap_or_default();
            }
        }
        Ok(())
    }

    /// Return the cached list of tools.
    pub fn tools(&self) -> &[McpToolDefinition] {
        &self.tools
    }

    /// Check whether a tool with the given name is available on this server.
    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.iter().any(|t| t.name == name)
    }

    /// Return the server info received during initialization.
    pub fn server_info(&self) -> Option<&McpServerInfo> {
        self.server_info.as_ref()
    }

    /// Return this client's server ID.
    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    /// Call a tool on the remote MCP server by name.
    ///
    /// # Arguments
    /// * `name` — The tool name (must match one of the names from [`McpClient::tools`]).
    /// * `arguments` — JSON object with the tool's input arguments.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        // Verify the tool exists locally before making the RPC call.
        if !self.tools.iter().any(|t| t.name == name) {
            return Err(McpError::ToolNotFound(name.to_string()));
        }

        debug!(server_id = %self.server_id, tool = %name, "Calling MCP tool");

        let req = McpRequest::new(
            "tools/call",
            Some(serde_json::json!({
                "name": name,
                "arguments": arguments,
            })),
        );

        let resp = self.transport.send_request(&req).await?;
        if let Some(err) = resp.error {
            return Err(McpError::ToolFailed(format!(
                "{} (code {})",
                err.message, err.code
            )));
        }

        resp.result.ok_or_else(|| {
            McpError::Protocol("Tool call returned neither result nor error".into())
        })
    }

    /// Read a resource from the remote MCP server by URI.
    pub async fn read_resource(&self, uri: &str) -> Result<String, McpError> {
        debug!(server_id = %self.server_id, uri = %uri, "Reading MCP resource");

        let req = McpRequest::new(
            "resources/read",
            Some(serde_json::json!({ "uri": uri })),
        );

        let resp = self.transport.send_request(&req).await?;
        if let Some(err) = resp.error {
            return Err(McpError::ToolFailed(format!(
                "Resource read failed: {} (code {})",
                err.message, err.code
            )));
        }

        if let Some(result) = resp.result {
            // MCP returns { contents: [{ text: "..." }] }
            if let Some(contents) = result.get("contents") {
                if let Some(first) = contents.as_array().and_then(|a| a.first()) {
                    if let Some(text) = first.get("text").and_then(|t| t.as_str()) {
                        return Ok(text.to_string());
                    }
                }
            }
            // Fall back to stringified result.
            Ok(result.to_string())
        } else {
            Err(McpError::Protocol(
                "Resource read returned neither result nor error".into(),
            ))
        }
    }

    /// List available prompts from the server.
    pub async fn list_prompts(&self) -> Result<Vec<McpPrompt>, McpError> {
        let req = McpRequest::new("prompts/list", None);
        let resp = self.transport.send_request(&req).await?;

        if let Some(err) = resp.error {
            return Err(McpError::Protocol(format!(
                "Prompts list failed: {} (code {})",
                err.message, err.code
            )));
        }

        if let Some(result) = resp.result {
            if let Some(prompts) = result.get("prompts") {
                return Ok(serde_json::from_value(prompts.clone()).unwrap_or_default());
            }
        }
        Ok(Vec::new())
    }

    /// List available resources from the server.
    pub async fn list_resources(&self) -> Result<Vec<McpResource>, McpError> {
        let req = McpRequest::new("resources/list", None);
        let resp = self.transport.send_request(&req).await?;

        if let Some(err) = resp.error {
            return Err(McpError::Protocol(format!(
                "Resources list failed: {} (code {})",
                err.message, err.code
            )));
        }

        if let Some(result) = resp.result {
            if let Some(resources) = result.get("resources") {
                let parsed: Vec<McpResource> =
                    serde_json::from_value(resources.clone()).unwrap_or_default();
                return Ok(parsed);
            }
        }
        Ok(Vec::new())
    }

    /// Gracefully disconnect from the MCP server.
    pub async fn disconnect(&self) -> Result<(), McpError> {
        info!(server_id = %self.server_id, "Disconnecting from MCP server");
        self.transport.shutdown().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_fields_accessible() {
        // Compile-time check that the public API is well-formed.
        // Runtime connection tests require a live MCP server, so we only
        // verify the type structure here.
        let _: fn(&McpClient) -> &[McpToolDefinition] = McpClient::tools;
        let _: fn(&McpClient) -> Option<&McpServerInfo> = McpClient::server_info;
        let _: fn(&McpClient) -> &str = McpClient::server_id;
    }
}
