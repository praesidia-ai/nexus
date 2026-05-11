//! Unified API error type that implements `IntoResponse`.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("too many requests: {0}")]
    TooManyRequests(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, msg, hint) = match &self {
            ApiError::NotFound(m) => (
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                m.clone(),
                "The item you're looking for doesn't exist or may have been removed.".to_string(),
            ),
            ApiError::BadRequest(m) => (
                StatusCode::BAD_REQUEST,
                "BAD_REQUEST",
                humanize_bad_request(m),
                "Try checking your input and trying again.".to_string(),
            ),
            ApiError::Unauthorized(m) => (
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                m.clone(),
                "Please sign in to continue.".to_string(),
            ),
            ApiError::Forbidden(m) => (
                StatusCode::FORBIDDEN,
                "FORBIDDEN",
                m.clone(),
                "You don't have permission for this action. Contact your admin if you think this is a mistake.".to_string(),
            ),
            ApiError::TooManyRequests(m) => (
                StatusCode::TOO_MANY_REQUESTS,
                "TOO_MANY_REQUESTS",
                m.clone(),
                "You're making requests too quickly. Please wait a moment and try again.".to_string(),
            ),
            ApiError::Conflict(m) => (
                StatusCode::CONFLICT,
                "CONFLICT",
                m.clone(),
                "The request conflicts with the current state. Try again in a moment.".to_string(),
            ),
            ApiError::Internal(m) => {
                tracing::error!(error = %m, "Internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL",
                    "Something went wrong on our end.".to_string(),
                    "This is usually temporary. Try refreshing the page. If it keeps happening, check that the server is running.".to_string(),
                )
            }
        };
        (status, Json(json!({
            "error": {
                "code": code,
                "message": msg,
                "hint": hint,
            }
        }))).into_response()
    }
}

/// Make bad-request errors more human-readable.
fn humanize_bad_request(msg: &str) -> String {
    if msg.starts_with("JSON error:") {
        "The request wasn't formatted correctly. Please try again.".to_string()
    } else if msg.contains("required") {
        msg.replace("required", "is needed")
    } else {
        msg.to_string()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError::Internal(e.to_string())
    }
}

impl From<nexus_store::StoreError> for ApiError {
    fn from(e: nexus_store::StoreError) -> Self {
        ApiError::Internal(e.to_string())
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        ApiError::Internal(e.to_string())
    }
}

impl From<nexus_memory::MemoryError> for ApiError {
    fn from(e: nexus_memory::MemoryError) -> Self {
        match e {
            nexus_memory::MemoryError::NotFound(m) => ApiError::NotFound(m),
            nexus_memory::MemoryError::Storage(m) => ApiError::Internal(m),
            nexus_memory::MemoryError::Embedding(m) => ApiError::BadRequest(m),
        }
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        ApiError::BadRequest(format!("JSON error: {e}"))
    }
}

impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        ApiError::Internal(e.to_string())
    }
}

pub type ApiResult<T> = std::result::Result<T, ApiError>;
