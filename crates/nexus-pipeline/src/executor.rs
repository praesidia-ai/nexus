use crate::blueprint::ResourceBudget;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Running,
    Passed,
    Failed { reason: String },
    Skipped,
    WaitingApproval,
    SelfHealing { round: u32, max: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_name: String,
    pub status: StepStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub output: serde_json::Value,
    pub metrics: StepMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StepMetrics {
    pub duration_ms: u64,
    pub llm_tokens_used: u64,
    pub tool_calls: u32,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineRun {
    pub id: String,
    pub blueprint_name: String,
    pub status: PipelineStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub step_results: Vec<StepResult>,
    pub total_metrics: StepMetrics,
    pub context: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStatus {
    Running,
    Completed,
    Failed,
    Aborted,
    WaitingApproval,
}

impl PipelineRun {
    pub fn new(blueprint_name: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            blueprint_name: blueprint_name.to_string(),
            status: PipelineStatus::Running,
            started_at: Utc::now(),
            finished_at: None,
            step_results: Vec::new(),
            total_metrics: StepMetrics::default(),
            context: HashMap::new(),
        }
    }

    pub fn record_step(&mut self, result: StepResult) {
        self.total_metrics.duration_ms += result.metrics.duration_ms;
        self.total_metrics.llm_tokens_used += result.metrics.llm_tokens_used;
        self.total_metrics.tool_calls += result.metrics.tool_calls;
        self.total_metrics.cost_usd += result.metrics.cost_usd;
        self.step_results.push(result);
    }

    pub fn complete(&mut self, status: PipelineStatus) {
        self.status = status;
        self.finished_at = Some(Utc::now());
    }

    pub fn is_budget_exceeded(&self, budget: &ResourceBudget) -> bool {
        self.total_metrics.llm_tokens_used > budget.max_llm_tokens
            || self.total_metrics.tool_calls > budget.max_tool_calls
            || self.total_metrics.cost_usd > budget.max_cost_usd
    }
}
