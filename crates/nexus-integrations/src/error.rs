//! Unified error type for all integrations.

/// Errors that can occur when interacting with external integrations.
#[derive(Debug, thiserror::Error)]
pub enum IntegrationError {
    /// An HTTP API call failed (non-success status or network error).
    #[error("API error: {message} (status: {status})")]
    Api {
        /// HTTP status code (0 if the request never reached the server).
        status: u16,
        /// Human-readable error message.
        message: String,
    },

    /// Authentication or authorization failure (401/403).
    #[error("Authentication failed: {0}")]
    Auth(String),

    /// The requested resource was not found (404).
    #[error("Not found: {0}")]
    NotFound(String),

    /// Invalid or missing configuration.
    #[error("Configuration error: {0}")]
    Config(String),

    /// The request timed out before completing.
    #[error("Request timed out: {0}")]
    Timeout(String),

    /// Rate limit exceeded — includes the retry-after duration if available.
    #[error("Rate limited: retry after {retry_after_secs}s")]
    RateLimited {
        /// Seconds until the rate limit resets.
        retry_after_secs: u64,
    },
}

impl IntegrationError {
    /// Build an [`IntegrationError`] from a [`reqwest::Response`] with a non-success status.
    pub async fn from_response(resp: reqwest::Response) -> Self {
        let status = resp.status().as_u16();

        // Check for rate limiting first.
        if status == 429 || status == 403 {
            if let Some(remaining) = resp.headers().get("x-ratelimit-remaining") {
                if remaining.to_str().unwrap_or("1") == "0" {
                    let reset = resp
                        .headers()
                        .get("x-ratelimit-reset")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok())
                        .map(|epoch| {
                            epoch.saturating_sub(
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                            )
                        })
                        .unwrap_or(60);
                    return Self::RateLimited {
                        retry_after_secs: reset,
                    };
                }
            }
        }

        let body = resp.text().await.unwrap_or_default();

        match status {
            401 | 403 => Self::Auth(body),
            404 => Self::NotFound(body),
            408 => Self::Timeout(body),
            _ => Self::Api {
                status,
                message: body,
            },
        }
    }

    /// Build an [`IntegrationError`] from a [`reqwest::Error`] (network-level failure).
    pub fn from_reqwest(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            return Self::Timeout(err.to_string());
        }
        Self::Api {
            status: err.status().map(|s| s.as_u16()).unwrap_or(0),
            message: err.to_string(),
        }
    }
}

impl serde::Serialize for IntegrationError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("IntegrationError", 2)?;
        s.serialize_field("error", &self.to_string())?;
        let kind = match self {
            Self::Api { .. } => "api",
            Self::Auth(_) => "auth",
            Self::NotFound(_) => "not_found",
            Self::Config(_) => "config",
            Self::Timeout(_) => "timeout",
            Self::RateLimited { .. } => "rate_limited",
        };
        s.serialize_field("kind", kind)?;
        s.end()
    }
}
