//! Agent Process Model — Layer 1 of the Nexus Agent OS.
//!
//! Every running agent is represented as an [`AgentProcess`] with a unique PID,
//! priority, resource limits, and parent/child relationships. The process model
//! supports suspend/resume, wake conditions, and hierarchical process trees.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use nexus_agents_core::definition::AgentDefinition;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Process identifier — a UUID v4 string.
pub type ProcessId = String;

/// The state of an agent process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessState {
    /// Process is ready to run but not yet scheduled.
    Ready,
    /// Process is currently executing on the runtime.
    Running,
    /// Process is waiting on an external resource (tool call, LLM response, etc.).
    Waiting,
    /// Process has been explicitly suspended by the user or scheduler.
    Suspended,
    /// Process is sleeping until a wake condition is met.
    Sleeping,
    /// Process completed successfully.
    Completed,
    /// Process terminated with an error.
    Failed,
    /// Process was explicitly killed.
    Killed,
}

impl ProcessState {
    /// Returns `true` if the process is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Killed)
    }

    /// Returns `true` if the process can transition to the `Running` state.
    pub fn can_run(&self) -> bool {
        matches!(self, Self::Ready | Self::Waiting)
    }

    /// Returns `true` if the process can be suspended.
    pub fn can_suspend(&self) -> bool {
        matches!(self, Self::Running | Self::Waiting | Self::Ready)
    }

    /// Returns `true` if the process can be resumed.
    pub fn can_resume(&self) -> bool {
        matches!(self, Self::Suspended | Self::Sleeping)
    }
}

/// A condition under which a sleeping process should be woken.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WakeCondition {
    /// Wake at a specific time.
    At { time: DateTime<Utc> },
    /// Wake on a cron schedule (standard 5-field cron expression).
    Cron { expression: String },
    /// Wake when an event matching the given channel pattern is published.
    OnEvent { channel_pattern: String },
    /// Wake when any of the given file paths change.
    OnFileChange { paths: Vec<String> },
    /// Wake when a specific process completes.
    OnProcessComplete { pid: ProcessId },
    /// Wake when a webhook is received at the given path.
    OnWebhook { path: String },
}

/// Permissions for tool access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPermissions {
    /// Tools the agent is allowed to use. Empty means all tools allowed.
    pub allowed: Vec<String>,
    /// Tools explicitly denied.
    pub denied: Vec<String>,
    /// Whether the agent can execute arbitrary shell commands.
    pub allow_shell: bool,
}

#[allow(clippy::derivable_impls)]
impl Default for ToolPermissions {
    fn default() -> Self {
        Self {
            allowed: Vec::new(),
            denied: Vec::new(),
            allow_shell: false,
        }
    }
}

/// Permissions for network access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPermissions {
    /// Allowed URL patterns (glob). Empty means no network access.
    pub allowed_urls: Vec<String>,
    /// Whether outbound HTTP requests are allowed.
    pub allow_outbound: bool,
    /// Whether the agent can listen on ports.
    pub allow_listen: bool,
}

#[allow(clippy::derivable_impls)]
impl Default for NetworkPermissions {
    fn default() -> Self {
        Self {
            allowed_urls: Vec::new(),
            allow_outbound: false,
            allow_listen: false,
        }
    }
}

/// Permissions for file system access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePermissions {
    /// Directories the agent can read from.
    pub read_paths: Vec<String>,
    /// Directories the agent can write to.
    pub write_paths: Vec<String>,
    /// Whether the agent can create new files.
    pub allow_create: bool,
    /// Whether the agent can delete files.
    pub allow_delete: bool,
}

#[allow(clippy::derivable_impls)]
impl Default for FilePermissions {
    fn default() -> Self {
        Self {
            read_paths: Vec::new(),
            write_paths: Vec::new(),
            allow_create: false,
            allow_delete: false,
        }
    }
}

