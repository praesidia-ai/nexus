//! Decision Engine Learning — feedback loop that adjusts decision weights.
//!
//! Tracks outcomes of architecture decisions:
//! - Build success/failure
//! - User overrides (changed the decision)
//! - Taste scores after generation
//!
//! Adjusts weights so future decisions favor historically successful choices.
//! Weights are persisted in `decision_weights` table.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionOutcome {
    pub project_id: String,
    pub area: String,
    pub chosen_value: String,
    pub outcome: Outcome,
    pub user_override: Option<String>,
    pub build_success: Option<bool>,
    pub taste_score: Option<u32>,
    pub feedback: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Success,
    Override,
    Failure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionWeight {
    pub area: String,
    pub value: String,
    pub success_count: i64,
    pub failure_count: i64,
    pub override_count: i64,
    pub weight_adj: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningSummary {
    pub total_decisions: i64,
    pub success_rate: f64,
    pub override_rate: f64,
    pub weights: Vec<DecisionWeight>,
}

// ---------------------------------------------------------------------------
// Record feedback
// ---------------------------------------------------------------------------

/// Record the outcome of a decision and update weights.
pub async fn record_outcome(app: &Arc<AppState>, outcome: &DecisionOutcome) {
    let db = app.db.lock().await;
    let id = uuid::Uuid::new_v4().to_string();

    let outcome_str = match outcome.outcome {
        Outcome::Success => "success",
        Outcome::Override => "override",
        Outcome::Failure => "failure",
    };

    // Record the individual feedback
    let _ = db.execute(
        "INSERT INTO decision_feedback (id, project_id, area, chosen_value, outcome, user_override, build_success, taste_score, feedback_signal)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            id,
            outcome.project_id,
            outcome.area,
            outcome.chosen_value,
            outcome_str,
            outcome.user_override,
            outcome.build_success.map(|b| b as i64),
            outcome.taste_score.map(|s| s as i64),
            outcome.feedback,
        ],
    );

    // Update aggregate weights
    let (success_delta, failure_delta, override_delta) = match outcome.outcome {
        Outcome::Success => (1, 0, 0),
        Outcome::Failure => (0, 1, 0),
        Outcome::Override => (0, 0, 1),
    };

    // Weight adjustment: +0.1 for success, -0.2 for failure, -0.1 for override
    let adj = match outcome.outcome {
        Outcome::Success => 0.1,
        Outcome::Failure => -0.2,
        Outcome::Override => -0.1,
    };

    let _ = db.execute(
        "INSERT INTO decision_weights (area, value, success_count, failure_count, override_count, weight_adj)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(area, value) DO UPDATE SET
           success_count = success_count + ?3,
           failure_count = failure_count + ?4,
           override_count = override_count + ?5,
           weight_adj = weight_adj + ?6,
           updated_at = datetime('now')",
        rusqlite::params![
            outcome.area,
            outcome.chosen_value,
            success_delta,
            failure_delta,
            override_delta,
            adj,
        ],
    );

    // If user overrode, also boost the override choice
    if let Some(ref override_val) = outcome.user_override {
        let _ = db.execute(
            "INSERT INTO decision_weights (area, value, success_count, failure_count, override_count, weight_adj)
             VALUES (?1, ?2, 1, 0, 0, 0.15)
             ON CONFLICT(area, value) DO UPDATE SET
               success_count = success_count + 1,
               weight_adj = weight_adj + 0.15,
               updated_at = datetime('now')",
            rusqlite::params![outcome.area, override_val],
        );
    }

    info!(
        area = %outcome.area,
        value = %outcome.chosen_value,
        outcome = outcome_str,
        "Decision outcome recorded"
    );
}

// ---------------------------------------------------------------------------
// Query weights
// ---------------------------------------------------------------------------

/// Get the learned weight adjustment for a specific decision.
pub async fn get_weight(app: &Arc<AppState>, area: &str, value: &str) -> f64 {
    let db = app.db.lock().await;
    db.query_row(
        "SELECT weight_adj FROM decision_weights WHERE area = ?1 AND value = ?2",
        rusqlite::params![area, value],
        |row| row.get::<_, f64>(0),
    )
    .unwrap_or(0.0)
}

/// Get all learned weights.
pub async fn get_all_weights(app: &Arc<AppState>) -> Vec<DecisionWeight> {
    let db = app.db.lock().await;
    let mut stmt = match db.prepare(
        "SELECT area, value, success_count, failure_count, override_count, weight_adj
         FROM decision_weights ORDER BY area, weight_adj DESC"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let rows: Vec<DecisionWeight> = stmt
        .query_map([], |row| {
            Ok(DecisionWeight {
                area: row.get(0)?,
                value: row.get(1)?,
                success_count: row.get(2)?,
                failure_count: row.get(3)?,
                override_count: row.get(4)?,
                weight_adj: row.get(5)?,
            })
        })
        .ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();

    rows
}

/// Get a learning summary across all decisions.
pub async fn learning_summary(app: &Arc<AppState>) -> LearningSummary {
    let weights = get_all_weights(app).await;
    let db = app.db.lock().await;

    let total: i64 = db
        .query_row("SELECT COUNT(*) FROM decision_feedback", [], |r| r.get(0))
        .unwrap_or(0);

    let successes: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM decision_feedback WHERE outcome = 'success'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let overrides: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM decision_feedback WHERE outcome = 'override'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    LearningSummary {
        total_decisions: total,
        success_rate: if total > 0 {
            successes as f64 / total as f64
        } else {
            0.0
        },
        override_rate: if total > 0 {
            overrides as f64 / total as f64
        } else {
            0.0
        },
        weights,
    }
}
