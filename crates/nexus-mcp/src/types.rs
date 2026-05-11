//! MCP protocol types following the Model Context Protocol specification.
//!
//! This module defines the wire-format types for JSON-RPC messages exchanged
//! between MCP clients and servers, as well as the domain objects (tools,
//! resources, prompts) that the protocol exposes.

use serde::{Deserialize, Serialize};

// ─── Server metadata ────────────────────────────────────────────────────────

/// Information about an MCP server, returned during the `initialize` handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerInfo {
    /// Human-readable server name.
    pub name: String,
    /// SemVer version string.
    pub version: String,
    /// Protocol version the server speaks (e.g. `"2024-11-05"`).
    #[serde(default)]
    pub protocol_version: String,
    /// Capabilities the server advertises.
    #[serde(default)]
    pub capabilities: McpCapabilities,
}

/// Capabilities an MCP server may advertise during initialization.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCapabilities {
    /// Whether the server supports tool calls.
    #[serde(default)]
    pub tools: bool,
    /// Whether the server exposes readable resources.
    #[serde(default)]
    pub resources: bool,
    /// Whether the server provides prompt templates.
    #[serde(default)]
    pub prompts: bool,
}

// ─── Tools ──────────────────────────────────────────────────────────────────

/// Definition of a single tool exposed by an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolDefinition {
    /// Unique tool name (within the server).
    pub name: String,
    /// Human-readable description of what the tool does.
    #[serde(default)]
    pub description: String,
    /// JSON Schema describing the tool's expected input.
    #[serde(default)]
    pub input_schema: serde_json::Value,
}

// ─── Resources ──────────────────────────────────────────────────────────────

/// A resource that can be read from an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpResource {
    /// URI identifying the resource (e.g. `file:///path/to/file`).
    pub uri: String,
    /// Human-readable name.
    pub name: String,
    /// Optional description.
    #[serde(default)]
    pub description: String,
    /// MIME type of the resource content.
    #[serde(default)]
    pub mime_type: String,
}

// ─── Prompts ────────────────────────────────────────────────────────────────

/// A prompt template exposed by an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPrompt {
    /// Unique prompt name.
    pub name: String,
    /// Human-readable description.
    #[serde(default)]
    pub description: String,
    /// Arguments the prompt template accepts.
    #[serde(default)]
    pub arguments: Vec<McpPromptArgument>,
}

/// A single argument accepted by a prompt template.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPromptArgument {
    /// Argument name.
    pub name: String,
    /// Human-readable description.
    #[serde(default)]
    pub description: String,
    /// Whether the argument is required.
    #[serde(default)]
    pub required: bool,
}

// ─── Content types ──────────────────────────────────────────────────────────

/// A message in an MCP conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpMessage {
    /// Role of the sender (`"user"`, `"assistant"`, `"system"`).
    pub role: String,
    /// Content blocks.
    pub content: Vec<McpContent>,
}

/// A single content block within an [`McpMessage`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum McpContent {
    /// Plain text content.
    #[serde(rename = "text")]
    Text {
        /// The text value.
        text: String,
    },
    /// Base64-encoded image content.
    #[serde(rename = "image")]
    Image {
        /// Base64-encoded image data.
        data: String,
        /// MIME type (e.g. `"image/png"`).
        mime_type: String,
    },
    /// Embedded resource reference.
    #[serde(rename = "resource")]
    Resource {
        /// The resource being referenced.
        resource: McpResource,
    },
}

// ─── JSON-RPC wire types ────────────────────────────────────────────────────

/// A JSON-RPC 2.0 request sent to an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRequest {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// Request identifier — the server echoes this in its response.
    pub id: serde_json::Value,
    /// The RPC method name (e.g. `"tools/call"`, `"initialize"`).
    pub method: String,
    /// Optional parameters for the method.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl McpRequest {
    /// Create a new JSON-RPC request with an auto-generated numeric ID.
    pub fn new(method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self {
            jsonrpc: "2.0".to_string(),
            id: serde_json::Value::Number(COUNTER.fetch_add(1, Ordering::Relaxed).into()),
            method: method.into(),
            params,
        }
    }
}

/// A JSON-RPC 2.0 response received from an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResponse {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// Echoed request identifier.
    pub id: serde_json::Value,
    /// The result payload (present on success).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// The error payload (present on failure).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<McpRpcError>,
}

impl McpResponse {
    /// Create a successful response with the given result.
    pub fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response.
    pub fn error(id: serde_json::Value, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(McpRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRpcError {
    /// Numeric error code.
    pub code: i64,
    /// Human-readable error message.
    pub message: String,
    /// Optional additional data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// ─── JSON-RPC error codes (standard + MCP-specific) ─────────────────────────

/// Standard JSON-RPC parse error.
pub const PARSE_ERROR: i64 = -32700;
/// Standard JSON-RPC invalid request.
pub const INVALID_REQUEST: i64 = -32600;
/// Standard JSON-RPC method not found.
pub const METHOD_NOT_FOUND: i64 = -32601;
/// Standard JSON-RPC invalid params.
pub const INVALID_PARAMS: i64 = -32602;
/// Standard JSON-RPC internal error.
pub const INTERNAL_ERROR: i64 = -32603;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_definition_roundtrip() {
        let tool = McpToolDefinition {
            name: "read_file".to_string(),
            description: "Read a file from disk".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path" }
                },
                "required": ["path"]
            }),
        };
        let json = serde_json::to_string(&tool).unwrap();
        let parsed: McpToolDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "read_file");
        assert_eq!(parsed.description, "Read a file from disk");
    }

    #[test]
    fn request_response_json_rpc_format() {
        let req = McpRequest::new(
            "tools/call",
            Some(serde_json::json!({ "name": "read_file", "arguments": { "path": "/tmp/x" } })),
        );
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["method"], "tools/call");
        assert!(json["id"].is_number());

        let resp = McpResponse::success(
            req.id.clone(),
            serde_json::json!({ "content": [{ "type": "text", "text": "hello" }] }),
        );
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert!(json["error"].is_null());
        assert!(json["result"].is_object());
    }

    #[test]
    fn error_response_format() {
        let resp = McpResponse::error(
            serde_json::json!(1),
            METHOD_NOT_FOUND,
            "Method not found",
        );
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["error"]["code"], METHOD_NOT_FOUND);
        assert_eq!(json["error"]["message"], "Method not found");
        assert!(json["result"].is_null());
    }

    #[test]
    fn mcp_content_tagged_serialization() {
        let text = McpContent::Text {
            text: "hello world".to_string(),
        };
        let json = serde_json::to_value(&text).unwrap();
        assert_eq!(json["type"], "text");
        assert_eq!(json["text"], "hello world");

        let img = McpContent::Image {
            data: "aGVsbG8=".to_string(),
            mime_type: "image/png".to_string(),
        };
        let json = serde_json::to_value(&img).unwrap();
        assert_eq!(json["type"], "image");
    }

    #[test]
    fn server_info_deserialization() {
        let json = serde_json::json!({
            "name": "test-server",
            "version": "1.0.0",
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": true, "resources": false, "prompts": true }
        });
        let info: McpServerInfo = serde_json::from_value(json).unwrap();
        assert_eq!(info.name, "test-server");
        assert!(info.capabilities.tools);
        assert!(!info.capabilities.resources);
        assert!(info.capabilities.prompts);
    }
}
