//! SDK error types for the Nexus client.

/// All errors that can occur when interacting with the Nexus API.
#[derive(Debug, thiserror::Error)]
pub enum SdkError {
    /// An HTTP transport-level error (network, DNS, TLS, etc.).
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// The Nexus API returned a non-2xx status code.
    #[error("API error (status {status}): {message}")]
    Api {
        /// HTTP status code returned by the server.
        status: u16,
        /// Error message from the response body.
        message: String,
    },

    /// A request timed out before the server responded.
    #[error("Request timed out after {0}ms")]
    Timeout(u64),

    /// Authentication failed (invalid or missing API key).
    #[error("Authentication failed: {0}")]
    Auth(String),

    /// Failed to parse the response body as the expected type.
    #[error("Failed to parse response: {0}")]
    Parse(String),
}
