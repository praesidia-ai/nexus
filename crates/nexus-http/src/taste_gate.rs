//! Taste Gate — hard quality gate that prevents shipping apps below threshold.
//!
//! RULE: No app ships with taste score < target (default 90, 85 for simple).
//!
//! Flow: generate → build → score → redesign → score → repeat (max 3)
//! If still < target after max attempts → FAIL with explanation.

use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use tracing::{info, warn};

use crate::state::AppState;
use crate::taste_engine::{self, TasteImprovement, TasteScore};

/// Result of the taste gate evaluation.
#[derive(Debug, Clone, Serialize)]
pub struct TasteGateResult {
    /// Whether the app passed the gate.
    pub passed: bool,
    /// Final taste score after all attempts.
    pub final_score: TasteScore,
    /// Target score that was required.
    pub target_score: u32,
    /// Number of redesign attempts made.
    pub attempts: u32,
    /// Score history across attempts.
    pub score_history: Vec<u32>,
    /// Improvements applied during redesign.
    pub improvements_applied: Vec<String>,
    /// If failed, explanation of why and what's still wrong.
    pub failure_explanation: Option<String>,
}

/// Run the taste gate: score → redesign loop → pass/fail.
///
/// Returns `Ok(result)` with `result.passed` indicating success/failure.
/// This never returns Err — it always produces a result.
pub async fn enforce_taste_gate(
    app: &Arc<AppState>,
    project_id: &str,
    project_dir: &Path,
    target_score: u32,
    max_attempts: u32,
) -> TasteGateResult {
    let mut score_history = Vec::new();
    let mut improvements_applied = Vec::new();
    let mut current_score = taste_engine::score_project(project_dir);

    score_history.push(current_score.overall);

    info!(
        project_id = %project_id,
        score = current_score.overall,
        target = target_score,
        "Taste gate: initial score"
    );

    // Already passing?
    if current_score.overall >= target_score {
        return TasteGateResult {
            passed: true,
            final_score: current_score,
            target_score,
            attempts: 0,
            score_history,
            improvements_applied,
            failure_explanation: None,
        };
    }

    // Redesign loop
    for attempt in 1..=max_attempts {
        info!(
            attempt = attempt,
            current = current_score.overall,
            target = target_score,
            "Taste gate: redesign attempt"
        );

        // Get prioritized fix suggestions
        let fixes = prioritize_fixes(&current_score.improvements, target_score - current_score.overall);

        if fixes.is_empty() {
            warn!("Taste gate: no fixes available, stopping redesign loop");
            break;
        }

        // Apply fixes via taste_redesign
        let applied = apply_taste_fixes(app, project_id, project_dir, &fixes).await;
        improvements_applied.extend(applied);

        // Re-score
        current_score = taste_engine::score_project(project_dir);
        score_history.push(current_score.overall);

        info!(
            attempt = attempt,
            new_score = current_score.overall,
            target = target_score,
            "Taste gate: score after redesign"
        );

        if current_score.overall >= target_score {
            return TasteGateResult {
                passed: true,
                final_score: current_score,
                target_score,
                attempts: attempt,
                score_history,
                improvements_applied,
                failure_explanation: None,
            };
        }
    }

    // Failed after all attempts
    let gap = target_score.saturating_sub(current_score.overall);
    let remaining_issues: Vec<String> = current_score
        .improvements
        .iter()
        .filter(|i| i.priority == "critical" || i.priority == "high")
        .map(|i| format!("[{}] {}: {}", i.priority, i.axis, i.description))
        .collect();

    let failure_explanation = format!(
        "App scored {}/100 after {} redesign attempt(s). Target was {}. \
         Gap: {} points. Remaining issues:\n{}",
        current_score.overall,
        max_attempts,
        target_score,
        gap,
        remaining_issues.join("\n"),
    );

    warn!(
        project_id = %project_id,
        final_score = current_score.overall,
        target = target_score,
        attempts = max_attempts,
        "Taste gate: FAILED"
    );

    TasteGateResult {
        passed: false,
        final_score: current_score,
        target_score,
        attempts: max_attempts,
        score_history,
        improvements_applied,
        failure_explanation: Some(failure_explanation),
    }
}

/// Prioritize fixes that will give the biggest score improvement.
fn prioritize_fixes(improvements: &[TasteImprovement], gap: u32) -> Vec<TasteImprovement> {
    let mut fixes: Vec<TasteImprovement> = improvements.to_vec();

    // Sort by priority: critical > high > medium > low
    fixes.sort_by(|a, b| {
        let priority_ord = |p: &str| -> u32 {
            match p {
                "critical" => 0,
                "high" => 1,
                "medium" => 2,
                "low" => 3,
                _ => 4,
            }
        };
        priority_ord(&a.priority).cmp(&priority_ord(&b.priority))
    });

    // Take enough fixes to potentially close the gap
    // Each critical fix ~5 points, high ~3, medium ~2, low ~1
    let mut estimated_gain = 0u32;
    let mut selected = Vec::new();
    for fix in fixes {
        if estimated_gain >= gap + 5 {
            break; // Enough fixes selected
        }
        let gain = match fix.priority.as_str() {
            "critical" => 5,
            "high" => 3,
            "medium" => 2,
            _ => 1,
        };
        estimated_gain += gain;
        selected.push(fix);
    }

    selected
}

/// Apply taste fixes using the mutation engine.
async fn apply_taste_fixes(
    app: &Arc<AppState>,
    project_id: &str,
    project_dir: &Path,
    fixes: &[TasteImprovement],
) -> Vec<String> {
    let mut applied = Vec::new();

    for fix in fixes {
        let change = format!(
            "Improve {}: {}. Suggestion: {}",
            fix.axis, fix.description, fix.suggestion
        );

        let target_file = fix.file.clone();

        let request = crate::mutation_engine::MutationRequest {
            change,
            target_file,
        };

        match crate::mutation_engine::mutate(app, project_id, project_dir, &request).await {
            Ok(result) if result.applied => {
                for fc in &result.files_changed {
                    applied.push(format!(
                        "[{}] {} → {}",
                        fix.axis, fc.path, fc.diff_summary
                    ));
                }
            }
            Ok(_) => {
                // Mutation generated but didn't apply
            }
            Err(e) => {
                tracing::warn!(error = %e, axis = %fix.axis, "Taste gate: fix failed, continuing to next");
            }
        }
    }

    applied
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taste_engine::TasteImprovement;

    #[test]
    fn prioritize_fixes_selects_critical_first() {
        let improvements = vec![
            TasteImprovement {
                axis: "visual_quality".into(),
                priority: "low".into(),
                description: "Add shadows".into(),
                file: None,
                suggestion: "Add box-shadow".into(),
            },
            TasteImprovement {
                axis: "accessibility".into(),
                priority: "critical".into(),
                description: "Missing alt text".into(),
                file: Some("page.tsx".into()),
                suggestion: "Add alt attributes".into(),
            },
            TasteImprovement {
                axis: "ux_clarity".into(),
                priority: "high".into(),
                description: "No loading states".into(),
                file: None,
                suggestion: "Add skeleton loaders".into(),
            },
        ];

        let fixes = prioritize_fixes(&improvements, 10);
        assert_eq!(fixes[0].priority, "critical");
        assert_eq!(fixes[1].priority, "high");
    }
}
