//! A2A server handler — accept incoming tasks and dispatch to local agents.
//!
//! This module provides the core JSON-RPC dispatch logic. Nexus exposes an
//! A2A endpoint that other agents can call to delegate tasks.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::error::A2aError;
use crate::types::{
    Artifact, Message, MessageRole, Part, Task, TaskCancelParams, TaskGetParams, TaskId,
    TaskSendParams, TaskState, TaskStatus,
};

// ---------------------------------------------------------------------------
// Task handler trait — implement this to connect A2A to your agent system
// ---------------------------------------------------------------------------

/// Implement this trait to plug Nexus agents into the A2A server.
#[async_trait::async_trait]
pub trait TaskHandler: Send + Sync + 'static {
    /// Execute the task (blocking until complete).
    /// Return a list of artifacts produced.
    async fn handle(&self, task_id: &str, message: &Message) -> Result<Vec<Artifact>, A2aError>;
}

// ---------------------------------------------------------------------------
// A2A server — stores in-flight tasks and dispatches via TaskHandler
// ---------------------------------------------------------------------------

pub struct A2aServer {
    handler: Arc<dyn TaskHandler>,
    tasks: Arc<RwLock<HashMap<TaskId, Task>>>,
}

impl A2aServer {
    pub fn new(handler: Arc<dyn TaskHandler>) -> Self {
        Self {
            handler,
            tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Dispatch a raw JSON-RPC request body, return the JSON-RPC response.
    pub async fn dispatch(&self, body: serde_json::Value) -> serde_json::Value {
        let id = body.get("id").cloned().unwrap_or(serde_json::Value::Null);
        let method = body
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();
        let params = body.get("params").cloned().unwrap_or_default();

        let result = match method.as_str() {
            "tasks/send" => self.handle_send(params).await,
            "tasks/get" => self.handle_get(params).await,
            "tasks/cancel" => self.handle_cancel(params).await,
            other => Err(A2aError::UnsupportedOperation(format!(
                "Unknown method: {other}"
            ))),
        };

        match result {
            Ok(val) => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": val,
            }),
            Err(e) => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": e.rpc_code(),
                    "message": e.to_string(),
                },
            }),
        }
    }

    // -----------------------------------------------------------------------
    // tasks/send
    // -----------------------------------------------------------------------

    async fn handle_send(&self, params: serde_json::Value) -> Result<serde_json::Value, A2aError> {
        let p: TaskSendParams = serde_json::from_value(params)?;
        let task_id = p.id.clone();

        // Mark task as submitted
        {
            let mut tasks = self.tasks.write().await;
            let task = Task {
                id: task_id.clone(),
                session_id: p.session_id.clone(),
                status: TaskStatus {
                    state: TaskState::Submitted,
                    message: None,
                    timestamp: Utc::now(),
                },
                history: vec![p.message.clone()],
                artifacts: vec![],
                metadata: p.metadata.clone(),
            };
            tasks.insert(task_id.clone(), task);
        }

        // Mark as working
        {
            let mut tasks = self.tasks.write().await;
            if let Some(task) = tasks.get_mut(&task_id) {
                task.status.state = TaskState::Working;
                task.status.timestamp = Utc::now();
            }
        }

        info!(task_id = %task_id, "A2A task executing");

        // Execute
        let artifacts = match self.handler.handle(&task_id, &p.message).await {
            Ok(arts) => arts,
            Err(e) => {
                warn!(task_id = %task_id, error = %e, "A2A task failed");
                let mut tasks = self.tasks.write().await;
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.status.state = TaskState::Failed;
                    task.status.timestamp = Utc::now();
                    task.status.message = Some(Message {
                        role: MessageRole::Agent,
                        parts: vec![Part::Text {
                            text: e.to_string(),
                        }],
                        metadata: Default::default(),
                    });
                }
                return Err(e);
            }
        };

        // Mark as completed
        let final_task = {
            let mut tasks = self.tasks.write().await;
            if let Some(task) = tasks.get_mut(&task_id) {
                task.status.state = TaskState::Completed;
                task.status.timestamp = Utc::now();
                task.artifacts = artifacts;
                task.clone()
            } else {
                return Err(A2aError::TaskNotFound(task_id));
            }
        };

        info!(task_id = %final_task.id, "A2A task completed");
        Ok(serde_json::to_value(final_task)?)
    }

    // -----------------------------------------------------------------------
    // tasks/get
    // -----------------------------------------------------------------------

    async fn handle_get(&self, params: serde_json::Value) -> Result<serde_json::Value, A2aError> {
        let p: TaskGetParams = serde_json::from_value(params)?;
        let tasks = self.tasks.read().await;
        let task = tasks
            .get(&p.id)
            .ok_or_else(|| A2aError::TaskNotFound(p.id.clone()))?;
        Ok(serde_json::to_value(task)?)
    }

    // -----------------------------------------------------------------------
    // tasks/cancel
    // -----------------------------------------------------------------------

    async fn handle_cancel(
        &self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, A2aError> {
        let p: TaskCancelParams = serde_json::from_value(params)?;
        let mut tasks = self.tasks.write().await;
        let task = tasks
            .get_mut(&p.id)
            .ok_or_else(|| A2aError::TaskNotFound(p.id.clone()))?;

        match task.status.state {
            TaskState::Completed | TaskState::Failed | TaskState::Canceled => {
                return Err(A2aError::TaskNotCancelable(task.status.state.to_string()))
            }
            _ => {}
        }

        task.status.state = TaskState::Canceled;
        task.status.timestamp = Utc::now();
        Ok(serde_json::to_value(task.clone())?)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{error_codes, Message, MessageRole, Part};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Echoes the request message back as a single artifact.
    struct EchoHandler {
        calls: AtomicUsize,
    }

    impl EchoHandler {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                calls: AtomicUsize::new(0),
            })
        }
    }

    #[async_trait::async_trait]
    impl TaskHandler for EchoHandler {
        async fn handle(
            &self,
            _task_id: &str,
            message: &Message,
        ) -> Result<Vec<Artifact>, A2aError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(vec![Artifact {
                name: "echo".into(),
                description: None,
                parts: message.parts.clone(),
                index: Some(0),
                append: false,
                last_chunk: true,
                metadata: HashMap::new(),
            }])
        }
    }

    /// Always fails — used to verify error propagation.
    struct FailHandler;

    #[async_trait::async_trait]
    impl TaskHandler for FailHandler {
        async fn handle(
            &self,
            _task_id: &str,
            _message: &Message,
        ) -> Result<Vec<Artifact>, A2aError> {
            Err(A2aError::Internal("boom".into()))
        }
    }

    fn user_msg(text: &str) -> Message {
        Message {
            role: MessageRole::User,
            parts: vec![Part::Text { text: text.into() }],
            metadata: HashMap::new(),
        }
    }

    fn rpc(method: &str, params: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        })
    }

    #[tokio::test]
    async fn tasks_send_happy_path_marks_completed_with_artifacts() {
        let handler = EchoHandler::new();
        let server = A2aServer::new(handler.clone());

        let resp = server
            .dispatch(rpc(
                "tasks/send",
                serde_json::json!({
                    "id": "task-1",
                    "message": user_msg("hello"),
                }),
            ))
            .await;

        assert_eq!(resp["jsonrpc"], "2.0");
        assert_eq!(resp["id"], 1);
        assert!(resp.get("error").is_none(), "got error: {resp:?}");

        let task = resp.get("result").expect("result present");
        assert_eq!(task["id"], "task-1");
        assert_eq!(task["status"]["state"], "completed");
        assert_eq!(task["artifacts"].as_array().unwrap().len(), 1);
        assert_eq!(handler.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn tasks_get_returns_previously_sent_task() {
        let server = A2aServer::new(EchoHandler::new());
        // Send first
        let _ = server
            .dispatch(rpc(
                "tasks/send",
                serde_json::json!({
                    "id": "task-2",
                    "message": user_msg("ping"),
                }),
            ))
            .await;

        let resp = server
            .dispatch(rpc("tasks/get", serde_json::json!({ "id": "task-2" })))
            .await;

        assert!(resp.get("error").is_none());
        assert_eq!(resp["result"]["id"], "task-2");
        assert_eq!(resp["result"]["status"]["state"], "completed");
    }

    #[tokio::test]
    async fn tasks_get_unknown_returns_task_not_found() {
        let server = A2aServer::new(EchoHandler::new());
        let resp = server
            .dispatch(rpc("tasks/get", serde_json::json!({ "id": "no-such" })))
            .await;
        let err = resp.get("error").expect("error present");
        assert_eq!(err["code"], error_codes::TASK_NOT_FOUND);
    }

    #[tokio::test]
    async fn tasks_cancel_completed_returns_not_cancelable() {
        let server = A2aServer::new(EchoHandler::new());
        let _ = server
            .dispatch(rpc(
                "tasks/send",
                serde_json::json!({
                    "id": "task-3",
                    "message": user_msg("done"),
                }),
            ))
            .await;

        let resp = server
            .dispatch(rpc("tasks/cancel", serde_json::json!({ "id": "task-3" })))
            .await;
        let err = resp.get("error").expect("error present");
        assert_eq!(err["code"], error_codes::TASK_NOT_CANCELABLE);
    }

    #[tokio::test]
    async fn tasks_cancel_unknown_returns_task_not_found() {
        let server = A2aServer::new(EchoHandler::new());
        let resp = server
            .dispatch(rpc("tasks/cancel", serde_json::json!({ "id": "ghost" })))
            .await;
        let err = resp.get("error").expect("error present");
        assert_eq!(err["code"], error_codes::TASK_NOT_FOUND);
    }

    #[tokio::test]
    async fn unknown_method_returns_unsupported_operation() {
        let server = A2aServer::new(EchoHandler::new());
        let resp = server
            .dispatch(rpc("tasks/teleport", serde_json::json!({})))
            .await;
        let err = resp.get("error").expect("error present");
        assert_eq!(err["code"], error_codes::UNSUPPORTED_OPERATION);
    }

    #[tokio::test]
    async fn handler_failure_returns_jsonrpc_error_and_marks_failed() {
        let server = A2aServer::new(Arc::new(FailHandler));

        let resp = server
            .dispatch(rpc(
                "tasks/send",
                serde_json::json!({
                    "id": "task-4",
                    "message": user_msg("fail-me"),
                }),
            ))
            .await;

        // tasks/send returns the JSON-RPC error envelope on handler failure.
        let err = resp.get("error").expect("error present");
        assert_eq!(err["code"], -32603, "expected internal-error code");

        // The stored task should be in Failed state.
        let lookup = server
            .dispatch(rpc("tasks/get", serde_json::json!({ "id": "task-4" })))
            .await;
        assert_eq!(lookup["result"]["status"]["state"], "failed");
    }

    #[tokio::test]
    async fn malformed_params_return_jsonrpc_error_not_panic() {
        let server = A2aServer::new(EchoHandler::new());
        // `tasks/send` requires `id` and `message`; supply neither.
        let resp = server
            .dispatch(rpc("tasks/send", serde_json::json!({ "wrong": true })))
            .await;
        assert!(
            resp.get("error").is_some(),
            "should not panic, got {resp:?}"
        );
    }

    #[tokio::test]
    async fn tasks_send_preserves_jsonrpc_id_on_error() {
        let server = A2aServer::new(EchoHandler::new());
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "client-supplied-id-42",
            "method": "tasks/get",
            "params": { "id": "missing" },
        });
        let resp = server.dispatch(body).await;
        assert_eq!(resp["id"], "client-supplied-id-42");
        assert!(resp.get("error").is_some());
    }
}
