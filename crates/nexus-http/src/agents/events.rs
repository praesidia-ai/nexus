//! Agent execution events — emitted during agent runs.
//!
//! All agent types (interactive, background, parallel) emit the same event
//! vocabulary so consumers only need a single handler.

use serde::Serialize;

/// Events emitted during agent execution.
///
/// Streamed over SSE to frontends and logged for observability.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    /// Agent execution started.
    Started {
        /// Which agent started.
        agent_id: String,
        /// The task prompt.
        task: String,
    },
    /// Agent is reasoning/thinking.
    Thinking {
        /// Which agent is thinking.
        agent_id: String,
        /// The thinking content.
        content: String,
    },
    /// Agent invoked a tool.
    ToolInvoked {
        /// Which agent invoked the tool.
        agent_id: String,
        /// Tool name.
        tool: String,
        /// Tool input parameters.
        input: serde_json::Value,
    },
    /// Tool returned a result.
    ToolResult {
        /// Which agent's tool returned.
        agent_id: String,
        /// Tool name.
        tool: String,
        /// Tool output.
        output: serde_json::Value,
    },
    /// Progress update.
    Progress {
        /// Which agent.
        agent_id: String,
        /// Current iteration number.
        iteration: u32,
        /// Maximum allowed iterations.
        max_iterations: u32,
        /// Human-readable summary of progress.
        summary: String,
    },
    /// Agent completed successfully.
    Completed {
        /// Which agent completed.
        agent_id: String,
        /// The execution result.
        result: AgentResult,
    },
    /// Agent failed.
    Failed {
        /// Which agent failed.
        agent_id: String,
        /// Error message.
        error: String,
        /// Iteration at which the failure occurred.
        iteration: u32,
    },
}

/// Result of an agent execution.
#[derive(Debug, Clone, Serialize)]
pub struct AgentResult {
    /// Final text output from the agent.
    pub output: String,
    /// Files modified during execution.
    pub files_modified: Vec<String>,
    /// Total iterations used.
    pub iterations_used: u32,
    /// Total execution time in milliseconds.
    pub duration_ms: u64,
    /// Tool calls made during execution.
    pub tool_calls: Vec<ToolCallRecord>,
}

/// Record of a tool call made during execution.
#[derive(Debug, Clone, Serialize)]
pub struct ToolCallRecord {
    /// Tool name.
    pub tool: String,
    /// Truncated summary of the input.
    pub input_summary: String,
    /// Truncated summary of the output.
    pub output_summary: String,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// Whether the tool call succeeded.
    pub success: bool,
}
