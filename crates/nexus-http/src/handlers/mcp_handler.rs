//! MCP (Model Context Protocol) HTTP handlers.
//!
//! Exposes endpoints for managing MCP server connections and browsing their tools.
//! The registry lives in `AppState` behind an `RwLock` so multiple readers can
//! list servers/tools concurrently while writes (register, connect, disconnect)
//! take exclusive access.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use nexus_mcp::adapter::McpToolInfo;
use nexus_mcp::registry::{McpServerConfig, McpTransportConfig};

use crate::state::AppState;

// ─── Request / Response types ───────────────────────────────────────────────

/// Request body for registering a new MCP server.
#[derive(Debug, Deserialize)]
pub struct RegisterServerRequest {
    /// Unique identifier for the server.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Transport configuration.
    pub transport: McpTransportConfig,
    /// Whether to auto-connect on registration.
    #[serde(default)]
    pub auto_connect: bool,
    /// Optional environment variables for stdio transport.
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

/// Summary of a registered MCP server.
#[derive(Debug, Serialize)]
pub struct ServerSummary {
    /// Server ID.
    pub id: String,
    /// Server name.
    pub name: String,
    /// Whether the server is currently connected.
    pub connected: bool,
    /// Number of tools available (0 if not connected).
    pub tool_count: usize,
}

/// Response for tool listing endpoints.
#[derive(Debug, Serialize)]
pub struct ToolListResponse {
    /// Total number of tools.
    pub count: usize,
    /// The tools.
    pub tools: Vec<McpToolInfo>,
}

// ─── Handlers ───────────────────────────────────────────────────────────────

/// `POST /mcp/servers` — Register a new MCP server configuration.
pub async fn register_server(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterServerRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let config = McpServerConfig {
        id: req.id.clone(),
        name: req.name,
        transport: req.transport,
        auto_connect: req.auto_connect,
        env: req.env,
    };

    let mut registry = state.mcp_registry.write().await;
    registry.register(config).map_err(|e| {
        (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;

    // Auto-connect if requested.
    if req.auto_connect {
        if let Err(e) = registry.connect(&req.id).await {
            tracing::warn!(server_id = %req.id, error = %e, "Auto-connect failed");
        }
    }

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "status": "registered", "id": req.id })),
    ))
}

/// `GET /mcp/servers` — List all registered MCP servers.
pub async fn list_servers(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<ServerSummary>> {
    let registry = state.mcp_registry.read().await;
    let servers = registry
        .list_servers()
        .iter()
        .map(|config| {
            let connected = registry.is_connected(&config.id);
            let tool_count = if connected {
                registry
                    .get_client(&config.id)
                    .map(|c| c.tools().len())
                    .unwrap_or(0)
            } else {
                0
            };
            ServerSummary {
                id: config.id.clone(),
                name: config.name.clone(),
                connected,
                tool_count,
            }
        })
        .collect();
    Json(servers)
}

/// `POST /mcp/servers/:id/connect` — Connect to a registered MCP server.
pub async fn connect_server(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut registry = state.mcp_registry.write().await;
    registry.connect(&server_id).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;

    let tool_count = registry
        .get_client(&server_id)
        .map(|c| c.tools().len())
        .unwrap_or(0);

    Ok(Json(serde_json::json!({
        "status": "connected",
        "server_id": server_id,
        "tool_count": tool_count,
    })))
}

/// `GET /mcp/servers/:id/tools` — List tools from a specific connected server.
pub async fn list_server_tools(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
) -> Result<Json<ToolListResponse>, (StatusCode, Json<serde_json::Value>)> {
    let registry = state.mcp_registry.read().await;
    let client = registry.get_client(&server_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("Server '{}' not connected", server_id) })),
        )
    })?;

    let tools: Vec<McpToolInfo> = client
        .tools()
        .iter()
        .map(|t| McpToolInfo {
            server_id: server_id.clone(),
            name: t.name.clone(),
            description: t.description.clone(),
            input_schema: t.input_schema.clone(),
        })
        .collect();

    Ok(Json(ToolListResponse {
        count: tools.len(),
        tools,
    }))
}

/// `DELETE /mcp/servers/:id` — Unregister an MCP server (disconnects if connected).
pub async fn remove_server(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut registry = state.mcp_registry.write().await;
    registry.unregister(&server_id).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;

    Ok(Json(serde_json::json!({
        "status": "removed",
        "server_id": server_id,
    })))
}

/// Request body for calling a tool on a connected MCP server.
#[derive(Debug, Deserialize)]
pub struct CallToolRequest {
    /// The tool name.
    pub name: String,
    /// Tool arguments as a JSON object.
    pub arguments: serde_json::Value,
}

/// `POST /mcp/servers/:id/tools/call` — Invoke a tool on a connected MCP server.
pub async fn call_tool(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
    Json(req): Json<CallToolRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // Clone the Arc<McpClient> while holding the read lock so the lock is
    // released before the async call_tool invocation.
    let client = {
        let registry = state.mcp_registry.read().await;
        registry
            .get_client(&server_id)
            .cloned()
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({
                        "error": format!("Server '{}' not connected", server_id)
                    })),
                )
            })?
    };

    client.call_tool(&req.name, req.arguments).await.map(Json).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })
}

/// `GET /mcp/tools` — List all tools from all connected MCP servers.
pub async fn list_all_tools(
    State(state): State<Arc<AppState>>,
) -> Json<ToolListResponse> {
    let registry = state.mcp_registry.read().await;
    let tools: Vec<McpToolInfo> = registry
        .all_tools()
        .into_iter()
        .map(|(server_id, t)| McpToolInfo {
            server_id,
            name: t.name,
            description: t.description,
            input_schema: t.input_schema,
        })
        .collect();

    Json(ToolListResponse {
        count: tools.len(),
        tools,
    })
}

// ---------------------------------------------------------------------------
// MCP Proxy: route any tool call by qualified name (server_id:tool_name)
// ---------------------------------------------------------------------------

/// Request body for the proxy tool call endpoint.
#[derive(Debug, Deserialize)]
pub struct ProxyCallRequest {
    /// Qualified tool name: `server_id:tool_name` or just `tool_name`.
    pub tool: String,
    /// Tool arguments.
    pub arguments: serde_json::Value,
}

/// `POST /mcp/proxy/call` — Route a tool call via the proxy (auto-resolves server).
pub async fn proxy_call(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ProxyCallRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let registry = state.mcp_registry.read().await;
    nexus_mcp::proxy::route_tool_call(&registry, &req.tool, req.arguments)
        .await
        .map(Json)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })
}

// ---------------------------------------------------------------------------
// MCP Auto-discovery
// ---------------------------------------------------------------------------

/// `POST /mcp/discover` — Scan localhost for running MCP servers and auto-register.
pub async fn discover_local(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let found = nexus_mcp::proxy::discover_local_servers().await;
    let mut registered = Vec::new();

    for url in &found {
        let id = format!("auto-{}", uuid::Uuid::new_v4());
        let config = nexus_mcp::registry::McpServerConfig {
            id: id.clone(),
            name: format!("Auto-discovered: {url}"),
            transport: nexus_mcp::registry::McpTransportConfig::Sse { url: url.clone() },
            auto_connect: true,
            env: Default::default(),
        };
        let mut registry = state.mcp_registry.write().await;
        if registry.register(config).is_ok() {
            let _ = registry.connect(&id).await;
            registered.push(serde_json::json!({ "id": id, "url": url }));
        }
    }

    Json(serde_json::json!({
        "discovered": found.len(),
        "registered": registered,
    }))
}
