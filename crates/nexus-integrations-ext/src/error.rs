#[derive(Debug, thiserror::Error)]
pub enum IntegrationError {
    #[error("Network error: {0}")]
    Network(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Rate limited: retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },
}

impl IntegrationError {
    pub fn from_reqwest(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            return Self::Network(format!("Request timed out: {err}"));
        }
        Self::Network(err.to_string())
    }

    pub async fn from_response(resp: reqwest::Response) -> Self {
        let status = resp.status().as_u16();

        if status == 429 {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(60);
            return Self::RateLimited {
                retry_after_secs: retry_after,
            };
        }

        let body = resp.text().await.unwrap_or_default();

        match status {
            401 | 403 => Self::Auth(body),
            404 => Self::NotFound(body),
            _ => Self::Api {
                status,
                message: body,
            },
        }
    }
}

impl serde::Serialize for IntegrationError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("IntegrationError", 2)?;
        s.serialize_field("error", &self.to_string())?;
        let kind = match self {
            Self::Network(_) => "network",
            Self::Parse(_) => "parse",
            Self::Auth(_) => "auth",
            Self::NotFound(_) => "not_found",
            Self::RateLimited { .. } => "rate_limited",
            Self::Config(_) => "config",
            Self::Api { .. } => "api",
        };
        s.serialize_field("kind", kind)?;
        s.end()
    }
}
