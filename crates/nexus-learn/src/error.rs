//! Error types for the self-improvement engine.

#[derive(Debug, thiserror::Error)]
pub enum LearnError {
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Pattern extraction failed: {0}")]
    PatternExtraction(String),

    #[error("Evaluation failed: {0}")]
    Evaluation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl From<rusqlite::Error> for LearnError {
    fn from(e: rusqlite::Error) -> Self {
        LearnError::Storage(e.to_string())
    }
}
