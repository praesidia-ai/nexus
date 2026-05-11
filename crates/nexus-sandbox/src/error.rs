//! Error types for sandbox operations.

use thiserror::Error;

/// Errors that can occur during sandbox lifecycle operations.
#[derive(Debug, Error)]
pub enum SandboxError {
    /// Failed to create a new sandbox instance.
    #[error("failed to create sandbox: {0}")]
    Create(String),

    /// Failed to execute a command inside the sandbox.
    #[error("execution failed: {0}")]
    Exec(String),

    /// Failed to copy files into or out of the sandbox.
    #[error("file copy failed: {0}")]
    CopyFailed(String),

    /// Command execution exceeded the configured timeout.
    #[error("execution timed out after {0} seconds")]
    Timeout(u64),

    /// The sandbox backend is not available on this system.
    #[error("sandbox backend not available: {0}")]
    NotAvailable(String),

    /// The sandbox instance has already been destroyed.
    #[error("sandbox {0} has been destroyed")]
    Destroyed(String),
}
