//! Notification service — send alerts to Slack, Discord, and generic webhooks.
//!
//! Supports multiple channels simultaneously. When [`NotificationService::send`]
//! is called, it fans out to all registered channels concurrently and returns
//! per-channel results.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::IntegrationError;

// ─── Public data types ──────────────────────────────────────────────────────

/// A notification channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotificationChannel {
    /// Slack incoming webhook.
    Slack {
        /// The full webhook URL (`https://hooks.slack.com/services/...`).
        webhook_url: String,
    },
    /// Discord incoming webhook.
    Discord {
        /// The full webhook URL (`https://discord.com/api/webhooks/...`).
        webhook_url: String,
    },
    /// Generic HTTP webhook — sends JSON POST with custom headers.
    Webhook {
        /// Destination URL.
        url: String,
        /// Additional headers to include (e.g. `Authorization`).
        #[serde(default)]
        headers: HashMap<String, String>,
    },
}

/// A notification to be dispatched.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    /// Short title / subject line.
    pub title: String,
    /// Full message body (Markdown is fine for Slack/Discord).
    pub body: String,
    /// Severity level — controls color/emoji in supported channels.
    pub severity: NotifSeverity,
}

/// Severity level for a notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotifSeverity {
    Info,
    Warning,
    Error,
    Success,
}

impl NotifSeverity {
    /// Slack-compatible color hex code.
    fn slack_color(self) -> &'static str {
        match self {
            Self::Info => "#2196F3",
            Self::Warning => "#FF9800",
            Self::Error => "#F44336",
            Self::Success => "#4CAF50",
        }
    }

    /// Discord embed color as an integer.
    fn discord_color(self) -> u32 {
        match self {
            Self::Info => 0x2196F3,
            Self::Warning => 0xFF9800,
            Self::Error => 0xF44336,
            Self::Success => 0x4CAF50,
        }
    }

    /// Emoji prefix.
    fn emoji(self) -> &'static str {
        match self {
            Self::Info => "ℹ️",
            Self::Warning => "⚠️",
            Self::Error => "🚨",
            Self::Success => "✅",
        }
    }
}

// ─── Service ────────────────────────────────────────────────────────────────

/// Fan-out notification dispatcher.
///
/// Register one or more [`NotificationChannel`]s, then call [`send`](Self::send)
/// to dispatch a notification to all of them concurrently.
pub struct NotificationService {
    channels: Vec<NotificationChannel>,
    client: reqwest::Client,
}

impl NotificationService {
    /// Create a new empty notification service.
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("failed to build reqwest client");

        Self {
            channels: Vec::new(),
            client,
        }
    }

    /// Add a notification channel.
    pub fn add_channel(&mut self, channel: NotificationChannel) {
        self.channels.push(channel);
    }

    /// Return the number of registered channels.
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// Send a notification to all registered channels concurrently.
    ///
    /// Returns a `Vec` of `(channel_index, Result)` — one entry per channel,
    /// in channel order. Failures in one channel do not prevent delivery to others.
    pub async fn send(
        &self,
        notification: &Notification,
    ) -> Vec<(usize, Result<(), IntegrationError>)> {
        let futures: Vec<_> = self
            .channels
            .iter()
            .enumerate()
            .map(|(idx, channel)| {
                let client = self.client.clone();
                let notif = notification.clone();
                let ch = channel.clone();
                async move { (idx, Self::send_to_channel(&client, &ch, &notif).await) }
            })
            .collect();

        futures_unordered(futures).await
    }

    /// Send a single notification to a single channel.
    async fn send_to_channel(
        client: &reqwest::Client,
        channel: &NotificationChannel,
        notification: &Notification,
    ) -> Result<(), IntegrationError> {
        match channel {
            NotificationChannel::Slack { webhook_url } => {
                Self::send_slack(client, webhook_url, notification).await
            }
            NotificationChannel::Discord { webhook_url } => {
                Self::send_discord(client, webhook_url, notification).await
            }
            NotificationChannel::Webhook { url, headers } => {
                Self::send_webhook(client, url, headers, notification).await
            }
        }
    }

    /// Send via Slack incoming webhook using the attachments format.
    async fn send_slack(
        client: &reqwest::Client,
        webhook_url: &str,
        notification: &Notification,
    ) -> Result<(), IntegrationError> {
        debug!("Sending Slack notification");

        let payload = serde_json::json!({
            "text": format!("{} *{}*", notification.severity.emoji(), notification.title),
            "attachments": [{
                "color": notification.severity.slack_color(),
                "text": notification.body,
                "fallback": format!("{}: {}", notification.title, notification.body),
            }]
        });

        let resp = client
            .post(webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(IntegrationError::from_reqwest)?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            warn!(status, body = %body, "Slack webhook failed");
            return Err(IntegrationError::Api {
                status,
                message: format!("Slack webhook returned {status}: {body}"),
            });
        }

        Ok(())
    }

    /// Send via Discord webhook using the embeds format.
    async fn send_discord(
        client: &reqwest::Client,
        webhook_url: &str,
        notification: &Notification,
    ) -> Result<(), IntegrationError> {
        debug!("Sending Discord notification");

        let payload = serde_json::json!({
            "content": format!("{} **{}**", notification.severity.emoji(), notification.title),
            "embeds": [{
                "title": notification.title,
                "description": notification.body,
                "color": notification.severity.discord_color(),
            }]
        });

        let resp = client
            .post(webhook_url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(IntegrationError::from_reqwest)?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            warn!(status, body = %body, "Discord webhook failed");
            return Err(IntegrationError::Api {
                status,
                message: format!("Discord webhook returned {status}: {body}"),
            });
        }

        Ok(())
    }

    /// Send via a generic HTTP webhook — POSTs the notification as JSON.
    async fn send_webhook(
        client: &reqwest::Client,
        url: &str,
        headers: &HashMap<String, String>,
        notification: &Notification,
    ) -> Result<(), IntegrationError> {
        debug!(url, "Sending webhook notification");

        let payload = serde_json::json!({
            "title": notification.title,
            "body": notification.body,
            "severity": notification.severity,
        });

        let mut req = client.post(url).json(&payload);
        for (k, v) in headers {
            req = req.header(k.as_str(), v.as_str());
        }

        let resp = req.send().await.map_err(IntegrationError::from_reqwest)?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            warn!(status, url, body = %body, "Webhook delivery failed");
            return Err(IntegrationError::Api {
                status,
                message: format!("Webhook returned {status}: {body}"),
            });
        }

        Ok(())
    }
}

