//! Quality scoring — the "taste" engine interface.
//!
//! The quality scorer evaluates generated artifacts (HTML, CSS, components)
//! against heuristic rules and produces a detailed score with per-dimension
//! breakdowns and actionable suggestions.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A complete quality score for a generated artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TasteScore {
    /// Overall quality score (0.0 to 100.0).
    pub overall: f32,
    /// Per-dimension scores.
    pub dimensions: Vec<DimensionScore>,
    /// Issues found during scoring.
    pub issues: Vec<QualityIssue>,
    /// Suggestions for improvement.
    pub suggestions: Vec<String>,
    /// Grade label (e.g., "A+", "B", "C-").
    pub grade: String,
    /// Whether this score passes the minimum quality threshold.
    pub passes_threshold: bool,
}

/// Score for a single quality dimension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionScore {
    /// Dimension name (e.g., "layout", "typography", "color", "spacing").
    pub dimension: String,
    /// Score for this dimension (0.0 to 100.0).
    pub score: f32,
    /// Weight of this dimension in the overall score.
    pub weight: f32,
    /// Details about what contributed to this score.
    pub details: Vec<String>,
}

/// A quality issue found during scoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityIssue {
    /// Issue severity.
    pub severity: IssueSeverity,
    /// Which dimension this issue belongs to.
    pub dimension: String,
    /// Human-readable description of the issue.
    pub description: String,
    /// File and location where the issue was found.
    pub location: Option<IssueLocation>,
    /// Suggested fix.
    pub fix_suggestion: Option<String>,
}

/// Severity of a quality issue.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IssueSeverity {
    /// Critical — must be fixed before shipping.
    Critical,
    /// Warning — should be fixed but not blocking.
    Warning,
    /// Info — minor improvement opportunity.
    Info,
}

/// Location of a quality issue in a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueLocation {
    /// File path (relative to project root).
    pub file: String,
    /// Line number, if applicable.
    pub line: Option<u32>,
    /// CSS selector or component name, if applicable.
    pub selector: Option<String>,
}

/// Trait for quality scoring implementations.
///
/// Implementations should be deterministic heuristic scorers, not LLM-based.
/// They evaluate generated artifacts against a set of rules and produce
/// actionable scores.
#[async_trait]
pub trait QualityScorer: Send + Sync {
    /// Score a set of generated files.
    async fn score(
        &self,
        project_dir: &str,
        files: &[String],
    ) -> Result<TasteScore, Box<dyn std::error::Error + Send + Sync>>;

    /// Get the minimum passing score threshold.
    fn threshold(&self) -> f32;
}
