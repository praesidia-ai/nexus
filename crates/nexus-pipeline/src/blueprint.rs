use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blueprint {
    pub name: String,
    pub version: String,
    pub description: String,
    pub triggers: Vec<BlueprintTrigger>,
    pub steps: Vec<PipelineStep>,
    pub config: BlueprintConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlueprintTrigger {
    Manual,
    Schedule { cron: String },
    Event { channel: String, pattern: String },
    Webhook { path: String },
    GitPush { branch: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PipelineStep {
    Agent {
        name: String,
        model: Option<String>,
        system_prompt: String,
        tools: Vec<String>,
        max_iterations: u32,
        timeout_secs: u64,
    },
    Gate {
        name: String,
        checks: Vec<GateCheck>,
        on_failure: FailureAction,
    },
    Parallel {
        name: String,
        branches: Vec<PipelineStep>,
        merge_strategy: MergeStrategy,
    },
    Approval {
        name: String,
        approvers: Vec<String>,
        timeout_secs: u64,
    },
    Command {
        name: String,
        command: String,
        args: Vec<String>,
        timeout_secs: u64,
        working_dir: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GateCheck {
    Lint { command: String },
    TypeCheck { command: String },
    Test { command: String, selective: bool },
    SecurityScan { rules: Vec<String> },
    PolicyCheck { policy_id: String },
    Custom { command: String, expected_exit: i32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureAction {
    Abort,
    Retry { max_attempts: u32 },
    SelfHeal { max_rounds: u32 },
    Escalate { channel: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    WaitAll,
    FirstSuccess,
    Majority,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueprintConfig {
    pub max_total_duration_secs: u64,
    pub resource_budget: ResourceBudget,
    pub notifications: Vec<NotificationTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceBudget {
    pub max_llm_tokens: u64,
    pub max_tool_calls: u32,
    pub max_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotificationTarget {
    Slack { webhook_url: String },
    Email { address: String },
    Webhook { url: String },
}

impl Blueprint {
    pub fn from_toml(content: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(content)
    }

    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }
}
