use serde::{Deserialize, Serialize};

use crate::error::IntegrationError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordConfig {
    pub webhook_url: String,
    pub bot_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordEmbed {
    pub title: Option<String>,
    pub description: Option<String>,
    pub color: Option<u32>,
    pub fields: Vec<EmbedField>,
    pub footer: Option<EmbedFooter>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedField {
    pub name: String,
    pub value: String,
    pub inline: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedFooter {
    pub text: String,
}

pub struct DiscordClient {
    config: DiscordConfig,
    http: reqwest::Client,
}

impl DiscordClient {
    pub fn new(config: DiscordConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }

    pub async fn send_message(&self, content: &str) -> Result<(), IntegrationError> {
        self.http
            .post(&self.config.webhook_url)
            .json(&serde_json::json!({ "content": content }))
            .send()
            .await
            .map_err(|e| IntegrationError::Network(e.to_string()))?;
        Ok(())
    }

    pub async fn send_embed(&self, embed: DiscordEmbed) -> Result<(), IntegrationError> {
        self.http
            .post(&self.config.webhook_url)
            .json(&serde_json::json!({ "embeds": [embed] }))
            .send()
            .await
            .map_err(|e| IntegrationError::Network(e.to_string()))?;
        Ok(())
    }

    /// Build an embed for an agent task result notification.
    pub fn task_result_embed(task_name: &str, success: bool, details: &str) -> DiscordEmbed {
        let color = if success { 0x36A64F } else { 0xFF0000 };
        let status = if success { "Passed" } else { "Failed" };
        DiscordEmbed {
            title: Some(format!("Agent Task: {task_name}")),
            description: Some(details.to_string()),
            color: Some(color),
            fields: vec![EmbedField {
                name: "Status".to_string(),
                value: status.to_string(),
                inline: true,
            }],
            footer: Some(EmbedFooter {
                text: "Nexus Agent".to_string(),
            }),
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        }
    }
}
