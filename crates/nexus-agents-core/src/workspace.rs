//! Shared workspace types for team collaboration.
//!
//! When a team of agents works on a project, they share a workspace with
//! a task board, artifacts, key-value state, and a scratchpad. This module
//! defines the data types; the runtime workspace is in `nexus-http`.

use serde::{Deserialize, Serialize};

/// A task on the team's shared task board.
///
/// Tasks are created by the lead/coordinator and assigned to specialists.
/// Dependencies between tasks are tracked so the orchestrator can determine
/// which tasks are ready to execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique task identifier.
    pub id: String,
    /// Short title for the task.
    pub title: String,
    /// Detailed description of what needs to be done.
    pub description: String,
    /// Current status.
    pub status: TaskStatus,
    /// ID of the team member this task is assigned to (if any).
    pub assigned_to: Option<String>,
    /// ID of the team member who created this task.
    pub created_by: String,
    /// IDs of tasks that must complete before this one can start.
    pub dependencies: Vec<String>,
    /// IDs of subtasks that decompose this task further.
    pub subtasks: Vec<String>,
    /// Task priority (higher = more important).
    pub priority: TaskPriority,
}

/// Status of a task on the board.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Not yet scheduled — waiting in the backlog.
    Backlog,
    /// All dependencies met — ready to be picked up.
    Ready,
    /// Currently being worked on by the assigned member.
    InProgress,
    /// Work done, awaiting review.
    InReview,
    /// Blocked by an external dependency or issue.
    Blocked,
    /// Successfully completed.
    Done,
    /// Failed and will not be retried.
    Failed,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Backlog => write!(f, "backlog"),
            Self::Ready => write!(f, "ready"),
            Self::InProgress => write!(f, "in_progress"),
            Self::InReview => write!(f, "in_review"),
            Self::Blocked => write!(f, "blocked"),
            Self::Done => write!(f, "done"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

/// Priority level for a task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    /// Low priority — do when other work is done.
    Low,
    /// Normal priority.
    Normal,
    /// High priority — do before normal tasks.
    High,
    /// Critical — do immediately.
    Critical,
}

/// An artifact produced by a team member.
///
/// Artifacts are the tangible outputs of agent work: code files, documents,
/// plans, review reports, data, etc. They are published to the shared
/// workspace and can be referenced by other agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    /// Unique artifact identifier.
    pub id: String,
    /// Human-readable name (e.g., "login-page.tsx", "architecture-plan").
    pub name: String,
    /// The type of artifact.
    pub artifact_type: ArtifactType,
    /// The artifact content (code, text, JSON, etc.).
    pub content: String,
    /// ID of the team member who produced this artifact.
    pub produced_by: String,
    /// ID of the task this artifact was produced for (if any).
    pub task_id: Option<String>,
    /// Version number (incremented on updates).
    pub version: u32,
    /// ISO 8601 timestamp of creation.
    pub timestamp: String,
}

/// Type of an artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    /// Source code.
    Code,
    /// Written document (README, spec, etc.).
    Document,
    /// Execution plan or task breakdown.
    Plan,
    /// A decision record (ADR, design choice).
    Decision,
    /// Data output (CSV, JSON dataset, etc.).
    Data,
    /// A review report.
    Review,
    /// Custom artifact type.
    Custom {
        /// The custom type name.
        name: String,
    },
}

/// A point-in-time snapshot of the team workspace.
///
/// Used by the orchestrator to inject workspace state into agent prompts
/// and to persist workspace state for resume/replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    /// All tasks on the board.
    pub tasks: Vec<Task>,
    /// All published artifacts.
    pub artifacts: Vec<Artifact>,
    /// Key-value state entries.
    pub state: Vec<(String, serde_json::Value)>,
    /// Scratchpad entries: (author, topic, content).
    pub scratchpad: Vec<(String, String, String)>,
}
