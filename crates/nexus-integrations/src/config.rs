//! Integration configuration — serializable settings for all integrations.
//!
//! This module defines a top-level [`IntegrationConfig`] that aggregates
//! settings for GitHub, databases, and notification channels. It is designed
//! to be loaded from a JSON/YAML config file or environment-based construction.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Top-level configuration for all integrations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IntegrationConfig {
    /// GitHub integration settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<GitHubConfig>,

    /// Named database connections (key = alias, value = config).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub databases: HashMap<String, DatabaseConfig>,

    /// Notification channels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notifications: Vec<NotificationChannelConfig>,
}

/// GitHub integration configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubConfig {
    /// Personal access token (classic or fine-grained).
    pub token: String,

    /// Default GitHub organization for listing repos.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_org: Option<String>,
}

/// Configuration for a single database connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Database backend type (`postgresql`, `mysql`, `sqlite`, `supabase`).
    pub db_type: String,

    /// Connection string or descriptive reference.
    pub connection: String,

    /// Additional settings specific to the database type.
    /// For Supabase: `{ "project_url": "...", "anon_key": "..." }`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
}

/// Configuration for a single notification channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationChannelConfig {
    /// Channel type (`slack`, `discord`, `webhook`).
    pub channel_type: String,

    /// Channel-specific configuration as a JSON object.
    /// - Slack: `{ "webhook_url": "..." }`
    /// - Discord: `{ "webhook_url": "..." }`
    /// - Webhook: `{ "url": "...", "headers": { ... } }`
    pub config: serde_json::Value,
}

impl IntegrationConfig {
    /// Create a new empty integration config.
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a [`crate::github::GitHubIntegration`] from the stored config.
    ///
    /// Returns `None` if no GitHub config is set.
    pub fn build_github(&self) -> Option<crate::github::GitHubIntegration> {
        self.github
            .as_ref()
            .map(|cfg| crate::github::GitHubIntegration::new(&cfg.token))
    }

    /// Construct a [`crate::notifications::NotificationService`] from the stored channels.
    ///
    /// Channels with unrecognized types are logged and skipped.
    pub fn build_notification_service(&self) -> crate::notifications::NotificationService {
        use crate::notifications::{NotificationChannel, NotificationService};

        let mut svc = NotificationService::new();

        for ch_cfg in &self.notifications {
            let channel = match ch_cfg.channel_type.as_str() {
                "slack" => ch_cfg
                    .config
                    .get("webhook_url")
                    .and_then(|v| v.as_str())
                    .map(|url| NotificationChannel::Slack {
                        webhook_url: url.to_owned(),
                    }),
                "discord" => ch_cfg
                    .config
                    .get("webhook_url")
                    .and_then(|v| v.as_str())
                    .map(|url| NotificationChannel::Discord {
                        webhook_url: url.to_owned(),
                    }),
                "webhook" => {
                    let url = ch_cfg
                        .config
                        .get("url")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_owned());
                    let headers: HashMap<String, String> = ch_cfg
                        .config
                        .get("headers")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    url.map(|u| NotificationChannel::Webhook { url: u, headers })
                }
                other => {
                    tracing::warn!(channel_type = other, "Unknown notification channel type, skipping");
                    None
                }
            };

            if let Some(ch) = channel {
                svc.add_channel(ch);
            }
        }

        svc
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_serializes_to_minimal_json() {
        let cfg = IntegrationConfig::new();
        let json = serde_json::to_value(&cfg).unwrap();
        // Empty fields are skipped.
        assert!(json.as_object().unwrap().is_empty() || json == serde_json::json!({}));
    }

    #[test]
    fn full_config_round_trips() {
        let cfg = IntegrationConfig {
            github: Some(GitHubConfig {
                token: "ghp_abc123".into(),
                default_org: Some("my-org".into()),
            }),
            databases: {
                let mut m = HashMap::new();
                m.insert(
                    "primary".into(),
                    DatabaseConfig {
                        db_type: "supabase".into(),
                        connection: "supabase://xyz".into(),
                        extra: Some(serde_json::json!({
                            "project_url": "https://xyz.supabase.co",
                            "anon_key": "key"
                        })),
                    },
                );
                m
            },
            notifications: vec![NotificationChannelConfig {
                channel_type: "slack".into(),
                config: serde_json::json!({ "webhook_url": "https://hooks.slack.com/test" }),
            }],
        };

        let json = serde_json::to_string(&cfg).unwrap();
        let cfg2: IntegrationConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg2.github.unwrap().token, "ghp_abc123");
        assert_eq!(cfg2.databases.len(), 1);
        assert_eq!(cfg2.notifications.len(), 1);
    }

    #[test]
    fn build_github_returns_none_without_config() {
        let cfg = IntegrationConfig::new();
        assert!(cfg.build_github().is_none());
    }

    #[test]
    fn build_github_returns_some_with_config() {
        let cfg = IntegrationConfig {
            github: Some(GitHubConfig {
                token: "ghp_test".into(),
                default_org: None,
            }),
            ..Default::default()
        };
        assert!(cfg.build_github().is_some());
    }

    #[test]
    fn build_notification_service_parses_channels() {
        let cfg = IntegrationConfig {
            notifications: vec![
                NotificationChannelConfig {
                    channel_type: "slack".into(),
                    config: serde_json::json!({ "webhook_url": "https://hooks.slack.com/x" }),
                },
                NotificationChannelConfig {
                    channel_type: "discord".into(),
                    config: serde_json::json!({ "webhook_url": "https://discord.com/api/webhooks/x" }),
                },
                NotificationChannelConfig {
                    channel_type: "webhook".into(),
                    config: serde_json::json!({ "url": "https://example.com/hook", "headers": { "X-Key": "val" } }),
                },
                NotificationChannelConfig {
                    channel_type: "unknown".into(),
                    config: serde_json::json!({}),
                },
            ],
            ..Default::default()
        };

        let svc = cfg.build_notification_service();
        // 3 valid channels (unknown is skipped).
        assert_eq!(svc.channel_count(), 3);
    }
}