/// Resource allocation and limits for a process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceAllocation {
    /// Maximum LLM tokens this process can consume.
    pub max_tokens: u64,
    /// Maximum cost in USD this process is allowed to incur.
    pub max_cost_usd: f64,
    /// Maximum memory in bytes (for context windows, caches, etc.).
    pub max_memory_bytes: u64,
    /// Maximum wall-clock time in seconds.
    pub max_duration_secs: u64,
    /// Maximum number of tool call iterations.
    pub max_iterations: u32,
    /// Tool permissions for this process.
    pub tool_permissions: ToolPermissions,
    /// Network permissions for this process.
    pub network_permissions: NetworkPermissions,
    /// File permissions for this process.
    pub file_permissions: FilePermissions,
}

#[allow(clippy::derivable_impls)]
impl Default for ResourceAllocation {
    fn default() -> Self {
        Self {
            max_tokens: 100_000,
            max_cost_usd: 1.0,
            max_memory_bytes: 256 * 1024 * 1024, // 256 MB
            max_duration_secs: 300,
            max_iterations: 50,
            tool_permissions: ToolPermissions::default(),
            network_permissions: NetworkPermissions::default(),
            file_permissions: FilePermissions::default(),
        }
    }
}

/// Priority level for scheduling. Higher priority processes are scheduled first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    /// Idle — only runs when nothing else is queued.
    Idle = 0,
    /// Low priority.
    Low = 1,
    /// Normal priority (default).
    Normal = 2,
    /// High priority.
    High = 3,
    /// Critical — preempts all other work.
    Critical = 4,
}

impl PartialOrd for Priority {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Priority {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (*self as u8).cmp(&(*other as u8))
    }
}

#[allow(clippy::derivable_impls)]
impl Default for Priority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Runtime context for a process — mutable state accumulated during execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessContext {
    /// The task description or prompt this process is working on.
    pub task: String,
    /// Messages received from other processes.
    pub messages: Vec<serde_json::Value>,
    /// Scratch data for intermediate results.
    pub scratch: HashMap<String, serde_json::Value>,
    /// Tokens consumed so far.
    pub tokens_used: u64,
    /// Cost incurred so far in USD.
    pub cost_used: f64,
    /// Number of iterations completed.
    pub iterations: u32,
    /// Output produced by the process (set on completion).
    pub output: Option<String>,
    /// Error message (set on failure).
    pub error: Option<String>,
}

impl ProcessContext {
    /// Create a new process context for the given task.
    pub fn new(task: String) -> Self {
        Self {
            task,
            messages: Vec::new(),
            scratch: HashMap::new(),
            tokens_used: 0,
            cost_used: 0.0,
            iterations: 0,
            output: None,
            error: None,
        }
    }
}

/// A running agent process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProcess {
    /// Unique process identifier.
    pub pid: ProcessId,
    /// The agent definition this process is running.
    pub agent: AgentDefinition,
    /// Current process state.
    pub state: ProcessState,
    /// Scheduling priority.
    pub priority: Priority,
    /// Resource allocation and limits.
    pub resources: ResourceAllocation,
    /// Runtime context (task, messages, scratch data).
    pub context: ProcessContext,
    /// Parent process ID (if spawned by another process).
    pub parent: Option<ProcessId>,
    /// Child process IDs.
    pub children: Vec<ProcessId>,
    /// Wake condition (set when the process is sleeping).
    pub wake_condition: Option<WakeCondition>,
    /// When the process was created.
    pub created_at: DateTime<Utc>,
    /// When the process last changed state.
    pub updated_at: DateTime<Utc>,
    /// When the process started running (first transition to Running).
    pub started_at: Option<DateTime<Utc>>,
    /// When the process reached a terminal state.
    pub finished_at: Option<DateTime<Utc>>,
}

impl AgentProcess {
    /// Create a new process in the Ready state.
    pub fn new(
        agent: AgentDefinition,
        task: String,
        priority: Priority,
        resources: ResourceAllocation,
        parent: Option<ProcessId>,
    ) -> Self {
        let now = Utc::now();
        Self {
            pid: Uuid::new_v4().to_string(),
            agent,
            state: ProcessState::Ready,
            priority,
            resources,
            context: ProcessContext::new(task),
            parent,
            children: Vec::new(),
            wake_condition: None,
            created_at: now,
            updated_at: now,
            started_at: None,
            finished_at: None,
        }
    }

