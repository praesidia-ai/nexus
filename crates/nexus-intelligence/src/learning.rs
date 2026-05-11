//! Adaptive learning memory.
//!
//! The learning layer records signals from every generation — stack decisions,
//! quality scores, build outcomes, user overrides — and uses them to improve
//! future decisions. This creates a feedback loop where Nexus gets better
//! with every project it generates.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A learning signal recorded after a decision or outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningSignal {
    /// Unique signal identifier.
    pub id: String,
    /// Tenant that generated this signal.
    pub tenant_id: String,
    /// Project associated with this signal, if any.
    pub project_id: Option<String>,
    /// What type of signal this is.
    pub signal_type: SignalType,
    /// Additional context about the signal.
    pub context: serde_json::Value,
    /// The outcome observed.
    pub outcome: serde_json::Value,
    /// Quality score (0.0 to 1.0) of the outcome.
    pub quality: f32,
    /// ISO 8601 timestamp of when the signal was recorded.
    pub timestamp: String,
}

/// The type of learning signal recorded.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignalType {
    /// A stack/technology decision was made.
    StackDecision {
        /// Decision area (e.g., "frontend_framework").
        area: String,
        /// What was chosen.
        choice: String,
    },
    /// A taste/quality score was assigned.
    TasteScore {
        /// The score (0.0 to 1.0).
        score: f32,
    },
    /// An outcome guarantee cycle completed.
    GuaranteeResult {
        /// Whether all checks passed.
        passed: bool,
        /// Number of fix cycles used.
        cycles: u32,
    },
    /// A build was attempted.
    BuildSuccess {
        /// Whether it succeeded on the first try.
        first_try: bool,
    },
    /// User manually overrode a decision.
    UserOverride {
        /// What was overridden.
        what: String,
        /// Original value.
        from: String,
        /// New value.
        to: String,
    },
    /// User provided explicit feedback.
    UserFeedback {
        /// Sentiment score (-1.0 to 1.0).
        sentiment: f32,
        /// Context of the feedback.
        context: String,
    },
}

/// A preference learned from historical signals.
#[derive(Debug, Clone, Serialize)]
pub struct LearnedPreference {
    /// The preferred choice.
    pub choice: String,
    /// Confidence in this preference (0.0 to 1.0).
    pub confidence: f32,
    /// Number of signals this preference is based on.
    pub sample_size: usize,
    /// Average quality score for this choice.
    pub avg_quality: f32,
    /// Human-readable explanation.
    pub explanation: String,
}

/// Aggregated learning insights for a tenant.
#[derive(Debug, Clone, Serialize)]
pub struct LearningInsights {
    /// Top-performing technology stacks.
    pub top_stacks: Vec<StackInsight>,
    /// Common failure patterns.
    pub common_failures: Vec<FailurePattern>,
    /// Total number of learning signals recorded.
    pub total_signals: usize,
}

/// Insight about a technology stack choice.
#[derive(Debug, Clone, Serialize)]
pub struct StackInsight {
    /// Decision area.
    pub area: String,
    /// Technology choice.
    pub choice: String,
    /// Success rate (0.0 to 1.0).
    pub success_rate: f32,
    /// Number of times this choice was used.
    pub usage_count: usize,
}

/// A recurring failure pattern.
#[derive(Debug, Clone, Serialize)]
pub struct FailurePattern {
    /// Description of the failure pattern.
    pub pattern: String,
    /// How often this pattern has occurred.
    pub frequency: usize,
}

/// Trait for learning memory implementations.
///
/// Implementations persist signals to a store (SQLite, PostgreSQL, etc.)
/// and provide query methods for the decision engine to consult.
#[async_trait]
pub trait LearningStore: Send + Sync {
    /// Record a learning signal.
    async fn record(
        &self,
        signal: &LearningSignal,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Query the best choice for a decision area based on historical data.
    async fn best_choice(
        &self,
        area: &str,
        app_type: &str,
        tenant_id: &str,
    ) -> Result<Option<LearnedPreference>, Box<dyn std::error::Error + Send + Sync>>;

    /// Get aggregated learning insights for a tenant.
    async fn insights(
        &self,
        tenant_id: &str,
    ) -> Result<LearningInsights, Box<dyn std::error::Error + Send + Sync>>;
}
