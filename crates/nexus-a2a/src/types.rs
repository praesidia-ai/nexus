//! A2A protocol wire types (JSON-RPC 2.0 over HTTP + SSE).
//!
//! Based on the official A2A specification:
//! https://github.com/a2aproject/A2A

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Agent Card — the identity document of an A2A agent
// ---------------------------------------------------------------------------

/// An Agent Card advertises an agent's capabilities for discovery.
/// Published at `/.well-known/agent.json` on every A2A server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    /// Human-readable agent name.
    pub name: String,
    /// Semantic version of the agent.
    pub version: String,
    /// Human-readable description.
    pub description: String,
    /// URL of the A2A endpoint that accepts tasks.
    pub url: String,
    /// Contact / ownership info.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<AgentProvider>,
    /// Icon URL (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    /// Which A2A protocol version this agent implements.
    pub protocol_version: String,
    /// Declared skills / capabilities.
    pub capabilities: AgentCapabilities,
    /// Supported authentication schemes.
    pub authentication: Vec<AuthScheme>,
    /// Default input modalities supported.
    pub default_input_modes: Vec<String>,
    /// Default output modalities produced.
    pub default_output_modes: Vec<String>,
    /// List of defined skills (what the agent can do).
    pub skills: Vec<AgentSkill>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProvider {
    pub organization: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapabilities {
    pub streaming: bool,
    pub push_notifications: bool,
    pub state_transition_history: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub input_modes: Vec<String>,
    pub output_modes: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthScheme {
    pub scheme: String,
}

// ---------------------------------------------------------------------------
// Task — the unit of work in A2A
// ---------------------------------------------------------------------------

/// Unique identifier for a task.
pub type TaskId = String;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    Submitted,
    Working,
    InputRequired,
    Completed,
    Failed,
    Canceled,
}

impl std::fmt::Display for TaskState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskState::Submitted => write!(f, "submitted"),
            TaskState::Working => write!(f, "working"),
            TaskState::InputRequired => write!(f, "input_required"),
            TaskState::Completed => write!(f, "completed"),
            TaskState::Failed => write!(f, "failed"),
            TaskState::Canceled => write!(f, "canceled"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub status: TaskStatus,
    pub history: Vec<Message>,
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatus {
    pub state: TaskState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<Message>,
    pub timestamp: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Messages and Parts — the content containers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub parts: Vec<Part>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Part {
    Text { text: String },
    File { file: FileContent },
    Data { data: serde_json::Value },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContent {
    pub name: String,
    pub mime_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<String>, // base64-encoded
}

// ---------------------------------------------------------------------------
// Artifacts — structured outputs produced by the agent
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parts: Vec<Part>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
    #[serde(default)]
    pub append: bool,
    #[serde(default)]
    pub last_chunk: bool,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 envelope types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest<P = serde_json::Value> {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    pub params: P,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse<R = serde_json::Value> {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<R>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Method parameter types
// ---------------------------------------------------------------------------

/// `tasks/send` — send a message and get a task back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSendParams {
    pub id: TaskId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub message: Message,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_length: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_notification: Option<PushNotificationConfig>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// `tasks/sendSubscribe` — send and subscribe to SSE stream.
pub type TaskSendSubscribeParams = TaskSendParams;

/// `tasks/get` — get current task state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskGetParams {
    pub id: TaskId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_length: Option<usize>,
}

/// `tasks/cancel` — cancel a running task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCancelParams {
    pub id: TaskId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushNotificationConfig {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authentication: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// SSE streaming events
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskEvent {
    StatusUpdate {
        id: TaskId,
        status: TaskStatus,
        final_event: bool,
    },
    ArtifactUpdate {
        id: TaskId,
        artifact: Artifact,
    },
}

// ---------------------------------------------------------------------------
// Error codes (A2A spec)
// ---------------------------------------------------------------------------

pub mod error_codes {
    pub const TASK_NOT_FOUND: i32 = -32001;
    pub const TASK_NOT_CANCELABLE: i32 = -32002;
    pub const PUSH_NOTIFICATION_NOT_SUPPORTED: i32 = -32003;
    pub const UNSUPPORTED_OPERATION: i32 = -32004;
    pub const CONTENT_TYPE_NOT_SUPPORTED: i32 = -32005;
    pub const INVALID_AGENT_RESPONSE: i32 = -32006;
}
