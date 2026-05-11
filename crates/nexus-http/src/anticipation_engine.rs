//! Anticipation Engine — detects patterns in user behavior and auto-triggers actions.
//!
//! Goes beyond predictive_intent (which predicts WHAT the user will do) by detecting
//! HOW they behave: repeated actions, incomplete flows, hesitation patterns.
//! Can auto-trigger improvements, fixes, and optimizations.

use serde::Serialize;

use crate::predictive_intent::{self, PredictionContext};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct AnticipatedAction {
    /// What Nexus suggests or will auto-execute.
    pub suggestion: String,
    /// Confidence (0.0–1.0).
    pub confidence: f64,
    /// Whether this should be auto-executed (no user prompt needed).
    pub auto_execute: bool,
    /// Why this was anticipated.
    pub reason: String,
    /// Category.
    pub category: AnticipationCategory,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnticipationCategory {
    Improvement,
    Fix,
    Optimization,
    Feature,
    Guidance,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnticipationReport {
    /// Detected behavior patterns.
    pub patterns_detected: Vec<BehaviorPattern>,
    /// Anticipated actions.
    pub actions: Vec<AnticipatedAction>,
    /// Underlying prediction (from predictive_intent).
    pub prediction: predictive_intent::Prediction,
}

#[derive(Debug, Clone, Serialize)]
pub struct BehaviorPattern {
    pub pattern_type: PatternType,
    pub description: String,
    pub occurrences: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternType {
    /// User keeps doing the same action (e.g., multiple mutations on the same file).
    RepeatedAction,
    /// User started a flow but didn't finish (e.g., generated but didn't start app).
    IncompleteFlow,
    /// User is going back and forth (e.g., editing then undoing).
    Hesitation,
    /// User asks for help or seems stuck.
    Stuck,
    /// User is iterating rapidly (power user mode).
    RapidIteration,
}

// ---------------------------------------------------------------------------
// Analysis
// ---------------------------------------------------------------------------

/// Analyze user behavior and produce anticipated actions.
///
/// Deterministic — no LLM calls.
pub fn anticipate(ctx: &PredictionContext, action_history: &[UserAction]) -> AnticipationReport {
    let mut patterns = Vec::new();
    let mut actions = Vec::new();

    // Get base prediction from predictive_intent
    let prediction = predictive_intent::predict(ctx);

    // ── Detect repeated actions ────────────────────────────────────────
    let repeated = detect_repeated_actions(action_history);
    if let Some(repeated_pattern) = repeated {
        patterns.push(repeated_pattern.clone());

        actions.push(AnticipatedAction {
            suggestion: format!(
                "You've done '{}' {} times. Want me to automate this?",
                repeated_pattern.description, repeated_pattern.occurrences
            ),
            confidence: 0.8,
            auto_execute: false,
            reason: "Detected repeated manual action that could be automated".into(),
            category: AnticipationCategory::Optimization,
        });
    }

    // ── Detect incomplete flows ────────────────────────────────────────
    if ctx.has_generated_code && !ctx.app_running {
        patterns.push(BehaviorPattern {
            pattern_type: PatternType::IncompleteFlow,
            description: "Code generated but app not started".into(),
            occurrences: 1,
        });

        actions.push(AnticipatedAction {
            suggestion: "Start the generated app to see it live".into(),
            confidence: 0.85,
            auto_execute: false,
            reason: "App was generated but never started".into(),
            category: AnticipationCategory::Guidance,
        });
    }

    // App running but never tested
    if ctx.app_running && ctx.taste_score.is_none() {
        patterns.push(BehaviorPattern {
            pattern_type: PatternType::IncompleteFlow,
            description: "App running but quality not checked".into(),
            occurrences: 1,
        });

        actions.push(AnticipatedAction {
            suggestion: "Run quality check and runtime tests".into(),
            confidence: 0.7,
            auto_execute: false,
            reason: "App is running but hasn't been quality-tested".into(),
            category: AnticipationCategory::Improvement,
        });
    }

    // ── Detect low quality needing auto-fix ────────────────────────────
    if let Some(score) = ctx.taste_score {
        if score < 80 {
            actions.push(AnticipatedAction {
                suggestion: "Auto-improve app quality (taste score is low)".into(),
                confidence: 0.9,
                auto_execute: true, // Safe to auto-execute
                reason: format!("Taste score {} is below 80 threshold", score),
                category: AnticipationCategory::Fix,
            });
        }
    }

    // ── Detect stuck user ──────────────────────────────────────────────
    let recent_msgs = &ctx.recent_messages;
    if recent_msgs.len() >= 3 {
        let help_words = ["help", "stuck", "error", "broken", "wrong", "not working", "how"];
        let stuck_signals = recent_msgs
            .iter()
            .filter(|m| {
                let lower = m.to_lowercase();
                help_words.iter().any(|w| lower.contains(w))
            })
            .count();

        if stuck_signals >= 2 {
            patterns.push(BehaviorPattern {
                pattern_type: PatternType::Stuck,
                description: "User appears to be struggling".into(),
                occurrences: stuck_signals as u32,
            });

            actions.push(AnticipatedAction {
                suggestion: "Run diagnostic analysis on the project".into(),
                confidence: 0.75,
                auto_execute: false,
                reason: "Multiple messages indicate user is struggling".into(),
                category: AnticipationCategory::Fix,
            });
        }
    }

    // ── Detect rapid iteration (power user) ────────────────────────────
    if ctx.mutations_count > 5 {
        patterns.push(BehaviorPattern {
            pattern_type: PatternType::RapidIteration,
            description: format!("{} mutations applied — power user mode", ctx.mutations_count),
            occurrences: ctx.mutations_count,
        });

        actions.push(AnticipatedAction {
            suggestion: "Switch to autonomous mode for faster iteration".into(),
            confidence: 0.6,
            auto_execute: false,
            reason: "High mutation count suggests user wants rapid changes".into(),
            category: AnticipationCategory::Optimization,
        });
    }

    // Sort by confidence
    actions.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

    AnticipationReport {
        patterns_detected: patterns,
        actions,
        prediction,
    }
}

// ---------------------------------------------------------------------------
// User action tracking
// ---------------------------------------------------------------------------

/// A recorded user action (for pattern detection).
#[derive(Debug, Clone)]
pub struct UserAction {
    pub action_type: String,
    pub target: String,
    pub timestamp_ms: u64,
}

fn detect_repeated_actions(history: &[UserAction]) -> Option<BehaviorPattern> {
    if history.len() < 3 {
        return None;
    }

    // Count action types in last 10 actions
    let recent = &history[history.len().saturating_sub(10)..];
    let mut counts: Vec<(&str, u32)> = Vec::new();

    for action in recent {
        if let Some(entry) = counts.iter_mut().find(|(t, _)| *t == action.action_type.as_str()) {
            entry.1 += 1;
        } else {
            counts.push((&action.action_type, 1));
        }
    }

    // Find the most repeated action (>= 3 times)
    counts
        .into_iter()
        .filter(|(_, count)| *count >= 3)
        .max_by_key(|(_, count)| *count)
        .map(|(action, count)| BehaviorPattern {
            pattern_type: PatternType::RepeatedAction,
            description: action.to_string(),
            occurrences: count,
        })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn base_ctx() -> PredictionContext {
        PredictionContext {
            recent_messages: vec![],
            project_phase: 2,
            app_type: None,
            has_generated_code: true,
            app_running: false,
            taste_score: None,
            mutations_count: 0,
        }
    }

    #[test]
    fn detects_incomplete_flow() {
        let ctx = base_ctx(); // generated but not running
        let report = anticipate(&ctx, &[]);
        assert!(report
            .patterns_detected
            .iter()
            .any(|p| matches!(p.pattern_type, PatternType::IncompleteFlow)));
        assert!(report
            .actions
            .iter()
            .any(|a| a.suggestion.to_lowercase().contains("start")));
    }

    #[test]
    fn detects_low_quality_auto_fix() {
        let mut ctx = base_ctx();
        ctx.app_running = true;
        ctx.taste_score = Some(65);
        let report = anticipate(&ctx, &[]);
        assert!(report.actions.iter().any(|a| a.auto_execute && a.suggestion.contains("quality")));
    }

    #[test]
    fn detects_stuck_user() {
        let mut ctx = base_ctx();
        ctx.recent_messages = vec![
            "This isn't working".into(),
            "Help me fix this error".into(),
            "Something is wrong".into(),
        ];
        let report = anticipate(&ctx, &[]);
        assert!(report
            .patterns_detected
            .iter()
            .any(|p| matches!(p.pattern_type, PatternType::Stuck)));
    }

    #[test]
    fn detects_repeated_actions() {
        let history: Vec<UserAction> = (0..5)
            .map(|i| UserAction {
                action_type: "mutate_ui".into(),
                target: "page.tsx".into(),
                timestamp_ms: i * 1000,
            })
            .collect();
        let report = anticipate(&base_ctx(), &history);
        assert!(report
            .patterns_detected
            .iter()
            .any(|p| matches!(p.pattern_type, PatternType::RepeatedAction)));
    }

    #[test]
    fn detects_power_user() {
        let mut ctx = base_ctx();
        ctx.mutations_count = 8;
        ctx.app_running = true;
        let report = anticipate(&ctx, &[]);
        assert!(report
            .patterns_detected
            .iter()
            .any(|p| matches!(p.pattern_type, PatternType::RapidIteration)));
    }
}
