//! Shared types for the Super Coding Agent system.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Coding Task — the top-level unit of work
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodingTask {
    pub id: String,
    pub description: String,
    pub mode: CodingMode,
    pub constraints: Vec<String>,
    pub priority: TaskPriority,
    pub max_iterations: u32,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodingMode {
    Auto,
    Pipeline,
    FullPipeline,
    Architect,
    CoderOnly,
    ReviewOnly,
    DebugOnly,
    PerformanceOnly,
    UxOnly,
    DevOpsOnly,
    RefactorOnly,
    ProductOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    Low,
    Normal,
    High,
    Critical,
}

// ---------------------------------------------------------------------------
// Agent Phase — the lifecycle of a coding task
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentPhase {
    Analyze,
    Plan,
    Implement,
    Review,
    Test,
    Debug,
    Verify,
    Optimize,
    Ux,
    Deploy,
    Refactor,
    Product,
    Complete,
    Failed,
}

impl AgentPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Analyze => "analyze",
            Self::Plan => "plan",
            Self::Implement => "implement",
            Self::Review => "review",
            Self::Test => "test",
            Self::Debug => "debug",
            Self::Verify => "verify",
            Self::Optimize => "optimize",
            Self::Ux => "ux",
            Self::Deploy => "deploy",
            Self::Refactor => "refactor",
            Self::Product => "product",
            Self::Complete => "complete",
            Self::Failed => "failed",
        }
    }
}

// ---------------------------------------------------------------------------
// Agent Role — specialized agent types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Architect,
    Coder,
    Reviewer,
    Tester,
    Debugger,
    Performance,
    Ux,
    DevOps,
    Refactor,
    Product,
}

impl AgentRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Architect => "architect",
            Self::Coder => "coder",
            Self::Reviewer => "reviewer",
            Self::Tester => "tester",
            Self::Debugger => "debugger",
            Self::Performance => "performance",
            Self::Ux => "ux",
            Self::DevOps => "devops",
            Self::Refactor => "refactor",
            Self::Product => "product",
        }
    }
}

// ---------------------------------------------------------------------------
// Workspace — shared mutable state across all agents in a task
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct CodingWorkspace {
    pub task: CodingTask,
    pub project_dir: PathBuf,
    pub state: RwLock<WorkspaceState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceState {
    pub phase: AgentPhase,
    pub files_read: Vec<String>,
    pub files_modified: Vec<FileChange>,
    pub plan: Option<ImplementationPlan>,
    pub decisions: Vec<AgentDecision>,
    pub errors: Vec<AgentError>,
    pub verification_results: Vec<VerificationResult>,
    pub iteration_count: u32,
    pub retry_count: u32,
    pub context_summary: String,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            phase: AgentPhase::Analyze,
            files_read: Vec::new(),
            files_modified: Vec::new(),
            plan: None,
            decisions: Vec::new(),
            errors: Vec::new(),
            verification_results: Vec::new(),
            iteration_count: 0,
            retry_count: 0,
            context_summary: String::new(),
        }
    }
}

impl CodingWorkspace {
    pub fn new(task: CodingTask, project_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            task,
            project_dir,
            state: RwLock::new(WorkspaceState::default()),
        })
    }
}

// ---------------------------------------------------------------------------
// Implementation Plan — output of the Architect agent
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationPlan {
    pub summary: String,
    pub steps: Vec<PlanStep>,
    pub files_to_create: Vec<PlannedFile>,
    pub files_to_modify: Vec<PlannedModification>,
    pub files_to_delete: Vec<String>,
    pub dependencies: Vec<String>,
    pub risk_assessment: String,
    pub estimated_complexity: String,
    pub test_strategy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub order: u32,
    pub description: String,
    pub files: Vec<String>,
    pub agent: AgentRole,
    pub depends_on: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedFile {
    pub path: String,
    pub purpose: String,
    pub template: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedModification {
    pub path: String,
    pub description: String,
    pub impact_level: String,
}

// ---------------------------------------------------------------------------
// File Change — tracks what was modified
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub change_type: ChangeType,
    pub agent: AgentRole,
    pub description: String,
    pub diff_summary: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    Created,
    Modified,
    Deleted,
}

// ---------------------------------------------------------------------------
// Decisions and Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDecision {
    pub agent: AgentRole,
    pub phase: AgentPhase,
    pub decision: String,
    pub reasoning: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentError {
    pub agent: AgentRole,
    pub phase: AgentPhase,
    pub error: String,
    pub context: String,
    pub recoverable: bool,
    pub timestamp: String,
}

// ---------------------------------------------------------------------------
// Verification
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub check_type: VerificationType,
    pub passed: bool,
    pub output: String,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub duration_ms: u64,
    pub timestamp: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationType {
    Build,
    Lint,
    TypeCheck,
    UnitTest,
    IntegrationTest,
    SecurityScan,
}

// ---------------------------------------------------------------------------
// Events — streamed to the frontend during execution
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum CodingEvent {
    #[serde(rename = "phase_change")]
    PhaseChange {
        phase: AgentPhase,
        agent: Option<AgentRole>,
    },
    #[serde(rename = "thinking")]
    Thinking { agent: AgentRole, content: String },
    #[serde(rename = "plan_ready")]
    PlanReady { plan: ImplementationPlan },
    #[serde(rename = "file_change")]
    FileChange { change: FileChange },
    #[serde(rename = "tool_call")]
    ToolCall {
        agent: AgentRole,
        tool: String,
        arguments: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        agent: AgentRole,
        tool: String,
        success: bool,
        output: String,
        duration_ms: u64,
    },
    /// One completed LLM call — incremental token + cost counters attributed
    /// to `agent`. Fan-out targets (wave orchestrator, classic orchestrator)
    /// translate this into a per-project `AgentTokens` event for Agent TV.
    #[serde(rename = "llm_usage")]
    LlmUsage {
        agent: AgentRole,
        tokens_in: u64,
        tokens_out: u64,
        cost_usd: f64,
    },
    #[serde(rename = "verification")]
    Verification { result: VerificationResult },
    #[serde(rename = "error_recovery")]
    ErrorRecovery {
        error: String,
        strategy: String,
        retry_count: u32,
    },
    #[serde(rename = "decision")]
    Decision { decision: AgentDecision },
    #[serde(rename = "progress")]
    Progress {
        phase: AgentPhase,
        pct: u8,
        message: String,
    },
    #[serde(rename = "iteration")]
    Iteration {
        number: u32,
        max: u32,
        agent: AgentRole,
    },
    #[serde(rename = "complete")]
    Complete { summary: TaskSummary },
    #[serde(rename = "error")]
    Error { message: String, fatal: bool },
}

// ---------------------------------------------------------------------------
// Task Summary — final report when a coding task completes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    pub task_id: String,
    pub status: String,
    pub phases_completed: Vec<AgentPhase>,
    pub files_created: Vec<String>,
    pub files_modified: Vec<String>,
    pub files_deleted: Vec<String>,
    pub total_iterations: u32,
    pub total_retries: u32,
    pub verification_passed: bool,
    pub decisions: Vec<AgentDecision>,
    pub duration_ms: u64,
    pub agent_stats: HashMap<String, AgentStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStats {
    pub iterations: u32,
    pub tools_used: Vec<String>,
    pub files_touched: Vec<String>,
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// LLM Configuration for the coding system
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CodingLlmConfig {
    pub provider: String,
    pub model: String,
    pub api_key: String,
    pub api_base: String,
    pub max_tokens: u32,
    pub temperature: f32,
}
