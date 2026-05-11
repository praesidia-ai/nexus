//! Fix plans — structured remediation for quality issues.
//!
//! When the quality scorer or guarantee checks find issues, a fix plan
//! describes exactly what changes need to be made. Fix plans are consumed
//! by coding agents or automated fixers.

use serde::{Deserialize, Serialize};

/// A plan for fixing quality issues in a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixPlan {
    /// Unique plan identifier.
    pub id: String,
    /// Project ID this plan applies to.
    pub project_id: String,
    /// Individual fix steps, in suggested execution order.
    pub steps: Vec<FixStep>,
    /// Estimated effort level.
    pub estimated_effort: FixEffort,
    /// Which guarantee checks this plan addresses.
    pub addresses_checks: Vec<String>,
}

/// A single fix step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixStep {
    /// Step number (1-based, in order).
    pub order: u32,
    /// Human-readable description of the fix.
    pub description: String,
    /// File to modify.
    pub file: String,
    /// Type of change.
    pub change_type: ChangeType,
    /// The actual change to make (implementation-specific).
    pub change: serde_json::Value,
    /// Confidence that this fix will resolve the issue (0.0 to 1.0).
    pub confidence: f32,
}

/// Type of change in a fix step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChangeType {
    /// Replace content at a specific location.
    Replace,
    /// Insert new content.
    Insert,
    /// Delete content.
    Delete,
    /// Create a new file.
    CreateFile,
    /// Delete a file.
    DeleteFile,
    /// Run a command (e.g., install a dependency).
    RunCommand,
}

/// Estimated effort to apply a fix plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FixEffort {
    /// Quick fix, typically one or two lines.
    Trivial,
    /// Small change, a few lines across one or two files.
    Minor,
    /// Moderate change, multiple files.
    Moderate,
    /// Significant rework required.
    Major,
}
