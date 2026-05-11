//! A2A client — send tasks to remote A2A agents.

use serde_json::json;
use tracing::instrument;

use crate::error::A2aError;
use crate::types::{Message, Task, TaskCancelParams, TaskGetParams, TaskSendParams};

/// Client for calling a remote A2A agent endpoint.
#[derive(Clone)]
pub struct A2aClient {
    base_url: String,
    http: reqwest::Client,
}

impl A2aClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    fn next_id() -> serde_json::Value {
        json!(uuid::Uuid::new_v4().to_string())
    }

    // -----------------------------------------------------------------------
    // tasks/send — blocking (returns final task state)
    // -----------------------------------------------------------------------

    /// Send a message to the remote agent and wait for the task to complete.
    #[instrument(skip(self, params))]
    pub async fn send(&self, params: TaskSendParams) -> Result<Task, A2aError> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": Self::next_id(),
            "method": "tasks/send",
            "params": params,
        });

        let resp: serde_json::Value = self
            .http
            .post(&self.base_url)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        if let Some(err) = resp.get("error") {
            return Err(A2aError::Internal(err.to_string()));
        }

        let task: Task = serde_json::from_value(
            resp.get("result")
                .cloned()
                .ok_or_else(|| A2aError::InvalidAgentResponse("no result field".into()))?,
        )?;

        Ok(task)
    }

    // -----------------------------------------------------------------------
    // tasks/get — query status of an existing task
    // -----------------------------------------------------------------------

    pub async fn get(&self, task_id: &str) -> Result<Task, A2aError> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": Self::next_id(),
            "method": "tasks/get",
            "params": TaskGetParams {
                id: task_id.to_string(),
                history_length: None,
            },
        });

        let resp: serde_json::Value = self
            .http
            .post(&self.base_url)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        if let Some(err) = resp.get("error") {
            return Err(A2aError::Internal(err.to_string()));
        }

        let task: Task = serde_json::from_value(
            resp.get("result")
                .cloned()
                .ok_or_else(|| A2aError::InvalidAgentResponse("no result field".into()))?,
        )?;

        Ok(task)
    }

    // -----------------------------------------------------------------------
    // tasks/cancel — cancel a running task
    // -----------------------------------------------------------------------

    pub async fn cancel(&self, task_id: &str) -> Result<Task, A2aError> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": Self::next_id(),
            "method": "tasks/cancel",
            "params": TaskCancelParams { id: task_id.to_string() },
        });

        let resp: serde_json::Value = self
            .http
            .post(&self.base_url)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        if let Some(err) = resp.get("error") {
            return Err(A2aError::Internal(err.to_string()));
        }

        let task: Task = serde_json::from_value(
            resp.get("result")
                .cloned()
                .ok_or_else(|| A2aError::InvalidAgentResponse("no result field".into()))?,
        )?;

        Ok(task)
    }

    // -----------------------------------------------------------------------
    // Convenience: send text task (creates params automatically)
    // -----------------------------------------------------------------------

    /// Send a text message as a new task and wait for completion.
    pub async fn send_text(
        &self,
        task_id: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<Task, A2aError> {
        use crate::types::{MessageRole, Part};
        let params = TaskSendParams {
            id: task_id.into(),
            session_id: None,
            message: Message {
                role: MessageRole::User,
                parts: vec![Part::Text { text: text.into() }],
                metadata: Default::default(),
            },
            history_length: Some(10),
            push_notification: None,
            metadata: Default::default(),
        };
        self.send(params).await
    }
}

// ---------------------------------------------------------------------------
// Helper to extract text from a completed task
// ---------------------------------------------------------------------------

/// Extract the final text reply from a completed A2A task.
pub fn extract_text_reply(task: &Task) -> Option<String> {
    use crate::types::{MessageRole, Part};
    // Try artifacts first
    if let Some(artifact) = task.artifacts.first() {
        let text: String = artifact
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
            .join("\n");
        if !text.is_empty() {
            return Some(text);
        }
    }
    // Fall back to the last agent message in history
    task.history
        .iter()
        .rev()
        .find(|m| m.role == MessageRole::Agent)
        .map(|m| {
            m.parts
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
        })
}
