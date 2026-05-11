//! Nexus as an MCP server — exposes Nexus capabilities as MCP tools.
//!
//! This module allows external MCP clients (e.g. Claude Desktop, Cursor, other agents)
//! to discover and invoke Nexus functionality through the standard MCP protocol.
//! The server can run in stdio mode (as a subprocess) or be embedded into the
//! HTTP server.

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, error, info};

use crate::error::McpError;
use crate::types::*;

/// An MCP server that exposes Nexus capabilities as tools.
///
/// Register tools at construction time, then either handle individual requests
/// via [`NexusMcpServer::handle_request`] or start a stdio server loop via
/// [`NexusMcpServer::serve_stdio`].
pub struct NexusMcpServer {
    /// Tools this server advertises to clients.
    tools: Vec<McpToolDefinition>,
    /// Server metadata.
    info: McpServerInfo,
}

impl NexusMcpServer {
    /// Create a new Nexus MCP server with the built-in tool set.
    pub fn new() -> Self {
        let tools = vec![
            McpToolDefinition {
                name: "nexus_generate".to_string(),
                description: "Generate a full application from a natural language prompt using Nexus oneshot pipeline.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "prompt": {
                            "type": "string",
                            "description": "Natural language description of the application to generate"
                        },
                        "project_id": {
                            "type": "string",
                            "description": "Optional existing project ID to generate into"
                        }
                    },
                    "required": ["prompt"]
                }),
            },
            McpToolDefinition {
                name: "nexus_run_agent".to_string(),
                description: "Run a single Nexus agent (architect, coder, tester, etc.) on a task.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_role": {
                            "type": "string",
                            "description": "Agent role: architect, coder, tester, debugger, reviewer, ux, devops, product, refactor, performance"
                        },
                        "task": {
                            "type": "string",
                            "description": "Task description for the agent"
                        },
                        "project_id": {
                            "type": "string",
                            "description": "Project ID to work in"
                        }
                    },
                    "required": ["agent_role", "task", "project_id"]
                }),
            },
            McpToolDefinition {
                name: "nexus_run_team".to_string(),
                description: "Execute a multi-agent team on a collaborative task.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "team_id": {
                            "type": "string",
                            "description": "Team definition ID"
                        },
                        "task": {
                            "type": "string",
                            "description": "Task for the team"
                        }
                    },
                    "required": ["team_id", "task"]
                }),
            },
            McpToolDefinition {
                name: "nexus_score_quality".to_string(),
                description: "Score the UI/UX quality of a generated application using the Taste Engine.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "project_id": {
                            "type": "string",
                            "description": "Project ID to score"
                        }
                    },
                    "required": ["project_id"]
                }),
            },
            McpToolDefinition {
                name: "nexus_analyze_intent".to_string(),
                description: "Analyze a natural language prompt to extract intent, complexity, and architecture recommendations.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "prompt": {
                            "type": "string",
                            "description": "The prompt to analyze"
                        }
                    },
                    "required": ["prompt"]
                }),
            },
            McpToolDefinition {
                name: "nexus_mutate".to_string(),
                description: "Apply an incremental mutation (edit) to an existing project.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "project_id": {
                            "type": "string",
                            "description": "Project ID to mutate"
                        },
                        "instruction": {
                            "type": "string",
                            "description": "Natural language description of the change"
                        }
                    },
                    "required": ["project_id", "instruction"]
                }),
            },
        ];

        let info = McpServerInfo {
            name: "nexus".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: "2024-11-05".to_string(),
            capabilities: McpCapabilities {
                tools: true,
                resources: false,
                prompts: false,
            },
        };

        Self { tools, info }
    }

    /// Return the tools this server exposes.
    pub fn tools(&self) -> &[McpToolDefinition] {
        &self.tools
    }

    /// Handle a single incoming MCP JSON-RPC request and produce a response.
    ///
    /// This is the core dispatch method. For `initialize` it returns server info;
    /// for `tools/list` it returns the tool catalog; for `tools/call` it delegates
    /// to the appropriate handler. Tool execution is currently stubbed —
    /// see `handle_tool_call` for the dispatch table that needs real wiring.
    /// Tracked: wire each tool stub to the corresponding nexus-http service.
    pub async fn handle_request(&self, request: McpRequest) -> McpResponse {
        debug!(method = %request.method, "Handling MCP request");

        match request.method.as_str() {
            "initialize" => {
                McpResponse::success(
                    request.id,
                    serde_json::to_value(&self.info).unwrap_or_default(),
                )
            }
            "notifications/initialized" => {
                // Notification — return an empty success.
                McpResponse::success(request.id, serde_json::json!({}))
            }
            "tools/list" => {
                McpResponse::success(
                    request.id,
                    serde_json::json!({ "tools": self.tools }),
                )
            }
            "tools/call" => self.handle_tool_call(request.id, request.params).await,
            _ => McpResponse::error(
                request.id,
                METHOD_NOT_FOUND,
                format!("Unknown method: {}", request.method),
            ),
        }
    }

    /// Handle a `tools/call` request.
    async fn handle_tool_call(
        &self,
        id: serde_json::Value,
        params: Option<serde_json::Value>,
    ) -> McpResponse {
        let params = match params {
            Some(p) => p,
            None => {
                return McpResponse::error(id, INVALID_PARAMS, "Missing params");
            }
        };

        let tool_name = match params.get("name").and_then(|n| n.as_str()) {
            Some(n) => n.to_string(),
            None => {
                return McpResponse::error(id, INVALID_PARAMS, "Missing tool name");
            }
        };

        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or(serde_json::json!({}));

        // Verify the tool exists.
        if !self.tools.iter().any(|t| t.name == tool_name) {
            return McpResponse::error(
                id,
                METHOD_NOT_FOUND,
                format!("Tool not found: {tool_name}"),
            );
        }

        // REVIEW: Tool execution stubs — real delegation happens at the HTTP
        // layer (mcp_handler.rs) where AppState is available. This standalone
        // MCP server returns placeholder responses until wired into the runtime.
        let result = serde_json::json!({
            "content": [{
                "type": "text",
                "text": format!(
                    "Tool '{tool_name}' invoked with arguments: {}. (Execution requires AppState wiring via nexus-http.)",
                    serde_json::to_string_pretty(&arguments).unwrap_or_default()
                )
            }]
        });

        McpResponse::success(id, result)
    }

    /// Start serving MCP requests over stdio (stdin/stdout).
    ///
    /// This is intended for use when Nexus is launched as a subprocess by an MCP
    /// client (e.g. Claude Desktop). The server reads newline-delimited JSON-RPC
    /// from stdin and writes responses to stdout.
    pub async fn serve_stdio(&self) -> Result<(), McpError> {
        info!("Starting Nexus MCP server in stdio mode");

        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();

        loop {
            let line = match lines.next_line().await {
                Ok(Some(line)) => line,
                Ok(None) => {
                    info!("stdin closed, shutting down MCP server");
                    break;
                }
                Err(e) => {
                    error!(error = %e, "Error reading stdin");
                    break;
                }
            };

            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }

            let request: McpRequest = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    let err_resp = McpResponse::error(
                        serde_json::Value::Null,
                        PARSE_ERROR,
                        format!("Parse error: {e}"),
                    );
                    let mut out = serde_json::to_string(&err_resp).unwrap_or_default();
                    out.push('\n');
                    let _ = stdout.write_all(out.as_bytes()).await;
                    let _ = stdout.flush().await;
                    continue;
                }
            };

            let response = self.handle_request(request).await;
            let mut out = serde_json::to_string(&response).unwrap_or_default();
            out.push('\n');
            stdout
                .write_all(out.as_bytes())
                .await
                .map_err(|e| McpError::Transport(format!("stdout write failed: {e}")))?;
            stdout
                .flush()
                .await
                .map_err(|e| McpError::Transport(format!("stdout flush failed: {e}")))?;
        }

        Ok(())
    }
}

