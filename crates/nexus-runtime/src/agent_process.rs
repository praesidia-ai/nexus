use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExecContext {
    pub process_id: String,
    pub agent_id: String,
    pub project_id: String,
    pub status: ProcessStatus,
    pub budget: ResourceBudget,
    pub usage: ResourceUsage,
    pub memory_scope: MemoryScope,
    pub permissions: AgentPermissions,
    pub started_at: DateTime<Utc>,
    pub last_checkpoint: Option<DateTime<Utc>>,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessStatus {
    Queued,
    Running,
    Paused,
    Checkpointed,
    Completed,
    Failed { reason: String },
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceBudget {
    pub max_llm_tokens: u64,
    pub max_tool_calls: u32,
    pub max_duration_secs: u64,
    pub max_cost_usd: f64,
    pub max_memory_mb: u64,
}

impl Default for ResourceBudget {
    fn default() -> Self {
        Self {
            max_llm_tokens: 100_000,
            max_tool_calls: 50,
            max_duration_secs: 300,
            max_cost_usd: 1.0,
            max_memory_mb: 512,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceUsage {
    pub llm_tokens: u64,
    pub tool_calls: u32,
    pub elapsed_secs: u64,
    pub cost_usd: f64,
    pub memory_mb: u64,
}

impl ResourceUsage {
    pub fn exceeds(&self, budget: &ResourceBudget) -> Option<String> {
        if self.llm_tokens > budget.max_llm_tokens {
            return Some(format!(
                "LLM token budget exceeded: {}/{}",
                self.llm_tokens, budget.max_llm_tokens
            ));
        }
        if self.tool_calls > budget.max_tool_calls {
            return Some(format!(
                "Tool call budget exceeded: {}/{}",
                self.tool_calls, budget.max_tool_calls
            ));
        }
        if self.elapsed_secs > budget.max_duration_secs {
            return Some(format!(
                "Duration budget exceeded: {}s/{}s",
                self.elapsed_secs, budget.max_duration_secs
            ));
        }
        if self.cost_usd > budget.max_cost_usd {
            return Some(format!(
                "Cost budget exceeded: ${:.2}/${:.2}",
                self.cost_usd, budget.max_cost_usd
            ));
        }
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryScope {
    pub session_id: String,
    pub shared_keys: Vec<String>,
    pub isolation_level: IsolationLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum IsolationLevel {
    #[default]
    Full,
    SharedRead,
    SharedWrite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPermissions {
    pub can_read_files: bool,
    pub can_write_files: bool,
    pub can_execute_commands: bool,
    pub can_access_network: bool,
    pub can_spawn_children: bool,
    pub allowed_tools: Vec<String>,
    pub denied_tools: Vec<String>,
    pub allowed_paths: Vec<String>,
}

impl Default for AgentPermissions {
    fn default() -> Self {
        Self {
            can_read_files: true,
            can_write_files: false,
            can_execute_commands: false,
            can_access_network: false,
            can_spawn_children: false,
            allowed_tools: Vec::new(),
            denied_tools: Vec::new(),
            allowed_paths: Vec::new(),
        }
    }
}

impl AgentExecContext {
    pub fn new(agent_id: &str, project_id: &str) -> Self {
        Self {
            process_id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.to_string(),
            project_id: project_id.to_string(),
            status: ProcessStatus::Queued,
            budget: ResourceBudget::default(),
            usage: ResourceUsage::default(),
            memory_scope: MemoryScope {
                session_id: uuid::Uuid::new_v4().to_string(),
                ..Default::default()
            },
            permissions: AgentPermissions::default(),
            started_at: Utc::now(),
            last_checkpoint: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_budget(mut self, budget: ResourceBudget) -> Self {
        self.budget = budget;
        self
    }

    pub fn with_permissions(mut self, permissions: AgentPermissions) -> Self {
        self.permissions = permissions;
        self
    }

    pub fn record_usage(&mut self, tokens: u64, tool_calls: u32, cost: f64) {
        self.usage.llm_tokens += tokens;
        self.usage.tool_calls += tool_calls;
        self.usage.cost_usd += cost;
    }

    pub fn check_budget(&self) -> Option<String> {
        self.usage.exceeds(&self.budget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_context() {
        let ctx = AgentExecContext::new("agent-1", "project-1");
        assert_eq!(ctx.agent_id, "agent-1");
        assert_eq!(ctx.project_id, "project-1");
        assert_eq!(ctx.status, ProcessStatus::Queued);
        assert!(!ctx.process_id.is_empty());
        assert!(!ctx.memory_scope.session_id.is_empty());
    }

    #[test]
    fn test_with_budget() {
        let budget = ResourceBudget {
            max_llm_tokens: 500,
            max_tool_calls: 10,
            max_duration_secs: 60,
            max_cost_usd: 0.5,
            max_memory_mb: 256,
        };
        let ctx = AgentExecContext::new("a", "p").with_budget(budget.clone());
        assert_eq!(ctx.budget.max_llm_tokens, 500);
        assert_eq!(ctx.budget.max_tool_calls, 10);
    }

    #[test]
    fn test_with_permissions() {
        let perms = AgentPermissions {
            can_write_files: true,
            can_execute_commands: true,
            ..Default::default()
        };
        let ctx = AgentExecContext::new("a", "p").with_permissions(perms);
        assert!(ctx.permissions.can_write_files);
        assert!(ctx.permissions.can_execute_commands);
        assert!(!ctx.permissions.can_access_network);
    }

    #[test]
    fn test_record_usage_accumulates() {
        let mut ctx = AgentExecContext::new("a", "p");
        ctx.record_usage(100, 2, 0.01);
        ctx.record_usage(200, 3, 0.02);
        assert_eq!(ctx.usage.llm_tokens, 300);
        assert_eq!(ctx.usage.tool_calls, 5);
        assert!((ctx.usage.cost_usd - 0.03).abs() < f64::EPSILON);
    }

    #[test]
    fn test_budget_within_limits() {
        let ctx = AgentExecContext::new("a", "p");
        assert!(ctx.check_budget().is_none());
    }

    #[test]
    fn test_budget_token_exceeded() {
        let budget = ResourceBudget {
            max_llm_tokens: 100,
            ..Default::default()
        };
        let mut ctx = AgentExecContext::new("a", "p").with_budget(budget);
        ctx.record_usage(101, 0, 0.0);
        let msg = ctx.check_budget();
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("LLM token budget exceeded"));
    }

    #[test]
    fn test_budget_tool_calls_exceeded() {
        let budget = ResourceBudget {
            max_tool_calls: 5,
            ..Default::default()
        };
        let mut ctx = AgentExecContext::new("a", "p").with_budget(budget);
        ctx.record_usage(0, 6, 0.0);
        let msg = ctx.check_budget();
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("Tool call budget exceeded"));
    }

    #[test]
    fn test_budget_cost_exceeded() {
        let budget = ResourceBudget {
            max_cost_usd: 0.50,
            ..Default::default()
        };
        let mut ctx = AgentExecContext::new("a", "p").with_budget(budget);
        ctx.record_usage(0, 0, 0.51);
        let msg = ctx.check_budget();
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("Cost budget exceeded"));
    }

    #[test]
    fn test_budget_duration_exceeded() {
        let budget = ResourceBudget {
            max_duration_secs: 60,
            ..Default::default()
        };
        let ctx = AgentExecContext::new("a", "p").with_budget(budget);
        let mut usage = ctx.usage.clone();
        usage.elapsed_secs = 61;
        let msg = usage.exceeds(&ctx.budget);
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("Duration budget exceeded"));
    }

    #[test]
    fn test_default_permissions_restrictive() {
        let perms = AgentPermissions::default();
        assert!(perms.can_read_files);
        assert!(!perms.can_write_files);
        assert!(!perms.can_execute_commands);
        assert!(!perms.can_access_network);
        assert!(!perms.can_spawn_children);
    }

    #[test]
    fn test_isolation_level_default() {
        let scope = MemoryScope::default();
        assert_eq!(scope.isolation_level, IsolationLevel::Full);
    }

    #[test]
    fn test_process_status_serde() {
        let status = ProcessStatus::Failed {
            reason: "out of memory".into(),
        };
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: ProcessStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, deserialized);
    }
}
