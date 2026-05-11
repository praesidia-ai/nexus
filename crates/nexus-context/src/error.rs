#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Compression error: {0}")]
    Compression(String),

    #[error("Context overflow: current {current} exceeds max {max}")]
    Overflow { current: usize, max: usize },
}

impl From<rusqlite::Error> for ContextError {
    fn from(e: rusqlite::Error) -> Self {
        ContextError::Storage(e.to_string())
    }
}

impl From<serde_json::Error> for ContextError {
    fn from(e: serde_json::Error) -> Self {
        ContextError::Storage(e.to_string())
    }
}
