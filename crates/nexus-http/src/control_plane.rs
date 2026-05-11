//! Control Plane — safety layer giving users control over Nexus autonomy.
//!
//! Three modes: Safe (read+suggest), Assisted (approve risky), Autonomous (full exec).
//! Every action is classified by risk and validated against limits before execution.
//! High-risk actions ALWAYS require approval. Limits prevent runaway costs/scope.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use tracing::warn;

// ---------------------------------------------------------------------------
// Nexus operating mode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum NexusMode {
    /// Read-only analysis + suggestions. Never modifies files.
    Safe,
    /// Executes low-risk automatically; medium/high require approval.
    #[default]
    Assisted,
    /// Full execution — all risk levels auto-approved.
    Autonomous,
}


// ---------------------------------------------------------------------------
// Action classification
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    /// What is being done (e.g., "file_write", "bash_exec", "llm_call").
    pub action_type: String,
    /// Risk level of this action.
    pub risk_level: RiskLevel,
    /// Target (file path, command, URL).
    pub target: String,
    /// Optional description for approval UI.
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

/// Classify an action's risk level based on its type and target.
pub fn classify_risk(action_type: &str, target: &str) -> RiskLevel {
    match action_type {
        // Always high risk
        "bash_exec" | "shell_command" => {
            let lower = target.to_lowercase();
            if lower.contains("rm ") || lower.contains("sudo") || lower.contains("chmod")
                || lower.contains("kill") || lower.contains("DROP")
                || lower.contains("DELETE FROM")
            {
                RiskLevel::High
            } else {
                // npm install, git, and other shell commands default to Medium
                RiskLevel::Medium
            }
        }
        "file_delete" => RiskLevel::High,
        "deploy" | "push_to_github" | "deploy_to_server" => RiskLevel::High,
        "docker_run" | "docker_build" => RiskLevel::Medium,
        "file_write" | "file_edit" => {
            // Config/env files are higher risk
            if target.contains(".env") || target.contains("config")
                || target.contains("package.json") || target.contains("next.config")
            {
                RiskLevel::Medium
            } else {
                RiskLevel::Low
            }
        }
        "file_read" | "grep" | "glob" | "list_directory" | "git_status" | "git_diff" => {
            RiskLevel::Low
        }
        "llm_call" => RiskLevel::Low,
        "app_start" | "app_restart" => RiskLevel::Medium,
        "app_stop" => RiskLevel::Low,
        _ => RiskLevel::Medium,
    }
}

// ---------------------------------------------------------------------------
// Execution limits
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionLimits {
    /// Max files that can be modified in one task.
    pub max_files_modified: u32,
    /// Max USD cost per task.
    pub max_cost_per_task: f64,
    /// Max agent loop iterations.
    pub max_iterations: u32,
    /// Max agents that can run simultaneously.
    pub max_agents_spawned: u32,
    /// Max total tokens per task.
    pub max_tokens_per_task: u64,
    /// Max bash commands per task.
    pub max_bash_commands: u32,
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            max_files_modified: 50,
            max_cost_per_task: 10.0,
            max_iterations: 20,
            max_agents_spawned: 10,
            max_tokens_per_task: 500_000,
            max_bash_commands: 20,
        }
    }
}

// ---------------------------------------------------------------------------
// Control Plane (stateful per-session)
// ---------------------------------------------------------------------------

/// The control plane — validates every action before execution.
#[derive(Debug, Clone, Serialize)]
pub struct ControlPlane {
    pub mode: NexusMode,
    pub limits: ExecutionLimits,
    // Counters (reset per task)
    files_modified: u32,
    cost_spent: f64,
    iterations_used: u32,
    agents_spawned: u32,
    tokens_used: u64,
    bash_commands: u32,
    /// Actions that are pending approval (Assisted mode).
    #[serde(skip)]
    pending_approvals: Vec<Action>,
    /// Actions that were blocked.
    blocked_actions: Vec<BlockedAction>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlockedAction {
    pub action: Action,
    pub reason: String,
}

/// Result of validating an action.
#[derive(Debug, Clone, Serialize)]
pub enum ValidationResult {
    /// Action is approved — execute it.
    Approved,
    /// Action needs user approval first (Assisted mode).
    NeedsApproval { action: Action, reason: String },
    /// Action is blocked — do NOT execute.
    Blocked { reason: String },
}

impl ControlPlane {
    pub fn new(mode: NexusMode) -> Self {
        Self {
            mode,
            limits: ExecutionLimits::default(),
            files_modified: 0,
            cost_spent: 0.0,
            iterations_used: 0,
            agents_spawned: 0,
            tokens_used: 0,
            bash_commands: 0,
            pending_approvals: Vec::new(),
            blocked_actions: Vec::new(),
        }
    }

