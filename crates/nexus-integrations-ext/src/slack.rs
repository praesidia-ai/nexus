use serde::{Deserialize, Serialize};

use crate::error::IntegrationError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackConfig {
    pub webhook_url: String,
    pub bot_token: Option<String>,
    pub channel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackMessage {
    pub channel: String,
    pub text: String,
    pub blocks: Option<Vec<serde_json::Value>>,
    pub thread_ts: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackTrigger {
    pub command: String,
    pub text: String,
    pub user_id: String,
    pub channel_id: String,
    pub response_url: String,
}

pub struct SlackClient {
    config: SlackConfig,
    http: reqwest::Client,
}

impl SlackClient {
    pub fn new(config: SlackConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }

    pub async fn send_message(&self, text: &str) -> Result<(), IntegrationError> {
        self.http
            .post(&self.config.webhook_url)
            .json(&serde_json::json!({
                "channel": self.config.channel,
                "text": text,
            }))
            .send()
            .await
            .map_err(|e| IntegrationError::Network(e.to_string()))?;
        Ok(())
    }

    pub async fn send_rich_message(&self, msg: SlackMessage) -> Result<(), IntegrationError> {
        self.http
            .post(&self.config.webhook_url)
            .json(&msg)
            .send()
            .await
            .map_err(|e| IntegrationError::Network(e.to_string()))?;
        Ok(())
    }

    /// Format an agent task result as a Slack block message.
    pub fn format_task_result(task_name: &str, success: bool, details: &str) -> SlackMessage {
        let emoji = if success {
            ":white_check_mark:"
        } else {
            ":x:"
        };
        SlackMessage {
            channel: String::new(),
            text: format!("{emoji} Agent task: {task_name}"),
            blocks: Some(vec![serde_json::json!({
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": format!("{emoji} *{task_name}*\n{details}")
                }
            })]),
            thread_ts: None,
        }
    }
}
