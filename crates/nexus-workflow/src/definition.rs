//! Workflow definition types — the declarative "blueprint" for a workflow.
//!
//! A [`WorkflowDefinition`] is a list of [`WorkflowStep`]s with an error policy.
//! Steps declare dependencies via `depends_on` (referencing other step IDs) and
//! the engine executes them in topological order, running independent steps in
//! parallel.

use serde::{Deserialize, Serialize};

/// A complete workflow definition that can be started by the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    /// Unique identifier for this definition (e.g. `"full-build-pipeline"`).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Ordered list of steps. Dependencies between steps are expressed via
    /// [`WorkflowStep::depends_on`].
    pub steps: Vec<WorkflowStep>,
    /// What to do when a step fails.
    #[serde(default)]
    pub on_error: ErrorPolicy,
    /// Definition version — increment when modifying a definition to track
    /// which version an instance was created from.
    #[serde(default = "default_version")]
    pub version: u32,
    /// When `true`, this definition is a template that can be instantiated
    /// multiple times with different parameters via
    /// [`WorkflowEngine::instantiate`].
    #[serde(default)]
    pub template: bool,
}

fn default_version() -> u32 {
    1
}

/// A single step within a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    /// Unique identifier for this step within the workflow.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// What this step actually does.
    pub action: StepAction,
    /// IDs of steps that must complete before this one can start.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Optional condition that controls whether this step executes.
    /// If the condition evaluates to false, the step is skipped.
    #[serde(default)]
    pub condition: Option<StepCondition>,
    /// Retry policy for this step.
    #[serde(default)]
    pub retry: RetryPolicy,
    /// Maximum execution time in seconds before the step is considered timed out.
    /// Defaults to 300 (5 minutes).
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    /// Optional timeout in milliseconds (takes precedence over `timeout_secs`
    /// when set). Provides finer-grained control for fast steps.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Whether this step should run in parallel with other steps that share
    /// the same `parallel_group`.
    #[serde(default)]
    pub parallel: bool,
    /// Optional group name — all steps in the same parallel group with no
    /// unsatisfied dependencies are spawned concurrently.
    #[serde(default)]
    pub parallel_group: Option<String>,
    /// Maximum number of times this step is allowed to execute across the
    /// entire workflow instance. Used for intentional loops to prevent
    /// runaway execution.
    #[serde(default)]
    pub max_iterations: Option<u32>,
}

fn default_timeout_secs() -> u64 {
    300
}

/// Condition that determines whether a step should execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StepCondition {
    /// Always execute (default behavior).
    Always,
    /// Execute only if the referenced step succeeded.
    OnSuccess {
        /// ID of the step to check.
        step_id: String,
    },
    /// Execute only if the referenced step failed.
    OnFailure {
        /// ID of the step to check.
        step_id: String,
    },
    /// Evaluate a simple expression against the workflow context and step
    /// outputs.
    ///
    /// Supported expressions:
    /// - `steps.<step_id>.status == "success"` / `"failed"`
    /// - `steps.<step_id>.output.contains("<substring>")`
    /// - Dot-path into context: `build.success` (truthiness check)
    Expression {
        /// The expression string to evaluate.
        expr: String,
    },
}

/// The action a step performs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StepAction {
    /// Dispatch work to a single agent.
    Agent {
        /// Agent identifier (e.g. `"nova"`, `"orion"`).
        agent_id: String,
        /// Prompt or instruction for the agent.
        prompt: String,
    },
    /// Dispatch work to a team of agents.
    Team {
        /// Team identifier.
        team_id: String,
        /// Prompt or instruction for the team.
        prompt: String,
    },
    /// Generate content via LLM.
    Generate {
        /// Model to use (e.g. `"gpt-4o"`).
        model: String,
        /// System prompt.
        system: String,
        /// User prompt.
        prompt: String,
    },
    /// Run a shell command.
    Command {
        /// The command to execute.
        cmd: String,
        /// Optional working directory.
        cwd: Option<String>,
    },
    /// Run a quality gate check.
    QualityGate {
        /// What to check (e.g. `"lint"`, `"test"`, `"security"`).
        check: String,
    },
    /// Pause and wait for human approval.
    HumanApproval {
        /// Description of what the operator should review.
        description: String,
    },
    /// Call an external webhook.
    Webhook {
        /// URL to POST to.
        url: String,
        /// Optional request body.
        body: Option<serde_json::Value>,
    },
    /// Run multiple sub-steps in parallel.
    Parallel {
        /// Sub-steps to execute concurrently.
        steps: Vec<WorkflowStep>,
    },
    /// Custom handler — the engine emits the step and expects the caller to
    /// provide a result via the API.
    Custom {
        /// Handler name for routing.
        handler: String,
        /// Arbitrary configuration.
        config: serde_json::Value,
    },
}

/// Retry policy for a single step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts (0 = no retries).
    #[serde(default)]
    pub max_retries: u32,
    /// Base backoff in seconds between retries. Actual backoff is
    /// `backoff_secs * 2^attempt` (exponential).
    #[serde(default = "default_backoff")]
    pub backoff_secs: u64,
}

fn default_backoff() -> u64 {
    2
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 0,
            backoff_secs: 2,
        }
    }
}

/// Error handling policy for the entire workflow.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorPolicy {
    /// Stop the workflow immediately on first failure.
    #[default]
    FailFast,
    /// Continue executing independent steps even if one fails.
    ContinueOnError,
    /// Attempt to roll back completed steps (best effort).
    Rollback,
}