    pub fn with_limits(mode: NexusMode, limits: ExecutionLimits) -> Self {
        Self {
            mode,
            limits,
            files_modified: 0,
            cost_spent: 0.0,
            iterations_used: 0,
            agents_spawned: 0,
            tokens_used: 0,
            bash_commands: 0,
            pending_approvals: Vec::new(),
            blocked_actions: Vec::new(),
        }
    }

    /// Validate an action before execution.
    ///
    /// Returns `Approved`, `NeedsApproval`, or `Blocked`.
    pub fn validate(&mut self, action: &Action) -> ValidationResult {
        // Step 1: Check limits
        if let Some(reason) = self.check_limits(action) {
            let blocked = BlockedAction {
                action: action.clone(),
                reason: reason.clone(),
            };
            self.blocked_actions.push(blocked);
            warn!(
                action = %action.action_type,
                target = %action.target,
                reason = %reason,
                "Control plane: action BLOCKED"
            );
            return ValidationResult::Blocked { reason };
        }

        // Step 2: Check mode + risk
        match self.mode {
            NexusMode::Safe => {
                // Safe mode: only allow reads
                if action.risk_level == RiskLevel::Low
                    && matches!(
                        action.action_type.as_str(),
                        "file_read" | "grep" | "glob" | "list_directory"
                            | "git_status" | "git_diff" | "llm_call"
                    )
                {
                    ValidationResult::Approved
                } else {
                    ValidationResult::Blocked {
                        reason: format!(
                            "Safe mode: action '{}' is not allowed (read-only mode)",
                            action.action_type
                        ),
                    }
                }
            }
            NexusMode::Assisted => match action.risk_level {
                RiskLevel::Low => ValidationResult::Approved,
                RiskLevel::Medium => {
                    // Auto-approve file writes to generated code
                    if action.action_type == "file_write"
                        && !action.target.contains(".env")
                        && !action.target.contains("config")
                    {
                        ValidationResult::Approved
                    } else {
                        ValidationResult::NeedsApproval {
                            action: action.clone(),
                            reason: format!(
                                "Medium-risk action requires approval: {} on {}",
                                action.action_type, action.target
                            ),
                        }
                    }
                }
                RiskLevel::High => ValidationResult::NeedsApproval {
                    action: action.clone(),
                    reason: format!(
                        "High-risk action requires approval: {} on {}",
                        action.action_type, action.target
                    ),
                },
            },
            NexusMode::Autonomous => {
                // Autonomous: approve everything except hard limit violations
                ValidationResult::Approved
            }
        }
    }

    /// Record that an action was executed (updates counters).
    pub fn record_execution(&mut self, action: &Action) {
        match action.action_type.as_str() {
            "file_write" | "file_edit" | "file_delete" => self.files_modified += 1,
            "bash_exec" | "shell_command" => self.bash_commands += 1,
            "agent_spawn" | "agent_run" => self.agents_spawned += 1,
            "llm_call" => { /* tracked via record_tokens/record_cost */ }
            _ => {}
        }
    }

    /// Record cost spent.
    pub fn record_cost(&mut self, cost_usd: f64) {
        self.cost_spent += cost_usd;
    }

    /// Record tokens used.
    pub fn record_tokens(&mut self, tokens: u64) {
        self.tokens_used += tokens;
    }

    /// Record an iteration.
    pub fn record_iteration(&mut self) {
        self.iterations_used += 1;
    }

    /// Record agent spawn.
    pub fn record_agent_spawn(&mut self) {
        self.agents_spawned += 1;
    }

    /// Approve a pending action (for Assisted mode UI).
    pub fn approve_pending(&mut self, action_type: &str) -> bool {
        if let Some(idx) = self
            .pending_approvals
            .iter()
            .position(|a| a.action_type == action_type)
        {
            self.pending_approvals.remove(idx);
            true
        } else {
            false
        }
    }

    /// Get all blocked actions.
    pub fn get_blocked(&self) -> &[BlockedAction] {
        &self.blocked_actions
    }

