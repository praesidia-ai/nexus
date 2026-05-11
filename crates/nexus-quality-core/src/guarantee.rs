//! Outcome guarantees — multi-cycle repair with certificates.
//!
//! The guarantee engine runs generated projects through validation checks
//! (build, lint, test, accessibility, quality score) and iteratively fixes
//! issues until all checks pass or the cycle budget is exhausted.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::scoring::TasteScore;

/// A guarantee certificate issued after all checks pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuaranteeCertificate {
    /// Unique certificate identifier.
    pub id: String,
    /// Project ID this certificate covers.
    pub project_id: String,
    /// ISO 8601 timestamp of issuance.
    pub issued_at: String,
    /// Results of all checks that were run.
    pub checks: Vec<CheckResult>,
    /// Number of fix cycles used before all checks passed.
    pub cycles_used: u32,
    /// Maximum cycles that were allowed.
    pub max_cycles: u32,
    /// Final quality score at time of certification.
    pub final_score: Option<TasteScore>,
    /// Whether all checks passed.
    pub all_passed: bool,
}

/// Result of a single guarantee check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    /// Check name (e.g., "build", "lint", "test", "accessibility").
    pub check: String,
    /// Whether the check passed.
    pub passed: bool,
    /// Details or error message.
    pub details: String,
    /// Duration of this check in milliseconds.
    pub duration_ms: u64,
}

/// Configuration for a guarantee run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuaranteeConfig {
    /// Maximum number of fix cycles before giving up.
    pub max_cycles: u32,
    /// Which checks to run.
    pub checks: Vec<String>,
    /// Minimum quality score to pass (if quality check is enabled).
    pub min_quality_score: Option<f32>,
    /// Whether to auto-fix issues between cycles.
    pub auto_fix: bool,
}

impl Default for GuaranteeConfig {
    fn default() -> Self {
        Self {
            max_cycles: 3,
            checks: vec![
                "build".to_string(),
                "lint".to_string(),
                "quality".to_string(),
            ],
            min_quality_score: Some(70.0),
            auto_fix: true,
        }
    }
}

/// Trait for outcome guarantee implementations.
#[async_trait]
pub trait OutcomeGuarantee: Send + Sync {
    /// Run the guarantee process on a project.
    async fn guarantee(
        &self,
        project_dir: &str,
        config: &GuaranteeConfig,
    ) -> Result<GuaranteeCertificate, Box<dyn std::error::Error + Send + Sync>>;
}
