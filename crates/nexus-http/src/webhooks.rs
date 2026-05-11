//! Webhook system — notify external systems of Nexus events.
//!
//! Supports registration of webhook endpoints, HMAC-SHA256 payload signing,
//! and async event dispatch for all major Nexus lifecycle events.

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tracing::{info, warn};

/// Configuration for a single webhook endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// Unique identifier for this webhook registration.
    pub id: String,
    /// URL to POST event payloads to.
    pub url: String,
    /// Events this webhook subscribes to.
    pub events: Vec<WebhookEvent>,
    /// Shared secret for HMAC-SHA256 payload signing.
    pub secret: String,
    /// Whether this webhook is currently active.
    pub active: bool,
}

/// Events that can trigger webhook notifications.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEvent {
    /// A full app generation completed (success or failure).
    GenerationCompleted,
    /// A single agent finished its task.
    AgentCompleted,
    /// A multi-agent team finished its run.
    TeamCompleted,
    /// A quality/taste score was computed for a project.
    QualityScored,
    /// A step in a workflow pipeline completed.
    WorkflowStepCompleted,
    /// LLM spending exceeded the configured budget.
    BudgetExceeded,
}

impl std::fmt::Display for WebhookEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| format!("{:?}", self));
        write!(f, "{}", s)
    }
}

/// Service that manages webhook registrations and fires events.
///
/// Outbound delivery uses a per-call pinned `reqwest::Client` built by
/// `url_guard::pinned_client` so each fetch is guaranteed to hit a
/// pre-validated IP — there is no shared client to share across deliveries.
pub struct WebhookService {
    /// Registered webhook configurations.
    webhooks: Vec<WebhookConfig>,
}

impl WebhookService {
    /// Create a new empty webhook service.
    pub fn new() -> Self {
        Self {
            webhooks: Vec::new(),
        }
    }

    /// Register a new webhook endpoint.
    ///
    /// If a webhook with the same ID already exists it will be replaced.
    pub fn register(&mut self, config: WebhookConfig) {
        self.webhooks.retain(|w| w.id != config.id);
        info!(id = %config.id, url = %config.url, "Webhook registered");
        self.webhooks.push(config);
    }

    /// Remove a webhook by its ID.
    ///
    /// Returns `true` if a webhook was removed.
    pub fn unregister(&mut self, id: &str) -> bool {
        let before = self.webhooks.len();
        self.webhooks.retain(|w| w.id != id);
        let removed = self.webhooks.len() < before;
        if removed {
            info!(id = %id, "Webhook unregistered");
        }
        removed
    }

    /// List all registered webhooks.
    pub fn list(&self) -> &[WebhookConfig] {
        &self.webhooks
    }