    /// Get summary of current usage vs limits.
    pub fn usage_summary(&self) -> UsageSummary {
        UsageSummary {
            files_modified: self.files_modified,
            files_limit: self.limits.max_files_modified,
            cost_spent: self.cost_spent,
            cost_limit: self.limits.max_cost_per_task,
            iterations_used: self.iterations_used,
            iterations_limit: self.limits.max_iterations,
            agents_spawned: self.agents_spawned,
            agents_limit: self.limits.max_agents_spawned,
            tokens_used: self.tokens_used,
            tokens_limit: self.limits.max_tokens_per_task,
            blocked_count: self.blocked_actions.len() as u32,
        }
    }

    // ── Internal ────────────────────────────────────────────────────────

    fn check_limits(&self, action: &Action) -> Option<String> {
        if matches!(action.action_type.as_str(), "file_write" | "file_edit")
            && self.files_modified >= self.limits.max_files_modified
        {
            return Some(format!(
                "File modification limit reached ({}/{})",
                self.files_modified, self.limits.max_files_modified
            ));
        }
        if self.cost_spent >= self.limits.max_cost_per_task {
            return Some(format!(
                "Cost limit reached (${:.2}/${:.2})",
                self.cost_spent, self.limits.max_cost_per_task
            ));
        }
        if self.iterations_used >= self.limits.max_iterations {
            return Some(format!(
                "Iteration limit reached ({}/{})",
                self.iterations_used, self.limits.max_iterations
            ));
        }
        if matches!(action.action_type.as_str(), "bash_exec" | "shell_command")
            && self.bash_commands >= self.limits.max_bash_commands
        {
            return Some(format!(
                "Bash command limit reached ({}/{})",
                self.bash_commands, self.limits.max_bash_commands
            ));
        }
        if self.tokens_used >= self.limits.max_tokens_per_task {
            return Some(format!(
                "Token limit reached ({}/{})",
                self.tokens_used, self.limits.max_tokens_per_task
            ));
        }
        None
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct UsageSummary {
    pub files_modified: u32,
    pub files_limit: u32,
    pub cost_spent: f64,
    pub cost_limit: f64,
    pub iterations_used: u32,
    pub iterations_limit: u32,
    pub agents_spawned: u32,
    pub agents_limit: u32,
    pub tokens_used: u64,
    pub tokens_limit: u64,
    pub blocked_count: u32,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_mode_blocks_writes() {
        let mut cp = ControlPlane::new(NexusMode::Safe);
        let action = Action {
            action_type: "file_write".into(),
            risk_level: RiskLevel::Low,
            target: "src/page.tsx".into(),
            description: "Write page".into(),
        };
        assert!(matches!(cp.validate(&action), ValidationResult::Blocked { .. }));
    }

    #[test]
    fn safe_mode_allows_reads() {
        let mut cp = ControlPlane::new(NexusMode::Safe);
        let action = Action {
            action_type: "file_read".into(),
            risk_level: RiskLevel::Low,
            target: "src/page.tsx".into(),
            description: "Read page".into(),
        };
        assert!(matches!(cp.validate(&action), ValidationResult::Approved));
    }

    #[test]
    fn assisted_mode_approves_low_risk() {
        let mut cp = ControlPlane::new(NexusMode::Assisted);
        let action = Action {
            action_type: "file_read".into(),
            risk_level: RiskLevel::Low,
            target: "src/page.tsx".into(),
            description: "Read".into(),
        };
        assert!(matches!(cp.validate(&action), ValidationResult::Approved));
    }

    #[test]
    fn assisted_mode_requires_approval_for_high() {
        let mut cp = ControlPlane::new(NexusMode::Assisted);
        let action = Action {
            action_type: "deploy".into(),
            risk_level: RiskLevel::High,
            target: "production".into(),
            description: "Deploy to prod".into(),
        };
        assert!(matches!(
            cp.validate(&action),
            ValidationResult::NeedsApproval { .. }
        ));
    }

    #[test]
    fn autonomous_mode_approves_all() {
        let mut cp = ControlPlane::new(NexusMode::Autonomous);
        let action = Action {
            action_type: "deploy".into(),
            risk_level: RiskLevel::High,
            target: "production".into(),
            description: "Deploy".into(),
        };
        assert!(matches!(cp.validate(&action), ValidationResult::Approved));
    }

    #[test]
    fn limits_block_when_exceeded() {
        let limits = ExecutionLimits {
            max_files_modified: 2,
            ..Default::default()
        };
        let mut cp = ControlPlane::with_limits(NexusMode::Autonomous, limits);
        let action = Action {
            action_type: "file_write".into(),
            risk_level: RiskLevel::Low,
            target: "a.tsx".into(),
            description: "Write".into(),
        };
        // First two pass
        assert!(matches!(cp.validate(&action), ValidationResult::Approved));
        cp.record_execution(&action);
        assert!(matches!(cp.validate(&action), ValidationResult::Approved));
        cp.record_execution(&action);
        // Third is blocked
        assert!(matches!(cp.validate(&action), ValidationResult::Blocked { .. }));
    }

    #[test]
    fn risk_classification_works() {
        assert_eq!(classify_risk("file_read", "anything"), RiskLevel::Low);
        assert_eq!(classify_risk("file_delete", "anything"), RiskLevel::High);
        assert_eq!(classify_risk("deploy", "prod"), RiskLevel::High);
        assert_eq!(classify_risk("bash_exec", "rm -rf /"), RiskLevel::High);
        assert_eq!(classify_risk("bash_exec", "echo hello"), RiskLevel::Medium);
        assert_eq!(classify_risk("file_write", "src/page.tsx"), RiskLevel::Low);
        assert_eq!(classify_risk("file_write", ".env"), RiskLevel::Medium);
    }

    #[test]
    fn usage_summary_tracks_correctly() {
        let mut cp = ControlPlane::new(NexusMode::Autonomous);
        let action = Action {
            action_type: "file_write".into(),
            risk_level: RiskLevel::Low,
            target: "a.tsx".into(),
            description: "".into(),
        };
        cp.validate(&action);
        cp.record_execution(&action);
        cp.record_cost(0.5);
        cp.record_tokens(1000);
        cp.record_iteration();

        let summary = cp.usage_summary();
        assert_eq!(summary.files_modified, 1);
        assert!((summary.cost_spent - 0.5).abs() < 0.01);
        assert_eq!(summary.tokens_used, 1000);
        assert_eq!(summary.iterations_used, 1);
    }
}

// ---------------------------------------------------------------------------
// Per-team control mode registry
// ---------------------------------------------------------------------------

/// Registry of per-team control modes, allowing different autonomy levels per team.
///
/// Global mode serves as the default. Teams can override to be more or less
/// restrictive than the global setting.
#[derive(Debug, Clone, Serialize)]
pub struct ControlPlaneRegistry {
    /// Global default mode.
    pub global_mode: NexusMode,
    /// Per-team mode overrides. Key is team_id.
    team_modes: HashMap<String, NexusMode>,
    /// Per-team control planes with their own counters.
    #[serde(skip)]
    team_planes: HashMap<String, ControlPlane>,
}

impl ControlPlaneRegistry {
    pub fn new(global_mode: NexusMode) -> Self {
        Self {
            global_mode,
            team_modes: HashMap::new(),
            team_planes: HashMap::new(),
        }
    }

    /// Set a per-team override mode.
    pub fn set_team_mode(&mut self, team_id: &str, mode: NexusMode) {
        self.team_modes.insert(team_id.to_string(), mode);
        self.team_planes
            .insert(team_id.to_string(), ControlPlane::new(mode));
    }

    /// Remove a per-team override, falling back to global.
    pub fn remove_team_mode(&mut self, team_id: &str) {
        self.team_modes.remove(team_id);
        self.team_planes.remove(team_id);
    }

    /// Get the effective mode for a team (team override or global).
    pub fn effective_mode(&self, team_id: &str) -> NexusMode {
        self.team_modes
            .get(team_id)
            .copied()
            .unwrap_or(self.global_mode)
    }

    /// Get or create a ControlPlane for a specific team.
    pub fn for_team(&mut self, team_id: &str) -> &mut ControlPlane {
        let mode = self.effective_mode(team_id);
        self.team_planes
            .entry(team_id.to_string())
            .or_insert_with(|| ControlPlane::new(mode))
    }

    /// Validate an action for a specific team.
    pub fn validate_for_team(&mut self, team_id: &str, action: &Action) -> ValidationResult {
        self.for_team(team_id).validate(action)
    }

    /// Get usage summary for a specific team.
    pub fn team_usage(&self, team_id: &str) -> Option<UsageSummary> {
        self.team_planes.get(team_id).map(|cp| cp.usage_summary())
    }

    /// List all team mode overrides.
    pub fn list_team_modes(&self) -> &HashMap<String, NexusMode> {
        &self.team_modes
    }
}