impl Default for NexusMcpServer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_has_expected_tools() {
        let server = NexusMcpServer::new();
        let names: Vec<&str> = server.tools().iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"nexus_generate"));
        assert!(names.contains(&"nexus_run_agent"));
        assert!(names.contains(&"nexus_run_team"));
        assert!(names.contains(&"nexus_score_quality"));
        assert!(names.contains(&"nexus_analyze_intent"));
        assert!(names.contains(&"nexus_mutate"));
    }

    #[tokio::test]
    async fn handle_initialize() {
        let server = NexusMcpServer::new();
        let req = McpRequest::new("initialize", Some(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "clientInfo": { "name": "test", "version": "0.1" }
        })));
        let resp = server.handle_request(req).await;
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["name"], "nexus");
    }

    #[tokio::test]
    async fn handle_tools_list() {
        let server = NexusMcpServer::new();
        let req = McpRequest::new("tools/list", None);
        let resp = server.handle_request(req).await;
        assert!(resp.error.is_none());
        let tools = resp.result.unwrap()["tools"].as_array().unwrap().len();
        assert_eq!(tools, 6);
    }

    #[tokio::test]
    async fn handle_tool_call() {
        let server = NexusMcpServer::new();
        let req = McpRequest::new(
            "tools/call",
            Some(serde_json::json!({
                "name": "nexus_analyze_intent",
                "arguments": { "prompt": "Build a todo app" }
            })),
        );
        let resp = server.handle_request(req).await;
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert!(result["content"][0]["text"].as_str().unwrap().contains("nexus_analyze_intent"));
    }

    #[tokio::test]
    async fn handle_unknown_tool() {
        let server = NexusMcpServer::new();
        let req = McpRequest::new(
            "tools/call",
            Some(serde_json::json!({
                "name": "nonexistent_tool",
                "arguments": {}
            })),
        );
        let resp = server.handle_request(req).await;
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, METHOD_NOT_FOUND);
    }

    #[tokio::test]
    async fn handle_unknown_method() {
        let server = NexusMcpServer::new();
        let req = McpRequest::new("bogus/method", None);
        let resp = server.handle_request(req).await;
        assert!(resp.error.is_some());
    }
}