    /// Fire an event, sending the payload to all active webhooks subscribed to it.
    ///
    /// Delivery is best-effort: failures are logged but do not propagate.
    ///
    /// SECURITY: each delivery resolves the webhook URL through
    /// `url_guard::resolve_and_validate_async` and pins the actual fetch to
    /// the validated IP. This defeats DNS rebinding — an attacker cannot
    /// register `evil.com` (resolving public) and then flip DNS to
    /// 169.254.169.254 between registration and delivery. If the resolve
    /// returns ANY blocked IP we skip that webhook and log a warning.
    ///
    /// Each delivery carries a fresh `X-Nexus-Timestamp` (RFC-3339) and
    /// `X-Nexus-Nonce` (UUID) so receivers can reject replays by refusing
    /// signatures with a reused nonce or a timestamp outside a short window.
    pub async fn fire(&self, event: WebhookEvent, payload: serde_json::Value) {
        let payload_bytes = serde_json::to_vec(&payload).unwrap_or_default();

        for webhook in &self.webhooks {
            if !webhook.active || !webhook.events.contains(&event) {
                continue;
            }

            // Per-delivery DNS resolve + IP allowlist + pinned client.
            let (host, _port, addrs) =
                match crate::security::url_guard::resolve_and_validate_async(&webhook.url).await {
                    Ok(parts) => parts,
                    Err(reason) => {
                        warn!(
                            webhook_id = %webhook.id,
                            event = %event,
                            reason = %reason,
                            "Webhook delivery skipped: URL failed SSRF/DNS validation",
                        );
                        continue;
                    }
                };
            let pin_timeout = std::time::Duration::from_secs(10);
            let pinned = match crate::security::url_guard::pinned_client(&host, &addrs, pin_timeout)
            {
                Ok(c) => c,
                Err(e) => {
                    warn!(
                        webhook_id = %webhook.id,
                        event = %event,
                        error = %e,
                        "Webhook delivery skipped: pinned client build failed",
                    );
                    continue;
                }
            };

            let timestamp = chrono::Utc::now().to_rfc3339();
            let nonce = uuid::Uuid::new_v4().to_string();
            // Sign "<timestamp>.<nonce>.<body>" so replaying the body alone
            // with a different timestamp does not match the original MAC.
            let mut signing =
                Vec::with_capacity(timestamp.len() + 1 + nonce.len() + 1 + payload_bytes.len());
            signing.extend_from_slice(timestamp.as_bytes());
            signing.push(b'.');
            signing.extend_from_slice(nonce.as_bytes());
            signing.push(b'.');
            signing.extend_from_slice(&payload_bytes);
            let signature = Self::sign_payload(&webhook.secret, &signing);

            let result = pinned
                .post(&webhook.url)
                .header("Content-Type", "application/json")
                .header("X-Nexus-Event", event.to_string())
                .header("X-Nexus-Signature", &signature)
                .header("X-Nexus-Timestamp", &timestamp)
                .header("X-Nexus-Nonce", &nonce)
                .body(payload_bytes.clone())
                .send()
                .await;

            match result {
                Ok(resp) if resp.status().is_success() => {
                    info!(
                        webhook_id = %webhook.id,
                        event = %event,
                        "Webhook delivered successfully"
                    );
                }
                Ok(resp) => {
                    warn!(
                        webhook_id = %webhook.id,
                        event = %event,
                        status = %resp.status(),
                        "Webhook delivery returned non-success status"
                    );
                }
                Err(err) => {
                    warn!(
                        webhook_id = %webhook.id,
                        event = %event,
                        error = %err,
                        "Webhook delivery failed"
                    );
                }
            }
        }
    }

    /// Compute HMAC-SHA256 signature of a payload using the given secret.
    ///
    /// Returns the hex-encoded signature string.
    fn sign_payload(secret: &str, payload: &[u8]) -> String {
        type HmacSha256 = Hmac<Sha256>;
        let mut mac =
            HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
        mac.update(payload);
        hex::encode(mac.finalize().into_bytes())
    }
}

impl Default for WebhookService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_list() {
        let mut svc = WebhookService::new();
        assert!(svc.list().is_empty());

        svc.register(WebhookConfig {
            id: "wh-1".into(),
            url: "https://example.com/hook".into(),
            events: vec![WebhookEvent::GenerationCompleted],
            secret: "s3cr3t".into(),
            active: true,
        });
        assert_eq!(svc.list().len(), 1);
        assert_eq!(svc.list()[0].id, "wh-1");
    }

    #[test]
    fn unregister_removes_webhook() {
        let mut svc = WebhookService::new();
        svc.register(WebhookConfig {
            id: "wh-1".into(),
            url: "https://example.com/hook".into(),
            events: vec![WebhookEvent::AgentCompleted],
            secret: "key".into(),
            active: true,
        });
        assert!(svc.unregister("wh-1"));
        assert!(svc.list().is_empty());
        assert!(!svc.unregister("wh-1"));
    }

    #[test]
    fn register_replaces_existing() {
        let mut svc = WebhookService::new();
        svc.register(WebhookConfig {
            id: "wh-1".into(),
            url: "https://old.com".into(),
            events: vec![],
            secret: "k".into(),
            active: true,
        });
        svc.register(WebhookConfig {
            id: "wh-1".into(),
            url: "https://new.com".into(),
            events: vec![],
            secret: "k".into(),
            active: true,
        });
        assert_eq!(svc.list().len(), 1);
        assert_eq!(svc.list()[0].url, "https://new.com");
    }

    #[test]
    fn sign_payload_deterministic() {
        let sig1 = WebhookService::sign_payload("secret", b"hello");
        let sig2 = WebhookService::sign_payload("secret", b"hello");
        assert_eq!(sig1, sig2);
        assert!(!sig1.is_empty());
    }

    #[test]
    fn sign_payload_different_secrets() {
        let sig1 = WebhookService::sign_payload("secret1", b"hello");
        let sig2 = WebhookService::sign_payload("secret2", b"hello");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn webhook_event_display() {
        let event = WebhookEvent::GenerationCompleted;
        let display = event.to_string();
        assert_eq!(display, "generation_completed");
    }
}
