//! Shared types for the Super Agents performance optimization system.

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Agent Identity
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuperAgentKind {
    LatencyOptimizer,
    ConcurrencyOptimizer,
    LlmCostOptimizer,
    ContextCompressor,
    PipelineBottleneckDetector,
    CacheOptimizer,
    DatabaseOptimizer,
    SseStreamOptimizer,
    AgentEfficiencyOptimizer,
    BuildRuntimeOptimizer,
}

impl SuperAgentKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LatencyOptimizer => "latency_optimizer",
            Self::ConcurrencyOptimizer => "concurrency_optimizer",
            Self::LlmCostOptimizer => "llm_cost_optimizer",
            Self::ContextCompressor => "context_compressor",
            Self::PipelineBottleneckDetector => "pipeline_bottleneck_detector",
            Self::CacheOptimizer => "cache_optimizer",
            Self::DatabaseOptimizer => "database_optimizer",
            Self::SseStreamOptimizer => "sse_stream_optimizer",
            Self::AgentEfficiencyOptimizer => "agent_efficiency_optimizer",
            Self::BuildRuntimeOptimizer => "build_runtime_optimizer",
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::LatencyOptimizer,
            Self::ConcurrencyOptimizer,
            Self::LlmCostOptimizer,
            Self::ContextCompressor,
            Self::PipelineBottleneckDetector,
            Self::CacheOptimizer,
            Self::DatabaseOptimizer,
            Self::SseStreamOptimizer,
            Self::AgentEfficiencyOptimizer,
            Self::BuildRuntimeOptimizer,
        ]
    }
}

// ---------------------------------------------------------------------------
// Trigger Modes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerMode {
    /// Runs on a fixed interval.
    Periodic { interval: Duration },
    /// Runs when a specific metric crosses a threshold.
    Threshold { metric: String, value: f64 },
    /// Runs after a specific system event.
    Event { event_name: String },
    /// Runs on schedule + threshold (whichever fires first).
    Hybrid {
        interval: Duration,
        metric: String,
        value: f64,
    },
}

// ---------------------------------------------------------------------------
// Severity / Priority
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Info = 0,
    Low = 1,
    Medium = 2,
    High = 3,
    Critical = 4,
}

// ---------------------------------------------------------------------------
// Analysis Report — produced by each agent after scanning
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub severity: Severity,
    pub category: String,
    pub description: String,
    pub metric_before: f64,
    pub suggested_action: String,
    pub estimated_improvement_pct: f64,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisReport {
    pub agent: String,
    pub agent_kind: SuperAgentKind,
    pub timestamp: String,
    pub findings: Vec<Finding>,
    pub scan_duration_ms: u64,
    pub system_snapshot: SystemSnapshot,
}

impl AnalysisReport {
    pub fn highest_severity(&self) -> Option<Severity> {
        self.findings.iter().map(|f| f.severity).max()
    }

    pub fn actionable_findings(&self) -> Vec<&Finding> {
        self.findings
            .iter()
            .filter(|f| f.severity >= Severity::Medium)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Optimization Result — produced after applying changes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Optimization {
    pub finding_id: String,
    pub action_taken: String,
    pub metric_before: f64,
    pub metric_after: f64,
    pub improvement_pct: f64,
    pub rollback_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationResult {
    pub agent: String,
    pub agent_kind: SuperAgentKind,
    pub timestamp: String,
    pub optimizations: Vec<Optimization>,
    pub total_improvement_pct: f64,
    pub duration_ms: u64,
    pub requires_restart: bool,
}

// ---------------------------------------------------------------------------
// Validation Result
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationOutcome {
    pub passed: bool,
    pub checks_run: u32,
    pub checks_passed: u32,
    pub failures: Vec<String>,
}

// ---------------------------------------------------------------------------
// System Snapshot — point-in-time view of key metrics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemSnapshot {
    pub pipeline_avg_latency_ms: f64,
    pub pipeline_p95_latency_ms: f64,
    pub pipeline_error_rate: f64,
    pub llm_daily_cost_usd: f64,
    pub llm_avg_latency_ms: f64,
    pub llm_total_tokens_today: u64,
    pub cache_hit_rate: f64,
    pub cache_size_entries: u64,
    pub db_avg_query_ms: f64,
    pub db_lock_contention_pct: f64,
    pub sse_active_streams: u64,
    pub sse_avg_first_byte_ms: f64,
    pub agent_loop_avg_iterations: f64,
    pub agent_loop_avg_duration_ms: f64,
    pub active_projects: u64,
    pub memory_used_mb: f64,
    pub uptime_secs: u64,
}

// ---------------------------------------------------------------------------
// Orchestrator types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum OrchestratorMode {
    /// Agents analyze but never auto-apply changes.
    Observe,
    /// Agents analyze and apply LOW-risk optimizations.
    #[default]
    Conservative,
    /// Agents apply MEDIUM-risk optimizations.
    Balanced,
    /// Agents apply all optimizations (use with caution).
    Aggressive,
}


/// Resource conflict groups — agents in the same group cannot run simultaneously.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConflictGroup {
    Pipeline,
    LlmCalls,
    Database,
    Cache,
    Network,
    Runtime,
}

impl SuperAgentKind {
    pub fn conflict_groups(&self) -> &'static [ConflictGroup] {
        match self {
            Self::LatencyOptimizer => &[ConflictGroup::Pipeline],
            Self::ConcurrencyOptimizer => &[ConflictGroup::Pipeline, ConflictGroup::Runtime],
            Self::LlmCostOptimizer => &[ConflictGroup::LlmCalls],
            Self::ContextCompressor => &[ConflictGroup::LlmCalls],
            Self::PipelineBottleneckDetector => &[ConflictGroup::Pipeline],
            Self::CacheOptimizer => &[ConflictGroup::Cache],
            Self::DatabaseOptimizer => &[ConflictGroup::Database],
            Self::SseStreamOptimizer => &[ConflictGroup::Network],
            Self::AgentEfficiencyOptimizer => &[ConflictGroup::LlmCalls, ConflictGroup::Pipeline],
            Self::BuildRuntimeOptimizer => &[ConflictGroup::Runtime],
        }
    }

    pub fn default_priority(&self) -> u8 {
        match self {
            Self::PipelineBottleneckDetector => 200,
            Self::LatencyOptimizer => 180,
            Self::LlmCostOptimizer => 170,
            Self::AgentEfficiencyOptimizer => 160,
            Self::ContextCompressor => 150,
            Self::CacheOptimizer => 140,
            Self::DatabaseOptimizer => 130,
            Self::ConcurrencyOptimizer => 120,
            Self::SseStreamOptimizer => 110,
            Self::BuildRuntimeOptimizer => 100,
        }
    }
}

// ---------------------------------------------------------------------------
// Agent run record (for history)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRunRecord {
    pub id: String,
    pub agent_kind: SuperAgentKind,
    pub started_at: String,
    pub finished_at: String,
    pub mode: OrchestratorMode,
    pub findings_count: u32,
    pub optimizations_applied: u32,
    pub total_improvement_pct: f64,
    pub validation_passed: bool,
    pub error: Option<String>,
}
