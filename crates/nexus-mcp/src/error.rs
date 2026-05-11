//! Error types for the MCP integration layer.
//!
//! All MCP operations surface errors through [`McpError`], which covers transport
//! failures, protocol violations, tool lookup/execution problems, and timeouts.

/// Unified error type for all MCP client and server operations.
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    /// The underlying transport (stdio pipe, HTTP/SSE stream) encountered an I/O error.
    #[error("Transport error: {0}")]
    Transport(String),

    /// The remote side sent a message that violates the JSON-RPC / MCP protocol.
    #[error("Protocol error: {0}")]
    Protocol(String),

    /// A tool was requested by name but does not exist on the connected server.
    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    /// The tool was found but its execution returned an error.
    #[error("Tool execution failed: {0}")]
    ToolFailed(String),

    /// Could not establish a connection to the MCP server.
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// The operation exceeded the configured deadline.
    #[error("Timeout")]
    Timeout,

    /// The server closed the connection unexpectedly.
    #[error("Server disconnected")]
    Disconnected,

    /// A server with the given ID was not found in the registry.
    #[error("Server not found: {0}")]
    ServerNotFound(String),

    /// A server with the given ID is already registered.
    #[error("Server already registered: {0}")]
    AlreadyRegistered(String),

    /// JSON serialization or deserialization failed.
    #[error("Serialization error: {0}")]
    Serialization(String),
}

impl From<serde_json::Error> for McpError {
    fn from(err: serde_json::Error) -> Self {
        McpError::Serialization(err.to_string())
    }
}

impl From<reqwest::Error> for McpError {
    fn from(err: reqwest::Error) -> Self {
        McpError::Transport(err.to_string())
    }
}

impl From<std::io::Error> for McpError {
    fn from(err: std::io::Error) -> Self {
        McpError::Transport(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_messages() {
        let err = McpError::Transport("pipe broken".into());
        assert_eq!(err.to_string(), "Transport error: pipe broken");

        let err = McpError::ToolNotFound("my_tool".into());
        assert_eq!(err.to_string(), "Tool not found: my_tool");

        let err = McpError::Timeout;
        assert_eq!(err.to_string(), "Timeout");

        let err = McpError::Disconnected;
        assert_eq!(err.to_string(), "Server disconnected");
    }
}
