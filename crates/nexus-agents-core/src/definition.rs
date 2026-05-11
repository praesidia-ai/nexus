//! Agent definitions — what an agent knows and can do.
//!
//! An [`AgentDefinition`] captures everything needed to instantiate and run an
//! agent: identity, system prompt, allowed tools, execution mode, and resource
//! limits. Definitions are pure data — the runtime turns them into executing tasks.

use serde::{Deserialize, Serialize};

/// An agent definition — describes an agent's identity, capabilities, and execution parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    /// Unique identifier for this agent definition.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// System prompt that defines the agent's personality and capabilities.
    pub system_prompt: String,
    /// Skills this agent has (e.g., "code_review", "debugging").
    pub skills: Vec<String>,
    /// Tool names this agent can use (resolved from ToolRegistry at runtime).
    pub tools: Vec<String>,
    /// Model preference (provider, model name).
    pub model_preference: Option<ModelPreference>,
    /// How this agent executes.
    pub execution_mode: ExecutionMode,
    /// Maximum tool-use iterations before forced stop.
    pub max_iterations: u32,
    /// Maximum wall-clock time for execution in seconds.
    pub timeout_secs: u64,
}

impl AgentDefinition {
    /// Create a minimal agent definition with just a name.
    pub fn default_with_name(name: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            system_prompt: format!("You are {name}, a helpful AI assistant."),
            skills: Vec::new(),
            tools: Vec::new(),
            model_preference: None,
            execution_mode: ExecutionMode::Interactive,
            max_iterations: 20,
            timeout_secs: 300,
        }
    }
}

/// Model preference for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPreference {
    /// LLM provider name (e.g., "openai", "anthropic", "ollama").
    pub provider: String,
    /// Model identifier (e.g., "gpt-4o", "claude-sonnet-4-6").
    pub model: String,
}

/// How an agent executes its task.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ExecutionMode {
    /// Interactive: streams events, waits for tool results.
    Interactive,
    /// Background: runs async, reports completion.
    Background {
        /// Conditions that trigger notifications.
        notify_on: Vec<NotifyCondition>,
    },
    /// Parallel: runs multiple agents concurrently, merges results.
    Parallel {
        /// Strategy for combining outputs from parallel agents.
        merge_strategy: MergeStrategy,
    },
}

/// When to notify during background execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotifyCondition {
    /// Notify when the agent completes successfully.
    Completion,
    /// Notify when the agent fails.
    Failure,
    /// Notify after a specific number of iterations.
    IterationThreshold(u32),
}

/// How to merge results from parallel agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    /// Concatenate all outputs.
    Concatenate,
    /// Take the best result by quality score.
    BestOf,
    /// Merge file changes (like git merge).
    FileMerge,
}