impl Default for NotificationService {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Run a collection of futures concurrently and collect results in order.
async fn futures_unordered<T, F>(futures: Vec<F>) -> Vec<T>
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let handles: Vec<_> = futures
        .into_iter()
        .map(|f| tokio::spawn(f))
        .collect();

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        match handle.await {
            Ok(val) => results.push(val),
            Err(_) => {
                // JoinError — task panicked. This shouldn't happen in normal operation.
            }
        }
    }
    results
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slack_payload_format() {
        let notif = Notification {
            title: "Deploy complete".into(),
            body: "Version 1.2.3 deployed to production.".into(),
            severity: NotifSeverity::Success,
        };
        let payload = serde_json::json!({
            "text": format!("{} *{}*", notif.severity.emoji(), notif.title),
            "attachments": [{
                "color": notif.severity.slack_color(),
                "text": notif.body,
                "fallback": format!("{}: {}", notif.title, notif.body),
            }]
        });
        assert_eq!(payload["attachments"][0]["color"], "#4CAF50");
        assert!(payload["text"].as_str().unwrap().contains("Deploy complete"));
    }

    #[test]
    fn discord_payload_format() {
        let notif = Notification {
            title: "Build failed".into(),
            body: "Tests did not pass.".into(),
            severity: NotifSeverity::Error,
        };
        let payload = serde_json::json!({
            "embeds": [{
                "title": notif.title,
                "description": notif.body,
                "color": notif.severity.discord_color(),
            }]
        });
        assert_eq!(payload["embeds"][0]["color"], 0xF44336_u32);
    }

    #[test]
    fn webhook_payload_format() {
        let notif = Notification {
            title: "Alert".into(),
            body: "Something happened.".into(),
            severity: NotifSeverity::Warning,
        };
        let payload = serde_json::json!({
            "title": notif.title,
            "body": notif.body,
            "severity": notif.severity,
        });
        assert_eq!(payload["severity"], "warning");
        assert_eq!(payload["title"], "Alert");
    }

    #[test]
    fn notification_channel_serializes_with_tag() {
        let slack = NotificationChannel::Slack {
            webhook_url: "https://hooks.slack.com/test".into(),
        };
        let json = serde_json::to_value(&slack).unwrap();
        assert_eq!(json["type"], "slack");
        assert_eq!(json["webhook_url"], "https://hooks.slack.com/test");
    }

    #[test]
    fn notification_service_starts_empty() {
        let svc = NotificationService::new();
        assert_eq!(svc.channel_count(), 0);
    }

    #[test]
    fn add_channels_increments_count() {
        let mut svc = NotificationService::new();
        svc.add_channel(NotificationChannel::Slack {
            webhook_url: "https://hooks.slack.com/test".into(),
        });
        svc.add_channel(NotificationChannel::Discord {
            webhook_url: "https://discord.com/api/webhooks/test".into(),
        });
        assert_eq!(svc.channel_count(), 2);
    }

    #[test]
    fn severity_colors() {
        assert_eq!(NotifSeverity::Info.slack_color(), "#2196F3");
        assert_eq!(NotifSeverity::Error.discord_color(), 0xF44336);
    }

    #[test]
    fn notification_round_trips() {
        let notif = Notification {
            title: "Test".into(),
            body: "Body text".into(),
            severity: NotifSeverity::Info,
        };
        let json = serde_json::to_string(&notif).unwrap();
        let notif2: Notification = serde_json::from_str(&json).unwrap();
        assert_eq!(notif2.title, "Test");
        assert_eq!(notif2.severity, NotifSeverity::Info);
    }
}
