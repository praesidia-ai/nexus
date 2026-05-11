//! Decision Learning HTTP handlers — expose the feedback loop API.
//!
//! SECURITY: every endpoint accepts a `project_id` in the body. Without
//! tenant validation, an authenticated tenant could inject feedback signals
//! into another tenant's project history (cross-tenant signal poisoning).
//! All write paths therefore call `validate_project_access` before recording.

use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::json;

use crate::{
    decision_learning::{self, DecisionOutcome, Outcome},
    error::{ApiError, ApiResult},
    security::auth::AuthContext,
    security::tenant::validate_project_access,
    state::AppState,
};

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RecordOutcomeRequest {
    pub project_id: String,
    pub area: String,
    pub chosen_value: String,
    pub outcome: String,
    pub user_override: Option<String>,
    pub build_success: Option<bool>,
    pub taste_score: Option<u32>,
    pub feedback: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BatchOutcomeRequest {
    pub outcomes: Vec<RecordOutcomeRequest>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /decisions/feedback — record the outcome of an architecture decision
pub async fn record_feedback(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<RecordOutcomeRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let outcome = parse_outcome(&body.outcome)?;

    // Tenant guard: refuse signals targeting projects the caller doesn't own.
    {
        let db = app.db.lock().await;
        validate_project_access(&db, &body.project_id, &auth.tenant_id)
            .map_err(ApiError::Forbidden)?;
    }

    decision_learning::record_outcome(
        &app,
        &DecisionOutcome {
            project_id: body.project_id.clone(),
            area: body.area.clone(),
            chosen_value: body.chosen_value.clone(),
            outcome,
            user_override: body.user_override.clone(),
            build_success: body.build_success,
            taste_score: body.taste_score,
            feedback: body.feedback.clone(),
        },
    )
    .await;

    Ok(Json(json!({
        "recorded": true,
        "area": body.area,
        "value": body.chosen_value,
        "outcome": body.outcome,
    })))
}

/// POST /decisions/feedback/batch — record multiple outcomes at once
pub async fn record_feedback_batch(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<BatchOutcomeRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if body.outcomes.len() > 1024 {
        return Err(ApiError::BadRequest(
            "batch capped at 1024 outcomes per call".into(),
        ));
    }

    // Tenant guard against the WHOLE batch up-front: a single foreign
    // project_id fails the entire request rather than recording a partial
    // mix of valid + invalid signals.
    {
        let db = app.db.lock().await;
        for req in &body.outcomes {
            validate_project_access(&db, &req.project_id, &auth.tenant_id)
                .map_err(ApiError::Forbidden)?;
        }
    }

    let mut recorded = 0usize;

    for req in &body.outcomes {
        let outcome = parse_outcome(&req.outcome)?;
        decision_learning::record_outcome(
            &app,
            &DecisionOutcome {
                project_id: req.project_id.clone(),
                area: req.area.clone(),
                chosen_value: req.chosen_value.clone(),
                outcome,
                user_override: req.user_override.clone(),
                build_success: req.build_success,
                taste_score: req.taste_score,
                feedback: req.feedback.clone(),
            },
        )
        .await;
        recorded += 1;
    }

    Ok(Json(json!({ "recorded": recorded })))
}

/// GET /decisions/weights — get all learned decision weights
pub async fn get_weights(State(app): State<Arc<AppState>>) -> ApiResult<Json<serde_json::Value>> {
    let weights = decision_learning::get_all_weights(&app).await;
    Ok(Json(serde_json::to_value(weights).unwrap_or(json!([]))))
}

/// GET /decisions/learning — get the learning summary
pub async fn get_learning_summary(
    State(app): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let summary = decision_learning::learning_summary(&app).await;
    Ok(Json(serde_json::to_value(summary).unwrap_or(json!({}))))
}

/// GET /decisions/recommend — get best recommendation for a decision area
pub async fn recommend_for_area(
    State(app): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> ApiResult<Json<serde_json::Value>> {
    let area = params.get("area").cloned().unwrap_or_default();
    if area.is_empty() {
        return Err(ApiError::BadRequest("area query param required".into()));
    }

    let weights = decision_learning::get_all_weights(&app).await;
    let area_weights: Vec<_> = weights.iter().filter(|w| w.area == area).collect();

    if area_weights.is_empty() {
        return Ok(Json(json!({
            "area": area,
            "recommendation": null,
            "reason": "No learning data yet for this area",
        })));
    }

    // Best choice = highest weight_adj with positive success_count
    let best = area_weights
        .iter()
        .filter(|w| w.success_count > 0)
        .max_by(|a, b| {
            a.weight_adj
                .partial_cmp(&b.weight_adj)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

    match best {
        Some(w) => Ok(Json(json!({
            "area": area,
            "recommendation": w.value,
            "weight_adj": w.weight_adj,
            "success_count": w.success_count,
            "failure_count": w.failure_count,
            "override_count": w.override_count,
            "confidence": compute_confidence(w.success_count, w.failure_count, w.override_count),
        }))),
        None => Ok(Json(json!({
            "area": area,
            "recommendation": null,
            "reason": "No successful decisions recorded yet",
        }))),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_outcome(s: &str) -> ApiResult<Outcome> {
    match s {
        "success" => Ok(Outcome::Success),
        "failure" => Ok(Outcome::Failure),
        "override" => Ok(Outcome::Override),
        _ => Err(ApiError::BadRequest(format!(
            "Invalid outcome '{}'. Use: success, failure, override",
            s
        ))),
    }
}

fn compute_confidence(success: i64, failure: i64, overrides: i64) -> f64 {
    let total = (success + failure + overrides).max(1) as f64;
    let positive = success as f64;
    let raw = positive / total;
    // Dampen confidence when we have few samples
    let sample_weight = (total / 10.0).min(1.0);
    raw * sample_weight + 0.5 * (1.0 - sample_weight)
}
