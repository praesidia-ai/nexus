#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Duplicate entity: {0}")]
    Duplicate(String),

    #[error("Invalid relation: {0}")]
    InvalidRelation(String),

    #[error("Extraction error: {0}")]
    Extraction(String),

    #[error("Contradiction: {0}")]
    Contradiction(String),
}

impl From<rusqlite::Error> for GraphError {
    fn from(e: rusqlite::Error) -> Self {
        GraphError::Storage(e.to_string())
    }
}

impl From<serde_json::Error> for GraphError {
    fn from(e: serde_json::Error) -> Self {
        GraphError::Storage(format!("JSON error: {e}"))
    }
}
