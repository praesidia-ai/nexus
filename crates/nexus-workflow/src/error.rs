//! Workflow error types.

use thiserror::Error;

/// All errors that can occur within the workflow engine.
#[derive(Debug, Error)]
pub enum WorkflowError {
    /// The requested workflow instance or step was not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// A workflow step failed after exhausting retries.
    #[error("step failed: {0}")]
    StepFailed(String),

    /// A step or workflow exceeded its timeout.
    #[error("timeout: {0}")]
    Timeout(String),

    /// Attempted to start a workflow that is already running.
    #[error("already running: {0}")]
    AlreadyRunning(String),

    /// The workflow was cancelled by the operator.
    #[error("cancelled: {0}")]
    Cancelled(String),

    /// Underlying storage (SQLite) error.
    #[error("storage error: {0}")]
    Storage(String),

    /// A cycle was detected in the workflow dependency graph.
    #[error("cycle detected: {0}")]
    CycleDetected(String),

    /// A step exceeded its maximum iteration count.
    #[error("max iterations exceeded: {0}")]
    MaxIterationsExceeded(String),

    /// Invalid workflow definition (e.g. missing step references).
    #[error("validation error: {0}")]
    ValidationError(String),
}

impl From<rusqlite::Error> for WorkflowError {
    fn from(e: rusqlite::Error) -> Self {
        WorkflowError::Storage(e.to_string())
    }
}

impl From<serde_json::Error> for WorkflowError {
    fn from(e: serde_json::Error) -> Self {
        WorkflowError::Storage(format!("json serialization error: {e}"))
    }
}
