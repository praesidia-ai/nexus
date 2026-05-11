//! Predictive Intent — predicts what the user will ask NEXT.
//!
//! Analyzes conversation history, project state, and patterns to preload
//! actions before the user requests them.
//!
//! This is the "mind reading" system that makes Nexus feel magical.

use serde::Serialize;

use crate::intent_engine::AppType;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct Prediction {
    /// Most likely next actions the user will take.
    pub likely_next_actions: Vec<PredictedAction>,
    /// Overall confidence in predictions (0.0–1.0).
    pub confidence: f64,
    /// Actions to preload/precompute in the background.
    pub preload_actions: Vec<PreloadAction>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PredictedAction {
    /// What the user will likely do.
    pub action: String,
    /// Confidence (0.0–1.0).
    pub confidence: f64,
    /// Category for UI hints.
    pub category: ActionCategory,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionCategory {
    Generate,
    Modify,
    Deploy,
    Test,
    Review,
    Configure,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreloadAction {
    /// What to precompute.
    pub action: String,
    /// Priority (1 = highest).
    pub priority: u32,
    /// Estimated compute cost (ms).
    pub estimated_ms: u64,
}

// ---------------------------------------------------------------------------
// Prediction context
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PredictionContext {
    /// Recent user messages (last 5).
    pub recent_messages: Vec<String>,
    /// Current project phase (1-4).
    pub project_phase: u32,
    /// App type if known.
    pub app_type: Option<AppType>,
    /// Whether the app has been generated.
    pub has_generated_code: bool,
    /// Whether the app is currently running.
    pub app_running: bool,
    /// Current taste score if available.
    pub taste_score: Option<u32>,
    /// Number of mutations applied.
    pub mutations_count: u32,
}

// ---------------------------------------------------------------------------
// Predict
// ---------------------------------------------------------------------------

/// Predict the user's next likely actions based on context.
///
/// 100% deterministic — no LLM calls.
pub fn predict(ctx: &PredictionContext) -> Prediction {
    let mut actions = Vec::new();
    let mut preloads = Vec::new();
    let mut confidence = 0.6;

    // ── Phase-based predictions ─────────────────────────────────────────

    if !ctx.has_generated_code {
        // User hasn't generated yet — they'll likely describe and generate
        actions.push(PredictedAction {
            action: "Describe and generate an app".into(),
            confidence: 0.9,
            category: ActionCategory::Generate,
        });
        preloads.push(PreloadAction {
            action: "precompute_intent_cache".into(),
            priority: 1,
            estimated_ms: 5,
        });
        confidence = 0.85;
    } else if !ctx.app_running {
        // Code exists but not running — they'll want to start it
        actions.push(PredictedAction {
            action: "Start the generated app".into(),
            confidence: 0.85,
            category: ActionCategory::Deploy,
        });
        preloads.push(PreloadAction {
            action: "precheck_dependencies".into(),
            priority: 1,
            estimated_ms: 500,
        });
    } else {
        // App is running — predict based on state
        if let Some(score) = ctx.taste_score {
            if score < 85 {
                actions.push(PredictedAction {
                    action: "Improve app quality (redesign)".into(),
                    confidence: 0.8,
                    category: ActionCategory::Modify,
                });
                preloads.push(PreloadAction {
                    action: "precompute_taste_improvements".into(),
                    priority: 1,
                    estimated_ms: 50,
                });
            }
        }

        if ctx.mutations_count == 0 {
            actions.push(PredictedAction {
                action: "Make a UI change (color, text, layout)".into(),
                confidence: 0.7,
                category: ActionCategory::Modify,
            });
        }

        actions.push(PredictedAction {
            action: "Deploy to production".into(),
            confidence: 0.5,
            category: ActionCategory::Deploy,
        });
    }

    // ── Message-based predictions ───────────────────────────────────────

    if let Some(last_msg) = ctx.recent_messages.last() {
        let lower = last_msg.to_lowercase();

        if lower.contains("change") || lower.contains("modify") || lower.contains("update") {
            actions.push(PredictedAction {
                action: "Apply another modification".into(),
                confidence: 0.75,
                category: ActionCategory::Modify,
            });
        }

        if lower.contains("deploy") || lower.contains("ship") || lower.contains("publish") {
            actions.push(PredictedAction {
                action: "Deploy or download the app".into(),
                confidence: 0.85,
                category: ActionCategory::Deploy,
            });
            preloads.push(PreloadAction {
                action: "prebuild_zip".into(),
                priority: 2,
                estimated_ms: 2000,
            });
        }

        if lower.contains("test") || lower.contains("check") || lower.contains("verify") {
            actions.push(PredictedAction {
                action: "Run tests or quality check".into(),
                confidence: 0.8,
                category: ActionCategory::Test,
            });
        }

        if lower.contains("add") || lower.contains("feature") || lower.contains("new") {
            actions.push(PredictedAction {
                action: "Add a new feature".into(),
                confidence: 0.8,
                category: ActionCategory::Generate,
            });
            preloads.push(PreloadAction {
                action: "prescan_project_brain".into(),
                priority: 1,
                estimated_ms: 20,
            });
        }
    }

    // ── App-type-specific predictions ───────────────────────────────────

    if let Some(ref app_type) = ctx.app_type {
        match app_type {
            AppType::SaasApp if ctx.has_generated_code && ctx.mutations_count < 3 => {
                actions.push(PredictedAction {
                    action: "Configure billing/Stripe integration".into(),
                    confidence: 0.6,
                    category: ActionCategory::Configure,
                });
            }
            AppType::ECommerce if ctx.has_generated_code => {
                actions.push(PredictedAction {
                    action: "Add product seed data".into(),
                    confidence: 0.65,
                    category: ActionCategory::Generate,
                });
            }
            _ => {}
        }
    }

    // Sort by confidence descending, take top 5
    actions.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
    actions.truncate(5);
    preloads.sort_by_key(|p| p.priority);
    preloads.truncate(3);

    Prediction {
        likely_next_actions: actions,
        confidence,
        preload_actions: preloads,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predicts_generation_for_new_project() {
        let ctx = PredictionContext {
            recent_messages: vec!["Build a CRM".into()],
            project_phase: 1,
            app_type: None,
            has_generated_code: false,
            app_running: false,
            taste_score: None,
            mutations_count: 0,
        };
        let pred = predict(&ctx);
        assert!(pred.likely_next_actions.iter().any(|a| a.action.contains("generate")));
        assert!(pred.confidence > 0.5);
    }

    #[test]
    fn predicts_start_for_generated_app() {
        let ctx = PredictionContext {
            recent_messages: vec![],
            project_phase: 2,
            app_type: Some(AppType::SaasApp),
            has_generated_code: true,
            app_running: false,
            taste_score: None,
            mutations_count: 0,
        };
        let pred = predict(&ctx);
        assert!(pred.likely_next_actions.iter().any(|a| a.action.contains("Start")));
    }

    #[test]
    fn predicts_improvement_for_low_taste() {
        let ctx = PredictionContext {
            recent_messages: vec![],
            project_phase: 3,
            app_type: Some(AppType::SaasApp),
            has_generated_code: true,
            app_running: true,
            taste_score: Some(72),
            mutations_count: 0,
        };
        let pred = predict(&ctx);
        assert!(pred.likely_next_actions.iter().any(|a| a.action.contains("quality") || a.action.contains("Improve")));
    }

    #[test]
    fn preloads_zip_for_deploy_intent() {
        let ctx = PredictionContext {
            recent_messages: vec!["I want to deploy this".into()],
            project_phase: 3,
            app_type: None,
            has_generated_code: true,
            app_running: true,
            taste_score: Some(92),
            mutations_count: 2,
        };
        let pred = predict(&ctx);
        assert!(pred.preload_actions.iter().any(|a| a.action.contains("zip")));
    }
}