    /// Transition the process to a new state. Returns an error if the transition is invalid.
    pub fn transition_to(&mut self, new_state: ProcessState) -> Result<ProcessState, ProcessError> {
        let old_state = self.state.clone();
        let valid = match (&old_state, &new_state) {
            // From Ready
            (ProcessState::Ready, ProcessState::Running) => true,
            (ProcessState::Ready, ProcessState::Suspended) => true,
            (ProcessState::Ready, ProcessState::Killed) => true,
            // From Running
            (ProcessState::Running, ProcessState::Waiting) => true,
            (ProcessState::Running, ProcessState::Suspended) => true,
            (ProcessState::Running, ProcessState::Sleeping) => true,
            (ProcessState::Running, ProcessState::Completed) => true,
            (ProcessState::Running, ProcessState::Failed) => true,
            (ProcessState::Running, ProcessState::Killed) => true,
            // From Waiting
            (ProcessState::Waiting, ProcessState::Running) => true,
            (ProcessState::Waiting, ProcessState::Suspended) => true,
            (ProcessState::Waiting, ProcessState::Failed) => true,
            (ProcessState::Waiting, ProcessState::Killed) => true,
            // From Suspended
            (ProcessState::Suspended, ProcessState::Ready) => true,
            (ProcessState::Suspended, ProcessState::Killed) => true,
            // From Sleeping
            (ProcessState::Sleeping, ProcessState::Ready) => true,
            (ProcessState::Sleeping, ProcessState::Killed) => true,
            // Terminal states cannot transition
            _ => false,
        };

        if !valid {
            return Err(ProcessError::InvalidTransition {
                pid: self.pid.clone(),
                from: old_state,
                to: new_state,
            });
        }

        self.state = new_state.clone();
        self.updated_at = Utc::now();

        if new_state == ProcessState::Running && self.started_at.is_none() {
            self.started_at = Some(self.updated_at);
        }

        if new_state.is_terminal() {
            self.finished_at = Some(self.updated_at);
        }

        Ok(old_state)
    }
}

/// Public view of a process (safe to expose in APIs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: ProcessId,
    pub agent_id: String,
    pub agent_name: String,
    pub state: ProcessState,
    pub priority: Priority,
    pub task: String,
    pub parent: Option<ProcessId>,
    pub children: Vec<ProcessId>,
    pub tokens_used: u64,
    pub cost_used: f64,
    pub iterations: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl From<&AgentProcess> for ProcessInfo {
    fn from(p: &AgentProcess) -> Self {
        Self {
            pid: p.pid.clone(),
            agent_id: p.agent.id.clone(),
            agent_name: p.agent.name.clone(),
            state: p.state.clone(),
            priority: p.priority,
            task: p.context.task.clone(),
            parent: p.parent.clone(),
            children: p.children.clone(),
            tokens_used: p.context.tokens_used,
            cost_used: p.context.cost_used,
            iterations: p.context.iterations,
            created_at: p.created_at,
            updated_at: p.updated_at,
            started_at: p.started_at,
            finished_at: p.finished_at,
        }
    }
}

/// Errors from process operations.
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("process {pid} not found")]
    NotFound { pid: ProcessId },

    #[error("invalid state transition for process {pid}: {from:?} -> {to:?}")]
    InvalidTransition {
        pid: ProcessId,
        from: ProcessState,
        to: ProcessState,
    },

    #[error("process {pid} resource limit exceeded: {resource}")]
    ResourceLimitExceeded { pid: ProcessId, resource: String },

    #[error("process {pid} permission denied: {reason}")]
    PermissionDenied { pid: ProcessId, reason: String },

    #[error("scheduler error: {0}")]
    SchedulerError(String),
}
