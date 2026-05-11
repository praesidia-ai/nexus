//! Nexus-specific A2A task handler — bridges A2A tasks to ZeroClaw/NexusAgent.

use tracing::info;

use crate::error::A2aError;
use crate::server::TaskHandler;
use crate::types::{Artifact, Message, Part};

/// Minimal task handler that routes A2A tasks to an LLM via text extraction.
/// In production, plug in the real ZeroClaw agent pool here.
pub struct NexusTaskHandler {
    /// OpenAI API key for fallback text generation.
    pub openai_api_key: String,
    /// Model to use.
    pub model: String,
}

impl NexusTaskHandler {
    pub fn new(openai_api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            openai_api_key: openai_api_key.into(),
            model: model.into(),
        }
    }

    fn extract_text(message: &Message) -> String {
        message
            .parts
            .iter()
            .filter_map(|p| {
                if let Part::Text { text } = p {
                    Some(text.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[async_trait::async_trait]
impl TaskHandler for NexusTaskHandler {
    async fn handle(&self, task_id: &str, message: &Message) -> Result<Vec<Artifact>, A2aError> {
        let text = Self::extract_text(message);
        info!(task_id, input_len = text.len(), "NexusTaskHandler processing A2A task");

        // Call OpenAI with the task text
        let client = reqwest::Client::new();
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {
                    "role": "system",
                    "content": "You are Nexus, an AI agent OS. Answer the user's request concisely and helpfully."
                },
                {
                    "role": "user",
                    "content": text,
                }
            ],
        });

        let resp = client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&self.openai_api_key)
            .json(&body)
            .send()
            .await
            .map_err(A2aError::Network)?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(A2aError::Internal(format!("LLM error {status}: {body}")));
        }

        let resp_json: serde_json::Value = resp.json().await.map_err(A2aError::Network)?;
        let reply = resp_json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("(no response)")
            .to_string();

        let artifact = Artifact {
            name: "response".into(),
            description: Some("Agent reply".into()),
            parts: vec![Part::Text { text: reply }],
            index: Some(0),
            append: false,
            last_chunk: true,
            metadata: Default::default(),
        };

        Ok(vec![artifact])
    }
}
