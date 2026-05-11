//! Core trait that every Coding Agent must implement.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use super::types::*;

/// Context provided to each coding agent during execution.
pub struct CodingAgentContext {
    pub workspace: Arc<CodingWorkspace>,
    pub llm_config: CodingLlmConfig,
    pub event_tx: mpsc::Sender<CodingEvent>,
    pub code_graph_context: String,
    pub brain_context: String,
    pub memory_context: String,
    pub previous_agent_output: Option<String>,
    /// Optional cost-tracker handle. When present, the engine records every
    /// LLM call here so per-tenant budgets reflect real spend. `None` on
    /// tests / places that don't go through AppState.
    pub cost_tracker: Option<crate::cost_intelligence::CostTrackerRef>,
    /// Project ID for cost attribution. Required alongside `cost_tracker`
    /// for per-project budget enforcement.
    pub project_id: Option<String>,
}

/// Result returned by a coding agent after its turn.
#[derive(Debug, Clone)]
pub struct AgentOutput {
    pub agent: AgentRole,
    pub summary: String,
    pub files_changed: Vec<FileChange>,
    pub decisions: Vec<AgentDecision>,
    pub errors: Vec<AgentError>,
    pub should_continue: bool,
    pub next_phase: Option<AgentPhase>,
    pub iterations_used: u32,
}

/// The core trait for all specialized coding agents.
///
/// Each agent receives a context with the shared workspace, LLM config,
/// and outputs from previous agents. It performs its work by calling the
/// LLM with tools, streaming events, and updating the workspace.
#[async_trait]
pub trait CodingAgent: Send + Sync + 'static {
    fn role(&self) -> AgentRole;
    fn name(&self) -> &str;

    fn system_prompt(&self, ctx: &CodingAgentContext) -> String;

    fn max_iterations(&self) -> u32 {
        15
    }

    fn tools_allowed(&self) -> Vec<&str> {
        vec![
            "file_read",
            "file_write",
            "file_edit",
            "list_directory",
            "grep",
            "glob",
            "bash",
            "git_status",
            "git_diff",
        ]
    }

    async fn execute(&self, ctx: &CodingAgentContext) -> anyhow::Result<AgentOutput>;

    fn can_handle_error(&self, _error: &AgentError) -> bool {
        false
    }
}
