//! `web.fetcher` — fetch one URL in sandbox, sanitise, return.
//!
//! Input schema:
//! ```json
//! {"url": "https://example.com/docs/page"}
//! ```
//!
//! Output schema:
//! ```json
//! {
//!   "url": "https://example.com/docs/page",
//!   "status": 200,
//!   "content_type": "text/html",
//!   "body": "…sanitised, ≤ 8 KB…",
//!   "truncated": false
//! }
//! ```
//!
//! Security:
//! - Runs through `security::url_guard::is_public_url` — rejects
//!   loopback / RFC1918 / link-local / metadata-IP / cloud-metadata
//!   hostnames (the SSRF allowlist added in the round-2 security
//!   pass).
//! - 5 MB hard cap on response body so a bomb URL can't starve the
//!   conductor.
//! - 10 s wall-clock timeout per fetch.

use async_trait::async_trait;
use nexus_agents_core::mini::{MiniAgent, MiniError, MiniKind, MiniOutput, Task};

use crate::security::url_guard;

pub struct WebFetcher {
    client: reqwest::Client,
}

impl WebFetcher {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .user_agent(concat!("nexus/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client }
    }
}

impl Default for WebFetcher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MiniAgent for WebFetcher {
    fn kind(&self) -> MiniKind {
        MiniKind::WebFetcher
    }

    async fn run(&self, task: Task) -> Result<MiniOutput, MiniError> {
        let started = std::time::Instant::now();
        let url = task
            .input
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| MiniError::BadInput {
                kind: MiniKind::WebFetcher,
                reason: "missing `url`".into(),
            })?;

        url_guard::is_public_url(url).map_err(|e| MiniError::BadInput {
            kind: MiniKind::WebFetcher,
            reason: format!("url rejected by SSRF guard: {e}"),
        })?;

        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| MiniError::Provider(format!("fetch {url}: {e}")))?;
        let status = resp.status().as_u16();
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        const MAX_BYTES: usize = 5 * 1024 * 1024;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| MiniError::Provider(format!("body read: {e}")))?;
        if bytes.len() > MAX_BYTES {
            return Err(MiniError::BudgetExceeded {
                dimension: "web_body_too_large",
            });
        }

        let body_str = String::from_utf8_lossy(&bytes).to_string();

        // Cheap sanitisation: strip zero-width + bidi override chars to
        // reduce prompt-injection via invisible-unicode smuggling, per
        // the Spotlighting design in the master plan.
        let sanitised: String = body_str
            .chars()
            .filter(|c| !matches!(*c as u32, 0x200B..=0x200F | 0x202A..=0x202E | 0x2066..=0x2069))
            .collect();

        const OUTPUT_CAP: usize = 8 * 1024;
        let (body_out, truncated) = if sanitised.chars().count() > OUTPUT_CAP {
            (sanitised.chars().take(OUTPUT_CAP).collect::<String>(), true)
        } else {
            (sanitised, false)
        };

        Ok(MiniOutput {
            task_id: task.id,
            kind: MiniKind::WebFetcher,
            output: serde_json::json!({
                "url": url,
                "status": status,
                "content_type": content_type,
                "body": body_out,
                "truncated": truncated,
            }),
            tokens_used: 0,
            duration: started.elapsed(),
            cost_usd: 0.0,
            needs_review: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_agents_core::mini::Budget;

    fn task(url: &str) -> Task {
        Task {
            id: "t".into(),
            kind: MiniKind::WebFetcher,
            input: serde_json::json!({"url": url}),
            budget: Budget::default(),
            parent_id: None,
        }
    }

    #[tokio::test]
    async fn rejects_loopback() {
        let f = WebFetcher::new();
        let err = f.run(task("http://127.0.0.1:8020/")).await.unwrap_err();
        assert!(matches!(err, MiniError::BadInput { .. }));
    }

    #[tokio::test]
    async fn rejects_non_http_scheme() {
        let f = WebFetcher::new();
        let err = f.run(task("file:///etc/passwd")).await.unwrap_err();
        assert!(matches!(err, MiniError::BadInput { .. }));
    }

    #[tokio::test]
    async fn rejects_metadata_ip() {
        let f = WebFetcher::new();
        let err = f
            .run(task("http://169.254.169.254/latest/meta-data/"))
            .await
            .unwrap_err();
        assert!(matches!(err, MiniError::BadInput { .. }));
    }
}
