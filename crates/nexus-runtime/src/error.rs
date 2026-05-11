use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("Budget exceeded: {0}")]
    BudgetExceeded(String),

    #[error("Process not found: {0}")]
    ProcessNotFound(String),

    #[error("Checkpoint error: {0}")]
    Checkpoint(String),

    #[error("Job queue error: {0}")]
    JobQueue(String),

    #[error("Coordinator error: {0}")]
    Coordinator(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Process timed out: {0}")]
    TimedOut(String),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, RuntimeError>;
