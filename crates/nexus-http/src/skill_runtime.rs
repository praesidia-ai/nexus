//! Skill Runtime — turns skill_dna into active execution intelligence.
//!
//! Before agent runs: inject relevant learned patterns.
//! During execution: bias toward previously successful strategies.
//! After success: reinforce pattern in skill_dna.

use std::sync::Arc;

use serde::Serialize;
use tracing::info;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct InjectedKnowledge {
    /// Prompt fragments injected from skill DNA.
    pub fragments: Vec<String>,
    /// Patterns that should be preferred.
    pub preferred_patterns: Vec<String>,
    /// Constraints that should be enforced.
    pub constraints: Vec<String>,
    /// Number of skills that contributed.
    pub skills_used: usize,
}

#[derive(Debug, Clone)]
pub struct TaskOutcome {
    pub task_intent: String,
    pub success: bool,
    pub iterations_used: u32,
    pub tools_used: Vec<String>,
    pub files_changed: Vec<String>,
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// Inject: enrich prompt with learned intelligence
// ---------------------------------------------------------------------------

/// Inject relevant skill DNA knowledge into a prompt BEFORE agent execution.
///
/// Queries the skill_dna table for active skills matching the task intent,
/// and composes them into the system prompt.
pub async fn inject(
    app: &Arc<AppState>,
    base_prompt: &str,
    task_description: &str,
) -> (String, InjectedKnowledge) {
    let db = app.db.lock().await;

    // Detect intent from task description
    let intent = infer_task_intent(task_description);

    // Query active skills matching this intent
    let skills: Vec<(String, String, String, f64)> = {
        let mut stmt = match db.prepare(
            "SELECT prompt_fragment, patterns, constraints, confidence
             FROM skill_dna
             WHERE status = 'active' AND intent = ?1
             ORDER BY confidence DESC, successes DESC
             LIMIT 5"
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "Skill runtime: failed to query skill_dna, proceeding without learned skills");
                return (base_prompt.to_string(), InjectedKnowledge {
                    fragments: vec![], preferred_patterns: vec![],
                    constraints: vec![], skills_used: 0,
                });
            }
        };

        stmt.query_map(rusqlite::params![intent], |row| {
            Ok((
                row.get::<_, String>(0)?,       // prompt_fragment
                row.get::<_, String>(1)?,       // patterns (JSON array)
                row.get::<_, String>(2)?,       // constraints (JSON array)
                row.get::<_, f64>(3)?,          // confidence
            ))
        })
        .ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };

    drop(db);

    if skills.is_empty() {
        return (base_prompt.to_string(), InjectedKnowledge {
            fragments: vec![], preferred_patterns: vec![],
            constraints: vec![], skills_used: 0,
        });
    }

    let mut fragments = Vec::new();
    let mut patterns = Vec::new();
    let mut constraints = Vec::new();

    for (fragment, patterns_json, constraints_json, confidence) in &skills {
        if *confidence >= 0.5 && !fragment.is_empty() {
            fragments.push(fragment.clone());
        }
        if let Ok(p) = serde_json::from_str::<Vec<String>>(patterns_json) {
            patterns.extend(p);
        }
        if let Ok(c) = serde_json::from_str::<Vec<String>>(constraints_json) {
            constraints.extend(c);
        }
    }

    // Compose enriched prompt
    let mut enriched = base_prompt.to_string();
    if !fragments.is_empty() {
        enriched.push_str("\n\n## Learned Patterns (from previous successful executions)\n");
        for frag in &fragments {
            enriched.push_str(&format!("- {}\n", frag));
        }
    }
    if !patterns.is_empty() {
        enriched.push_str("\n## Preferred Approaches\n");
        for pattern in &patterns {
            enriched.push_str(&format!("- {}\n", pattern));
        }
    }
    if !constraints.is_empty() {
        enriched.push_str("\n## Constraints (enforce these)\n");
        for constraint in &constraints {
            enriched.push_str(&format!("- {}\n", constraint));
        }
    }

    let knowledge = InjectedKnowledge {
        fragments: fragments.clone(),
        preferred_patterns: patterns,
        constraints,
        skills_used: skills.len(),
    };

    info!(
        intent = %intent,
        skills = skills.len(),
        fragments = fragments.len(),
        "Skill runtime: injected learned knowledge"
    );

    (enriched, knowledge)
}

/// Record a task outcome and reinforce or weaken relevant skill DNA.
pub async fn learn(app: &Arc<AppState>, outcome: &TaskOutcome) {
    let intent = infer_task_intent(&outcome.task_intent);
    let db = app.db.lock().await;

    if outcome.success {
        // Reinforce: increment successes, boost confidence
        let _ = db.execute(
            "UPDATE skill_dna
             SET successes = successes + 1,
                 total_uses = total_uses + 1,
                 confidence = MIN(confidence + 0.02, 1.0),
                 updated_at = datetime('now')
             WHERE intent = ?1 AND status = 'active'",
            rusqlite::params![intent],
        );
    } else {
        // Weaken: increment failures, reduce confidence
        let _ = db.execute(
            "UPDATE skill_dna
             SET failures = failures + 1,
                 total_uses = total_uses + 1,
                 confidence = MAX(confidence - 0.05, 0.0),
                 updated_at = datetime('now')
             WHERE intent = ?1 AND status = 'active'",
            rusqlite::params![intent],
        );
    }

    // Archive skills with very low confidence
    let _ = db.execute(
        "UPDATE skill_dna SET status = 'archived', updated_at = datetime('now')
         WHERE confidence < 0.1 AND total_uses > 10 AND status = 'active'",
        [],
    );
}

/// Infer task intent category from a description.
fn infer_task_intent(description: &str) -> &'static str {
    let lower = description.to_lowercase();
    if lower.contains("bug") || lower.contains("fix") || lower.contains("error") {
        "bug_fix"
    } else if lower.contains("feature") || lower.contains("add") || lower.contains("implement") {
        "feature_add"
    } else if lower.contains("refactor") || lower.contains("clean") || lower.contains("reorganize") {
        "refactor"
    } else if lower.contains("test") || lower.contains("spec") || lower.contains("coverage") {
        "testing"
    } else if lower.contains("ui") || lower.contains("design") || lower.contains("style") || lower.contains("css") {
        "ui_improvement"
    } else if lower.contains("api") || lower.contains("endpoint") || lower.contains("route") {
        "api_development"
    } else if lower.contains("database") || lower.contains("schema") || lower.contains("migration") {
        "data_modeling"
    } else {
        "general"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_bug_fix_intent() {
        assert_eq!(infer_task_intent("Fix the login error"), "bug_fix");
    }

    #[test]
    fn infers_feature_intent() {
        assert_eq!(infer_task_intent("Add user profile page"), "feature_add");
    }

    #[test]
    fn infers_ui_intent() {
        assert_eq!(infer_task_intent("Redesign the dashboard UI"), "ui_improvement");
    }

    #[test]
    fn defaults_to_general() {
        assert_eq!(infer_task_intent("do something"), "general");
    }
}
