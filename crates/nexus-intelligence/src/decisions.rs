//! Architecture decision making with explainability.
//!
//! The decision layer takes an [`Intent`] and produces a set of
//! architecture decisions (framework, database, auth strategy, etc.)
//! with full explainability: why each choice was made, what alternatives
//! were considered, and how learning history influenced the result.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::intent::Intent;

/// A single architecture decision with full explainability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainedDecision {
    /// Decision area (e.g., "frontend_framework", "database", "auth").
    pub area: String,
    /// The chosen option (e.g., "Next.js", "PostgreSQL", "JWT").
    pub choice: String,
    /// Confidence in this decision (0.0 to 1.0).
    pub confidence: f32,
    /// Full explanation of why this choice was made.
    pub explanation: DecisionExplanation,
}

/// Explanation for a decision — supports transparency and overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionExplanation {
    /// The primary reason for this choice.
    pub primary_reason: String,
    /// All factors that influenced the decision.
    pub factors: Vec<DecisionFactor>,
    /// Alternatives that were considered and why they were rejected.
    pub alternatives: Vec<Alternative>,
    /// How much historical learning influenced this decision (0.0 to 1.0).
    pub learning_influence: f32,
}

/// A factor that influenced a decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionFactor {
    /// What the factor is (e.g., "app complexity").
    pub factor: String,
    /// How much weight this factor had (0.0 to 1.0).
    pub weight: f32,
    /// Where this factor came from (e.g., "intent", "domain_pack", "learning").
    pub source: String,
}

/// An alternative that was considered but not chosen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alternative {
    /// The alternative choice.
    pub choice: String,
    /// How it scored relative to the winner (0.0 to 1.0).
    pub score: f32,
    /// Why it was not selected.
    pub reason_rejected: String,
}

/// Report on the coherence of a set of decisions.
///
/// Checks that choices across different areas are compatible with each other
/// (e.g., choosing "serverless" hosting with a "WebSocket" real-time strategy
/// would produce a warning).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoherenceReport {
    /// Overall coherence score (0.0 to 1.0).
    pub overall_score: f32,
    /// Non-breaking but suboptimal combinations.
    pub warnings: Vec<CoherenceWarning>,
    /// Breaking incompatibilities that must be resolved.
    pub incompatibilities: Vec<CoherenceIncompatibility>,
    /// Actionable recommendations to improve coherence.
    pub recommendations: Vec<String>,
}

/// A non-breaking coherence warning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoherenceWarning {
    /// First decision area.
    pub area_a: String,
    /// First decision choice.
    pub choice_a: String,
    /// Second decision area.
    pub area_b: String,
    /// Second decision choice.
    pub choice_b: String,
    /// Why this combination is suboptimal.
    pub reason: String,
}

/// A breaking incompatibility between two decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoherenceIncompatibility {
    /// First decision area.
    pub area_a: String,
    /// First decision choice.
    pub choice_a: String,
    /// Second decision area.
    pub area_b: String,
    /// Second decision choice.
    pub choice_b: String,
    /// Why these choices are incompatible.
    pub reason: String,
    /// Suggested resolution.
    pub suggested_fix: String,
}

/// A complete set of decisions with coherence validation.
#[derive(Debug, Clone, Serialize)]
pub struct DecisionSet {
    /// All architecture decisions made.
    pub decisions: Vec<ExplainedDecision>,
    /// Coherence report for the decision set.
    pub coherence: CoherenceReport,
}

/// Trait for architecture decision making.
///
/// Implementations use intent analysis, domain knowledge, and historical
/// learning to select the best technology choices for each area.
#[async_trait]
pub trait DecisionMaker: Send + Sync {
    /// Make architecture decisions based on analyzed intent.
    async fn decide(
        &self,
        intent: &Intent,
    ) -> Result<DecisionSet, Box<dyn std::error::Error + Send + Sync>>;

    /// Validate coherence of a set of decisions.
    async fn validate_coherence(
        &self,
        decisions: &[ExplainedDecision],
    ) -> Result<CoherenceReport, Box<dyn std::error::Error + Send + Sync>>;
}
