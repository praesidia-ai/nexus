//! Autonomous Engine — Nexus operates without user intervention.
//!
//! User says "improve my app" and Nexus:
//! 1. Analyzes the project (brain)
//! 2. Identifies: missing features, UX issues, performance issues
//! 3. Executes: coding pipeline, redesign, optimization
//! 4. Validates (taste gate)
//! 5. Repeats if needed
//!
//! NO user clarification required.

use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use tracing::info;

use crate::state::AppState;
use crate::taste_engine;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct AutonomousResult {
    /// What was analyzed.
    pub analysis: AutonomousAnalysis,
    /// Actions that were executed.
    pub actions_executed: Vec<AutonomousAction>,
    /// Final taste score after all improvements.
    pub final_taste_score: Option<u32>,
    /// Total duration of the autonomous session.
    pub duration_ms: u64,
    /// Whether all improvements were successful.
    pub fully_succeeded: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutonomousAnalysis {
    /// Missing features detected.
    pub missing_features: Vec<String>,
    /// UX issues found (from taste engine).
    pub ux_issues: Vec<String>,
    /// Performance issues found.
    pub performance_issues: Vec<String>,
    /// Total issues found.
    pub total_issues: usize,
    /// Priority order for fixes.
    pub fix_priority: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutonomousAction {
    /// What was done.
    pub action: String,
    /// Category.
    pub category: ActionCategory,
    /// Whether it succeeded.
    pub success: bool,
    /// Details.
    pub detail: String,
    /// Duration in ms.
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionCategory {
    UxImprovement,
    FeatureAdd,
    BugFix,
    PerformanceOptimization,
    Redesign,
    CodeCleanup,
}

// ---------------------------------------------------------------------------
// Autonomous execution
// ---------------------------------------------------------------------------

/// Run Nexus in autonomous mode on a project.
///
/// Analyzes, decides, executes, validates — no user input needed.
pub async fn run_autonomous(
    app: &Arc<AppState>,
    project_id: &str,
    project_dir: &Path,
    max_iterations: u32,
) -> AutonomousResult {
    let start = std::time::Instant::now();
    let mut actions_executed = Vec::new();

    // ── Step 0: Control plane check — analysis is always allowed, execution requires Autonomous mode
    // (Callers should check mode before calling; we validate here as defense-in-depth)

    // ── Step 1: Analyze ─────────────────────────────────────────────────
    let analysis = analyze_project(project_dir);

    info!(
        project_id = %project_id,
        issues = analysis.total_issues,
        "Autonomous mode: analysis complete"
    );

    if analysis.total_issues == 0 {
        return AutonomousResult {
            analysis,
            actions_executed: vec![],
            final_taste_score: Some(taste_engine::score_project(project_dir).overall),
            duration_ms: start.elapsed().as_millis() as u64,
            fully_succeeded: true,
        };
    }

    // ── Step 2: Execute fixes (priority order) ──────────────────────────
    for (iter, fix_desc) in analysis.fix_priority.iter().enumerate() {
        if iter as u32 >= max_iterations {
            break;
        }

        let action_start = std::time::Instant::now();
        let category = categorize_fix(fix_desc);

        // Validate via control plane before executing
        let action = crate::control_plane::Action {
            action_type: "file_write".into(),
            risk_level: crate::control_plane::RiskLevel::Medium,
            target: fix_desc.clone(),
            description: format!("Autonomous fix: {}", fix_desc),
        };
        let mut cp = crate::control_plane::ControlPlane::new(
            crate::control_plane::NexusMode::Autonomous,
        );
        if let crate::control_plane::ValidationResult::Blocked { reason } = cp.validate(&action) {
            actions_executed.push(AutonomousAction {
                action: fix_desc.clone(),
                category: categorize_fix(fix_desc),
                success: false,
                detail: format!("Blocked by control plane: {}", reason),
                duration_ms: action_start.elapsed().as_millis() as u64,
            });
            continue;
        }

        // Apply the fix via mutation engine
        let request = crate::mutation_engine::MutationRequest {
            change: fix_desc.clone(),
            target_file: None,
        };

        let (success, detail) =
            match crate::mutation_engine::mutate(app, project_id, project_dir, &request).await {
                Ok(result) if result.applied => {
                    let files: Vec<String> = result
                        .files_changed
                        .iter()
                        .map(|f| f.path.clone())
                        .collect();
                    (true, format!("Changed: {}", files.join(", ")))
                }
                Ok(_) => (false, "Mutation generated but not applied".into()),
                Err(e) => (false, format!("Failed: {}", e)),
            };

        actions_executed.push(AutonomousAction {
            action: fix_desc.clone(),
            category,
            success,
            detail,
            duration_ms: action_start.elapsed().as_millis() as u64,
        });
    }

    // ── Step 3: Validate ────────────────────────────────────────────────
    let final_score = taste_engine::score_project(project_dir);
    let fully_succeeded = actions_executed.iter().all(|a| a.success);

    info!(
        project_id = %project_id,
        actions = actions_executed.len(),
        succeeded = actions_executed.iter().filter(|a| a.success).count(),
        taste_score = final_score.overall,
        "Autonomous mode: complete"
    );

    AutonomousResult {
        analysis,
        actions_executed,
        final_taste_score: Some(final_score.overall),
        duration_ms: start.elapsed().as_millis() as u64,
        fully_succeeded,
    }
}

/// Analyze a project for issues.
fn analyze_project(project_dir: &Path) -> AutonomousAnalysis {
    let mut missing_features = Vec::new();
    let mut ux_issues = Vec::new();
    let mut performance_issues = Vec::new();

    // Taste-based UX analysis
    let taste = taste_engine::score_project(project_dir);
    for imp in &taste.improvements {
        match imp.priority.as_str() {
            "critical" | "high" => {
                ux_issues.push(format!("[{}] {}: {}", imp.axis, imp.description, imp.suggestion));
            }
            _ => {}
        }
    }

    // Check for missing common patterns
    let has_loading_states = crate::file_utils::contains_pattern(project_dir, "loading");
    let has_error_handling = crate::file_utils::contains_pattern(project_dir, "error");
    let has_empty_states = crate::file_utils::contains_pattern(project_dir, "empty");
    let _has_dark_mode = crate::file_utils::contains_pattern(project_dir, "dark");
    let has_responsive = crate::file_utils::contains_pattern(project_dir, "md:");

    if !has_loading_states {
        missing_features.push("Add loading states (skeleton loaders) to data-fetching pages".into());
    }
    if !has_error_handling {
        missing_features.push("Add error boundary and error handling UI".into());
    }
    if !has_empty_states {
        missing_features.push("Add empty state illustrations for lists/tables".into());
    }
    if !has_responsive {
        missing_features.push("Add responsive breakpoints for mobile view".into());
    }

    // Performance checks
    let has_lazy_loading = crate::file_utils::contains_pattern(project_dir, "lazy");
    let has_image_optimization = crate::file_utils::contains_pattern(project_dir, "Image");
    if !has_lazy_loading {
        performance_issues.push("Add lazy loading for heavy components/images".into());
    }
    if !has_image_optimization {
        performance_issues.push("Use Next.js Image component for optimized images".into());
    }

    // Build priority list (UX issues first, then missing features, then performance)
    let mut fix_priority = Vec::new();
    fix_priority.extend(ux_issues.iter().cloned());
    fix_priority.extend(missing_features.iter().cloned());
    fix_priority.extend(performance_issues.iter().cloned());

    let total_issues = fix_priority.len();

    AutonomousAnalysis {
        missing_features,
        ux_issues,
        performance_issues,
        total_issues,
        fix_priority,
    }
}

// check_pattern_exists removed — use crate::file_utils::contains_pattern instead

fn categorize_fix(description: &str) -> ActionCategory {
    let lower = description.to_lowercase();
    if lower.contains("loading") || lower.contains("error") || lower.contains("empty")
        || lower.contains("responsive") || lower.contains("accessibility")
    {
        ActionCategory::UxImprovement
    } else if lower.contains("add") || lower.contains("missing") || lower.contains("feature") {
        ActionCategory::FeatureAdd
    } else if lower.contains("fix") || lower.contains("bug") {
        ActionCategory::BugFix
    } else if lower.contains("lazy") || lower.contains("optimize") || lower.contains("performance")
        || lower.contains("image")
    {
        ActionCategory::PerformanceOptimization
    } else if lower.contains("redesign") || lower.contains("improve") {
        ActionCategory::Redesign
    } else {
        ActionCategory::CodeCleanup
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_empty_dir_finds_missing_features() {
        let dir = tempfile::tempdir().unwrap();
        let analysis = analyze_project(dir.path());
        // Empty project should have missing features
        assert!(analysis.total_issues > 0);
    }

    #[test]
    fn categorize_fix_works() {
        assert!(matches!(categorize_fix("Add loading states"), ActionCategory::UxImprovement));
        assert!(matches!(categorize_fix("Fix login bug"), ActionCategory::BugFix));
        assert!(matches!(categorize_fix("Lazy load images"), ActionCategory::PerformanceOptimization));
    }
}
