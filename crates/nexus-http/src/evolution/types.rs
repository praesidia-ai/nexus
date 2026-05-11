//! Types for the self-evolution engine.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Execution Record — what happened during a coding task
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRecord {
    pub id: String,
    pub task_description: String,
    pub project_id: String,
    pub timestamp: String,
    pub duration_ms: u64,
    pub outcome: ExecutionOutcome,
    pub agent_phases: Vec<PhaseRecord>,
    pub metrics: ExecutionMetrics,
    pub patterns_detected: Vec<DetectedPattern>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionOutcome {
    Success,
    PartialSuccess,
    Failure,
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseRecord {
    pub phase: String,
    pub agent: String,
    pub iterations: u32,
    pub tools_used: Vec<String>,
    pub files_touched: Vec<String>,
    pub duration_ms: u64,
    pub errors: Vec<String>,
    pub success: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    pub total_llm_calls: u32,
    pub total_tokens_used: u64,
    pub total_tool_calls: u32,
    pub total_files_changed: u32,
    pub total_retries: u32,
    pub verification_attempts: u32,
    pub verification_passed: bool,
    pub estimated_cost_usd: f64,
}

// ---------------------------------------------------------------------------
// Detected Patterns — what the system learned
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedPattern {
    pub id: String,
    pub pattern_type: PatternType,
    pub description: String,
    pub confidence: f64,
    pub occurrences: u32,
    pub context: HashMap<String, String>,
    pub first_seen: String,
    pub last_seen: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternType {
    SuccessfulStrategy,
    FailurePattern,
    OptimalToolSequence,
    CommonErrorFix,
    EffectivePromptPattern,
    ProjectConvention,
    FileStructurePattern,
}

// ---------------------------------------------------------------------------
// Improvement — an optimization to apply
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Improvement {
    pub id: String,
    pub improvement_type: ImprovementType,
    pub description: String,
    pub version: u32,
    pub confidence: f64,
    pub impact_estimate: f64,
    pub applied: bool,
    pub created_at: String,
    pub evidence: Vec<String>,
    pub rollback_data: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImprovementType {
    PromptOptimization,
    ToolSequenceOptimization,
    IterationLimitTuning,
    ModelRouting,
    ErrorRecoveryStrategy,
    ContextCompression,
    VerificationStrategy,
}

// ---------------------------------------------------------------------------
// Evolution State — the persistent state of the evolution engine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionState {
    pub version: u32,
    pub total_executions: u64,
    pub success_rate: f64,
    pub avg_duration_ms: f64,
    pub avg_iterations: f64,
    pub avg_cost_usd: f64,
    pub patterns: Vec<DetectedPattern>,
    pub improvements: Vec<Improvement>,
    pub prompt_overrides: HashMap<String, String>,
    pub config_overrides: HashMap<String, serde_json::Value>,
    pub last_evolution_at: String,
}

impl Default for EvolutionState {
    fn default() -> Self {
        Self {
            version: 1,
            total_executions: 0,
            success_rate: 0.0,
            avg_duration_ms: 0.0,
            avg_iterations: 0.0,
            avg_cost_usd: 0.0,
            patterns: Vec::new(),
            improvements: Vec::new(),
            prompt_overrides: HashMap::new(),
            config_overrides: HashMap::new(),
            last_evolution_at: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Evolution Event — streamed during evolution runs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum EvolutionEvent {
    #[serde(rename = "pattern_detected")]
    PatternDetected { pattern: DetectedPattern },
    #[serde(rename = "improvement_proposed")]
    ImprovementProposed { improvement: Improvement },
    #[serde(rename = "improvement_applied")]
    ImprovementApplied {
        improvement_id: String,
        success: bool,
    },
    #[serde(rename = "metrics_updated")]
    MetricsUpdated {
        success_rate: f64,
        avg_duration_ms: f64,
        avg_cost_usd: f64,
    },
    #[serde(rename = "evolution_complete")]
    EvolutionComplete {
        version: u32,
        improvements_applied: u32,
        patterns_learned: u32,
    },
}
