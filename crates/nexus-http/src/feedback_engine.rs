//! Feedback Engine — user-driven system evolution.
//!
//! Accepts user feedback (positive/negative/suggestion) and updates
//! memory_unification, skill_dna, and project_brain accordingly.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    /// Type of feedback.
    pub feedback_type: FeedbackType,
    /// What the feedback is about (task, app, component, decision).
    pub context: FeedbackContext,
    /// Free-text message from the user.
    pub message: String,
    /// Optional: specific target (file path, decision area, agent name).
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackType {
    Positive,
    Negative,
    Suggestion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackContext {
    /// Feedback about a generated app.
    App,
    /// Feedback about a specific decision (architecture, design).
    Decision,
    /// Feedback about an agent's behavior.
    Agent,
    /// Feedback about code quality.
    CodeQuality,
    /// Feedback about UX/design.
    Design,
    /// Feedback about a specific feature.
    Feature,
    /// General feedback.
    General,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeedbackResult {
    /// What was updated based on feedback.
    pub updates_applied: Vec<String>,
    /// How this will affect future behavior.
    pub future_impact: String,
}

// ---------------------------------------------------------------------------
// Process feedback
// ---------------------------------------------------------------------------

/// Process user feedback and update all learning systems.
pub async fn process(
    app: &Arc<AppState>,
    project_id: Option<&str>,
    feedback: &Feedback,
) -> FeedbackResult {
    let mut updates = Vec::new();

    // 1. Update global memory based on feedback
    match feedback.feedback_type {
        FeedbackType::Positive => {
            if let Some(ref target) = feedback.target {
                crate::memory_unification::reinforce(app, target, &context_to_category(&feedback.context)).await;
                updates.push(format!("Reinforced pattern: {}", target));
            }
            // Record positive signal
            record_feedback_to_db(app, project_id, feedback, "positive").await;
            updates.push("Recorded positive signal for future decisions".into());
        }
        FeedbackType::Negative => {
            if let Some(ref target) = feedback.target {
                // Weaken the pattern
                let db = app.db.lock().await;
                let _ = db.execute(
                    "UPDATE global_memory
                     SET confidence = MAX(confidence - 0.1, 0.0),
                         updated_at = datetime('now')
                     WHERE key = ?1",
                    rusqlite::params![target],
                );
                drop(db);
                updates.push(format!("Weakened pattern: {}", target));
            }
            record_feedback_to_db(app, project_id, feedback, "negative").await;
            updates.push("Recorded negative signal — will avoid this approach".into());

            // Weaken related skill DNA
            weaken_related_skills(app, &feedback.message).await;
        }
        FeedbackType::Suggestion => {
            // Store as a preference
            let key = format!(
                "user_suggestion_{}",
                feedback.message.chars().take(30).collect::<String>()
                    .replace(' ', "_").to_lowercase()
            );
            let db = app.db.lock().await;
            let id = uuid::Uuid::new_v4().to_string();
            let _ = db.execute(
                "INSERT INTO global_memory
                 (id, category, key, value, source, confidence, times_applied, created_at, updated_at)
                 VALUES (?1, 'suggestion', ?2, ?3, 'user_feedback', 0.8, 0, datetime('now'), datetime('now'))
                 ON CONFLICT(key) DO UPDATE SET value = ?3, confidence = 0.8, updated_at = datetime('now')",
                rusqlite::params![id, key, feedback.message],
            );
            drop(db);
            updates.push(format!("Stored suggestion: {}", &feedback.message));
        }
    }

    // 2. Update project brain if project-specific
    if let Some(pid) = project_id {
        let project_dir = app.data_dir.join("projects").join(pid).join("generated");
        let mut intel = crate::project_brain::ProjectIntelligence::load(&project_dir);

        match feedback.feedback_type {
            FeedbackType::Positive => {
                if let Some(ref target) = feedback.target {
                    intel.record_successful_pattern(target);
                }
            }
            FeedbackType::Negative => {
                intel.record_failure_fix(
                    &feedback.message,
                    "User reported issue",
                    false,
                );
            }
            _ => {}
        }
        intel.user_preferences.push((
            format!("{:?}", feedback.feedback_type),
            feedback.message.clone(),
        ));
        intel.save(&project_dir);
        updates.push("Updated project intelligence".into());
    }

    let future_impact = match feedback.feedback_type {
        FeedbackType::Positive => {
            "Will prioritize similar approaches in future generations".into()
        }
        FeedbackType::Negative => {
            "Will avoid this approach and seek alternatives".into()
        }
        FeedbackType::Suggestion => {
            "Suggestion stored — will incorporate in future decisions".into()
        }
    };

    info!(
        feedback_type = ?feedback.feedback_type,
        context = ?feedback.context,
        updates = updates.len(),
        "Feedback processed"
    );

    FeedbackResult {
        updates_applied: updates,
        future_impact,
    }
}

async fn record_feedback_to_db(
    app: &Arc<AppState>,
    project_id: Option<&str>,
    feedback: &Feedback,
    signal: &str,
) {
    let db = app.db.lock().await;
    let id = uuid::Uuid::new_v4().to_string();
    let pid = project_id.unwrap_or("global");
    let _ = db.execute(
        "INSERT INTO feedback (id, project_id, run_id, step_id, rating, comment, signal, created_at)
         VALUES (?1, ?2, '', '', ?3, ?4, ?5, datetime('now'))",
        rusqlite::params![
            id,
            pid,
            match feedback.feedback_type {
                FeedbackType::Positive => 5,
                FeedbackType::Negative => 1,
                FeedbackType::Suggestion => 3,
            },
            feedback.message,
            signal,
        ],
    );
}

async fn weaken_related_skills(app: &Arc<AppState>, message: &str) {
    let lower = message.to_lowercase();
    let intent = if lower.contains("bug") || lower.contains("broken") {
        "bug_fix"
    } else if lower.contains("ugly") || lower.contains("design") || lower.contains("ui") {
        "ui_improvement"
    } else if lower.contains("slow") || lower.contains("performance") {
        "performance"
    } else {
        return; // Don't weaken if we can't determine what's wrong
    };

    let db = app.db.lock().await;
    let _ = db.execute(
        "UPDATE skill_dna
         SET confidence = MAX(confidence - 0.05, 0.0),
             failures = failures + 1,
             updated_at = datetime('now')
         WHERE intent = ?1 AND status = 'active'",
        rusqlite::params![intent],
    );
}

fn context_to_category(ctx: &FeedbackContext) -> String {
    match ctx {
        FeedbackContext::App => "app",
        FeedbackContext::Decision => "decision",
        FeedbackContext::Agent => "agent",
        FeedbackContext::CodeQuality => "code_quality",
        FeedbackContext::Design => "design",
        FeedbackContext::Feature => "feature",
        FeedbackContext::General => "general",
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_types_serialize() {
        let fb = Feedback {
            feedback_type: FeedbackType::Positive,
            context: FeedbackContext::App,
            message: "Great app!".into(),
            target: Some("dark_mode".into()),
        };
        let json = serde_json::to_string(&fb).unwrap();
        assert!(json.contains("positive"));
        assert!(json.contains("Great app"));
    }
}
