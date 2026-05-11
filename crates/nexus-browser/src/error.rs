#[derive(Debug, thiserror::Error)]
pub enum BrowserError {
    #[error("Session error: {0}")]
    Session(String),

    #[error("Navigation failed: {url} — {reason}")]
    Navigation { url: String, reason: String },

    #[error("Selector not found: {selector} (timeout {timeout_ms}ms)")]
    SelectorNotFound { selector: String, timeout_ms: u64 },

    #[error("JavaScript evaluation failed: {0}")]
    ScriptError(String),

    #[error("Screenshot failed: {0}")]
    Screenshot(String),

    #[error("Connection to browser lost: {0}")]
    ConnectionLost(String),

    #[error("Timeout after {ms}ms: {operation}")]
    Timeout { operation: String, ms: u64 },

    #[error("Driver not available: {0}")]
    DriverUnavailable(String),
}

impl serde::Serialize for BrowserError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("BrowserError", 2)?;
        s.serialize_field("error", &self.to_string())?;
        let kind = match self {
            Self::Session(_) => "session",
            Self::Navigation { .. } => "navigation",
            Self::SelectorNotFound { .. } => "selector_not_found",
            Self::ScriptError(_) => "script_error",
            Self::Screenshot(_) => "screenshot",
            Self::ConnectionLost(_) => "connection_lost",
            Self::Timeout { .. } => "timeout",
            Self::DriverUnavailable(_) => "driver_unavailable",
        };
        s.serialize_field("kind", kind)?;
        s.end()
    }
}
