//! Team definitions — multi-agent collaboration.
//!
//! A team is a named group of agents with roles, coordination protocols,
//! and shared budgets. Teams enable complex tasks to be decomposed across
//! specialist agents working together.

use serde::{Deserialize, Serialize};

use super::definition::AgentDefinition;

/// A team is a named group of agents with roles, protocols, and shared workspace.
///
/// Teams support six coordination protocols, each suited to different task
/// patterns. A team execution runs through the [`TeamOrchestrator`] in
/// `nexus-http`, which dispatches to the appropriate protocol handler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamDefinition {
    /// Unique identifier for this team.
    pub id: String,
    /// Human-readable team name.
    pub name: String,
    /// Description of what this team does.
    pub description: String,
    /// Team members with their roles and responsibilities.
    pub members: Vec<TeamMember>,
    /// How team members coordinate their work.
    pub coordination: CoordinationProtocol,
    /// Budget limits for the entire team.
    pub budget: TeamBudget,
    /// Human-in-the-loop configuration.
    pub hitl: HitlConfig,
    /// Criteria that determine when the team's work is complete.
    pub completion: CompletionCriteria,
}

/// A member of a team with a specific role.
///
/// Each member wraps an [`AgentDefinition`] with team-specific metadata:
/// delegation permissions, veto power, and explicit responsibilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    /// Unique member ID within the team.
    pub id: String,
    /// Role this member plays.
    pub role: TeamRole,
    /// The agent definition for this member.
    pub agent: AgentDefinition,
    /// What this member is responsible for.
    pub responsibilities: Vec<String>,
    /// IDs of other members this one can delegate tasks to.
    pub can_delegate_to: Vec<String>,
    /// Whether this member can veto team decisions.
    pub can_veto: bool,
}

/// Role a team member plays.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TeamRole {
    /// Team lead — makes final decisions, delegates work.
    Lead,
    /// Domain specialist — handles tasks in a specific area.
    Specialist {
        /// The domain this specialist covers.
        domain: String,
    },
    /// Code reviewer — reviews work from other members.
    Reviewer,
    /// Coordinator — manages workflow but doesn't produce artifacts.
    Coordinator,
    /// Observer — monitors progress, provides feedback.
    Observer,
    /// Custom role not covered by the built-in types.
    Custom {
        /// Role name.
        name: String,
        /// Role description.
        description: String,
    },
}

impl std::fmt::Display for TeamRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lead => write!(f, "Lead"),
            Self::Specialist { domain } => write!(f, "Specialist({})", domain),
            Self::Reviewer => write!(f, "Reviewer"),
            Self::Coordinator => write!(f, "Coordinator"),
            Self::Observer => write!(f, "Observer"),
            Self::Custom { name, .. } => write!(f, "Custom({})", name),
        }
    }
}

/// Protocol for coordinating work across team members.
///
/// Each protocol defines a different execution topology:
///
/// - **Hierarchical**: A lead decomposes and delegates, specialists execute, reviewers validate.
/// - **Parallel**: All specialists run concurrently, a coordinator merges results.
/// - **Sequential**: Members execute in order, passing output forward.
/// - **Consensus**: All members propose solutions, then vote on the best one.
/// - **Competitive**: Multiple candidates solve the same task, a judge picks the winner.
/// - **Swarm**: Self-organizing agents claim tasks from a shared board.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "protocol", rename_all = "snake_case")]
pub enum CoordinationProtocol {
    /// Top-down delegation from a lead.
    Hierarchical {
        /// ID of the team lead.
        lead_id: String,
    },
    /// All members work simultaneously, coordinator merges results.
    Parallel {
        /// ID of the coordinator who merges results.
        coordinator_id: String,
    },
    /// Members work in a defined order, passing results forward.
    Sequential {
        /// Ordered list of member IDs.
        order: Vec<String>,
    },
    /// Members vote on decisions, requiring a quorum.
    Consensus {
        /// Fraction of members needed to agree (0.0 to 1.0).
        quorum: f32,
        /// Maximum voting rounds before fallback.
        max_rounds: u32,
    },
    /// Multiple members attempt the same task, a judge picks the best.
    Competitive {
        /// ID of the judge who evaluates results.
        judge_id: String,
        /// IDs of the competing members.
        candidates: Vec<String>,
    },
    /// Self-organizing swarm with concurrent execution.
    Swarm {
        /// Maximum number of members working at once.
        max_concurrent: usize,
    },
}

/// Budget constraints for a team.
///
/// All limits are enforced by the orchestrator. When any limit is reached,
/// the team execution stops gracefully and returns partial results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamBudget {
    /// Maximum total cost in USD for the entire team execution.
    pub max_total_cost_usd: f32,
    /// Maximum cost in USD for any single member.
    pub max_cost_per_member_usd: f32,
    /// Maximum total tokens across all members.
    pub max_total_tokens: u64,
    /// Maximum wall-clock time in seconds.
    pub max_duration_secs: u64,
    /// Maximum iterations per member before forced stop.
    pub max_iterations_per_member: u32,
}

/// Human-in-the-loop configuration for a team.
///
/// Defines when the orchestrator should pause execution and wait for
/// human approval before proceeding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HitlConfig {
    /// Whether HITL is enabled for this team.
    pub enabled: bool,
    /// Checkpoints where the orchestrator will pause for human review.
    pub checkpoints: Vec<HitlCheckpoint>,
}

/// A checkpoint where the orchestrator pauses for human review.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "checkpoint", rename_all = "snake_case")]
pub enum HitlCheckpoint {
    /// Pause after the lead/coordinator produces a plan, before execution begins.
    AfterPlanning,
    /// Pause after each member completes their task, before passing results forward.
    AfterEachCompletion,
    /// Pause when cumulative spending crosses a percentage threshold.
    BudgetThreshold {
        /// The budget percentage (0.0 to 1.0) that triggers the pause.
        percent: f32,
    },
    /// Pause when team members disagree (e.g., conflicting reviews).
    OnConflict,
    /// Pause before committing final artifacts to the workspace.
    BeforeCommit,
    /// Never pause — fully autonomous execution.
    Never,
}

/// Criteria that determine when a team's work is complete.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionCriteria {
    /// All tasks on the workspace board must reach "Done" status.
    pub all_tasks_done: bool,
    /// The lead/coordinator must explicitly approve the final output.
    pub lead_approval_required: bool,
    /// All review tasks must be approved (no "RequestChanges" verdicts pending).
    pub all_reviews_passed: bool,
    /// A custom completion message pattern the lead must produce.
    pub completion_phrase: Option<String>,
}
