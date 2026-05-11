//! Unified error type for the nexus-a2a crate.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum A2aError {
    #[error("Task not found: {0}")]
    TaskNotFound(String),

    #[error("Task cannot be canceled in state {0}")]
    TaskNotCancelable(String),

    #[error("Operation not supported: {0}")]
    UnsupportedOperation(String),

    #[error("Invalid agent response: {0}")]
    InvalidAgentResponse(String),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl A2aError {
    pub fn rpc_code(&self) -> i32 {
        match self {
            A2aError::TaskNotFound(_) => crate::types::error_codes::TASK_NOT_FOUND,
            A2aError::TaskNotCancelable(_) => crate::types::error_codes::TASK_NOT_CANCELABLE,
            A2aError::UnsupportedOperation(_) => crate::types::error_codes::UNSUPPORTED_OPERATION,
            A2aError::InvalidAgentResponse(_) => crate::types::error_codes::INVALID_AGENT_RESPONSE,
            _ => -32603, // Internal JSON-RPC error
        }
    }
}
