use thiserror::Error;

#[derive(Debug, Error)]
pub enum ForgeError {
    #[error("storage error: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("duplicate: {0}")]
    Duplicate(String),
    #[error("publish error: {0}")]
    Publish(String),
}

pub type ForgeResult<T> = Result<T, ForgeError>;
