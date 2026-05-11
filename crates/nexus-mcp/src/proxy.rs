//! MCP Proxy — multiplexes tool calls across all registered MCP servers.
//!
//! The proxy acts as a single MCP gateway: it aggregates tools from all
//! connected servers and routes each tool call to the correct upstream server.
//!
//! This enables:
//! - Clients to discover all tools from all servers in a single request
//! - Tool call routing based on tool name prefix (`server_id:tool_name`)
//! - Fallback search by tool name alone when no prefix is provided

use tracing::debug;

use crate::error::McpError;
use crate::registry::McpServerRegistry;
use crate::types::McpToolDefinition;

/// Aggregated view of all tools across all connected MCP servers.
#[derive(Debug, Clone)]
pub struct ProxyTool {
    /// The server that provides this tool.
    pub server_id: String,
    /// Original tool definition.
    pub tool: McpToolDefinition,
    /// Qualified name: `{server_id}:{tool_name}` for disambiguation.
    pub qualified_name: String,
}

/// A composable tool chain — an ordered sequence of tools where the output
/// of each becomes context for the next.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolChain {
    pub name: String,
    pub description: String,
    /// Ordered list of `{server_id}:{tool_name}` references.
    pub steps: Vec<ToolChainStep>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolChainStep {
    /// Qualified tool name.
    pub tool: String,
    /// Optional static arguments merged with runtime input.
    #[serde(default)]
    pub static_args: serde_json::Value,
    /// When true, the output of this step is passed as context to the next step.
    #[serde(default = "bool_true")]
    pub chain_output: bool,
}

fn bool_true() -> bool {
    true
}

/// Route a tool call through the registry.
///
/// `tool_name` can be:
/// - `server_id:tool_name` — routed to the specific server
/// - `tool_name` — first matching server is used
pub async fn route_tool_call(
    registry: &McpServerRegistry,
    tool_name: &str,
    arguments: serde_json::Value,
) -> Result<serde_json::Value, McpError> {
    let (server_id, bare_tool_name) = if let Some((sid, tn)) = tool_name.split_once(':') {
        (Some(sid.to_string()), tn.to_string())
    } else {
        (None, tool_name.to_string())
    };

    if let Some(sid) = server_id {
        let client = registry
            .get_client(&sid)
            .ok_or_else(|| McpError::ServerNotFound(sid.clone()))?
            .clone();
        debug!(server = %sid, tool = %bare_tool_name, "Routing tool call to specific server");
        return client.call_tool(&bare_tool_name, arguments).await;
    }

    // Search all connected servers for a matching tool
    for (sid, client) in registry.clients_iter() {
        if client.has_tool(&bare_tool_name) {
            debug!(server = %sid, tool = %bare_tool_name, "Routing tool call to first matching server");
            return client.call_tool(&bare_tool_name, arguments).await;
        }
    }

    Err(McpError::ToolNotFound(bare_tool_name))
}

/// Execute a tool chain, threading outputs through as context.
pub async fn execute_tool_chain(
    registry: &McpServerRegistry,
    chain: &ToolChain,
    initial_input: serde_json::Value,
) -> Result<Vec<serde_json::Value>, McpError> {
    let mut results = Vec::new();
    let mut context = initial_input;

    for step in &chain.steps {
        // Merge static args with current context
        let args = if let (Some(obj), Some(static_obj)) = (
            context.as_object(),
            step.static_args.as_object(),
        ) {
            let mut merged = obj.clone();
            for (k, v) in static_obj {
                merged.insert(k.clone(), v.clone());
            }
            serde_json::Value::Object(merged)
        } else if step.static_args.is_object() {
            let mut merged = step.static_args.as_object().unwrap().clone();
            merged.insert("input".into(), context.clone());
            serde_json::Value::Object(merged)
        } else {
            serde_json::json!({ "input": context })
        };

        let result = route_tool_call(registry, &step.tool, args).await?;

        if step.chain_output {
            context = result.clone();
        }

        results.push(result);
    }

    Ok(results)
}

/// Build the aggregated list of all tools across connected servers.
pub fn all_proxy_tools(registry: &McpServerRegistry) -> Vec<ProxyTool> {
    registry
        .all_tools()
        .into_iter()
        .map(|(server_id, tool)| {
            let qualified_name = format!("{}:{}", server_id, tool.name);
            ProxyTool {
                server_id,
                tool,
                qualified_name,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Auto-discovery: scan well-known local MCP server ports
// ---------------------------------------------------------------------------

/// Scan localhost for running MCP SSE servers on common ports.
/// Returns discovered server URLs.
pub async fn discover_local_servers() -> Vec<String> {
    let candidate_ports: Vec<u16> = (3100..=3120).chain([9000, 9001, 9002]).collect();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(200))
        .build()
        .unwrap_or_default();

    let mut found = Vec::new();
    for port in candidate_ports {
        let url = format!("http://localhost:{}/mcp", port);
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                found.push(url);
            }
        }
    }
    found
}
