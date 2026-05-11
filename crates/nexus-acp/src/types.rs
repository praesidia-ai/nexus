//! Wire types for ACP JSON-RPC messages. We roll our own narrow
//! structs rather than pulling a full JSON-RPC crate — the protocol
//! surface is small and keeping this module dep-free lets callers
//! embed the adapter without implicit transitive deps.

use serde::{Deserialize, Serialize};

/// Any JSON-RPC 2.0 message. Requests, notifications, and responses
/// all flatten into this shape — the presence/absence of `method`,
/// `id`, and `result`/`error` disambiguates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcMessage {
    #[serde(rename = "jsonrpc")]
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcMessage {
    /// Build a success response matching an incoming request id.
    pub fn success(id: Option<serde_json::Value>, result: serde_json::Value) -> Self {
        Self {
            version: "2.0".into(),
            id,
            method: None,
            params: None,
            result: Some(result),
            error: None,
        }
    }

    /// Build an error response matching an incoming request id.
    pub fn error(id: Option<serde_json::Value>, err: JsonRpcError) -> Self {
        Self {
            version: "2.0".into(),
            id,
            method: None,
            params: None,
            result: None,
            error: Some(err),
        }
    }

    /// Build a notification (no `id`, no response expected).
    pub fn notification(method: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            version: "2.0".into(),
            id: None,
            method: Some(method.into()),
            params: Some(params),
            result: None,
            error: None,
        }
    }

    pub fn is_request(&self) -> bool {
        self.method.is_some() && self.id.is_some()
    }

    pub fn is_notification(&self) -> bool {
        self.method.is_some() && self.id.is_none()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcError {
    pub fn parse_error(msg: impl Into<String>) -> Self {
        Self {
            code: -32700,
            message: msg.into(),
            data: None,
        }
    }
    pub fn invalid_request(msg: impl Into<String>) -> Self {
        Self {
            code: -32600,
            message: msg.into(),
            data: None,
        }
    }
    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("method not found: {method}"),
            data: None,
        }
    }
    pub fn invalid_params(msg: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: msg.into(),
            data: None,
        }
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: msg.into(),
            data: None,
        }
    }
}

/// Upstream errors the caller might want to distinguish.
#[derive(Debug, thiserror::Error)]
pub enum AcpError {
    #[error("transport closed")]
    TransportClosed,
    #[error("malformed json: {0}")]
    MalformedJson(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
