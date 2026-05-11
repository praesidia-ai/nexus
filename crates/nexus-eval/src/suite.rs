//! Core evaluation types — suites, cases, inputs, expectations, and results.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::EvalError;

// ── Suite & Case definitions ────────────────────────────────────────────────

/// A complete evaluation suite containing multiple test cases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalSuite {
    /// Unique identifier for this suite.
    pub id: String,
    /// Human-readable name (e.g. "Generation Quality").
    pub name: String,
    /// Longer description of what this suite tests.
    pub description: String,
    /// The category of capabilities this suite evaluates.
    pub category: EvalCategory,
    /// Ordered list of test cases.
    pub cases: Vec<EvalCase>,
}

/// Category of evaluation — maps to a major Nexus capability area.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvalCategory {
    /// Full code / UI generation quality.
    Generation,
    /// How accurately the system classifies user intent.
    IntentAccuracy,
    /// Quality of architectural and design decisions.
    DecisionQuality,
    /// Quality of generated source code.
    CodeQuality,
    /// Efficiency of individual agent execution (iterations, cost).
    AgentEfficiency,
    /// How well multi-agent teams coordinate.
    TeamCoordination,
    /// Calibration of the taste engine (aesthetic scoring).
    TasteCalibration,
    /// Success rate of self-healing / repair loops.
    RepairSuccess,
}

/// A single evaluation case within a suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCase {
    /// Unique case identifier (unique within the suite).
    pub id: String,
    /// Short human-readable name for this case.
    pub name: String,
    /// The input to feed into the system under test.
    pub input: EvalInput,
    /// What the output should satisfy.
    pub expected: EvalExpectation,
    /// Relative weight of this case in the suite score (default 1.0).
    #[serde(default = "default_weight")]
    pub weight: f32,
    /// Maximum seconds this case may run before being killed.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_weight() -> f32 {
    1.0
}

fn default_timeout() -> u64 {
    60
}

// ── Inputs ──────────────────────────────────────────────────────────────────

/// The input fed to the system under test.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EvalInput {
    /// A natural-language description (e.g. "Build a todo app").
    Description {
        /// The description text.
        text: String,
    },
    /// A pair of (user utterance, expected intent label).
    IntentPair {
        /// The user utterance.
        utterance: String,
        /// The expected intent label.
        expected_intent: String,
    },
    /// A task assigned to a single agent.
    AgentTask {
        /// Which agent type to run.
        agent: String,
        /// The task description.
        task: String,
    },
    /// A task assigned to a multi-agent team.
    TeamTask {
        /// Team template name.
        team: String,
        /// The task description.
        task: String,
    },
    /// A taste judgment input (UI screenshot description or HTML snippet).
    TasteJudgment {
        /// The content to judge.
        content: String,
        /// Expected minimum taste score (0.0-1.0).
        expected_score: f32,
    },
}

// ── Expectations ────────────────────────────────────────────────────────────

/// What the output of a test case should satisfy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EvalExpectation {
    /// The generated project must compile / build without errors.
    Builds,
    /// The generated project must build AND pass runtime smoke tests.
    BuildsAndRuns,
    /// The taste score must meet or exceed a threshold.
    QualityThreshold {
        /// Minimum acceptable taste score (0.0-1.0).
        min_taste: f32,
    },
    /// Specific fields must match expected values.
    FieldMatch {
        /// List of (field_path, expected_value) checks.
        checks: Vec<FieldCheck>,
    },
    /// An LLM judge evaluates the output against criteria.
    LlmJudge {
        /// The criteria the judge should evaluate.
        criteria: Vec<String>,
    },
    /// Agent efficiency constraints.
    AgentEfficiency {
        /// Maximum allowed LLM iterations.
        max_iterations: u32,
        /// Maximum allowed cost in USD.
        max_cost: f64,
    },
}

/// A single field-level check for the `FieldMatch` expectation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldCheck {
    /// Dot-separated field path (e.g. "intent.category").
    pub field: String,
    /// Expected value (compared as string).
    pub expected: String,
}

// ── Results ─────────────────────────────────────────────────────────────────

/// Aggregated result for an entire suite run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuiteResult {
    /// The suite that was run.
    pub suite_id: String,
    /// When this run completed.
    pub timestamp: DateTime<Utc>,
    /// Weighted overall score (0.0-1.0).
    pub overall_score: f32,
    /// Per-case results.
    pub case_results: Vec<CaseResult>,
    /// Total LLM cost for the run in USD.
    pub cost_total: f64,
    /// Total wall-clock duration in milliseconds.
    pub duration_total_ms: u64,
}

/// Result of running a single eval case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseResult {
    /// The case that was run.
    pub case_id: String,
    /// Whether the case passed all expectations.
    pub passed: bool,
    /// Numeric score (0.0-1.0).
    pub score: f32,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
    /// Estimated cost in USD.
    pub cost_usd: f64,
    /// Error message if the case failed.
    pub error: Option<String>,
}

// ── Loaders ─────────────────────────────────────────────────────────────────

/// Load an [`EvalSuite`] from a JSON file on disk.
///
/// # Errors
///
/// Returns [`EvalError::Io`] if the file cannot be read, or
/// [`EvalError::Json`] if it cannot be parsed.
pub fn load_suite_from_json(path: &Path) -> Result<EvalSuite, EvalError> {
    let contents = std::fs::read_to_string(path)?;
    let suite: EvalSuite = serde_json::from_str(&contents)?;
    Ok(suite)
}

/// Create a new empty [`SuiteResult`] scaffold for a given suite.
pub fn empty_suite_result(suite_id: &str) -> SuiteResult {
    SuiteResult {
        suite_id: suite_id.to_string(),
        timestamp: Utc::now(),
        overall_score: 0.0,
        case_results: Vec::new(),
        cost_total: 0.0,
        duration_total_ms: 0,
    }
}

/// Generate a unique case ID.
pub fn new_case_id() -> String {
    Uuid::new_v4().to_string()
}
