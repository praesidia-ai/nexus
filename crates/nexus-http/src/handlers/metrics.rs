//! Metrics + feedback + analytics HTTP handlers.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;

use nexus_store::{
    AnalyticsService, FeedbackService, MetricsService, NewFeedback,
};

use crate::error::{ApiError, ApiResult};
use crate::security::auth::AuthContext;
use crate::security::tenant::validate_project_access;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Step metrics
// ---------------------------------------------------------------------------

/// GET /runs/:run_id/metrics — list step metrics for a run.
///
/// The run is resolved to a project_id and that project is validated against
/// the caller's tenant — preventing cross-tenant run inspection.
pub async fn list_step_metrics(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(run_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = state.db.lock().await;
    let svc = MetricsService::new(&db);
    // Resolve run -> project -> tenant BEFORE exposing any step rows.
    let project_id = svc
        .get_run_project_id(&run_id)
        .map_err(|e| ApiError::Internal(format!("Failed to resolve run: {e}")))?
        .ok_or_else(|| ApiError::NotFound(format!("run {run_id} not found")))?;
    validate_project_access(&db, &project_id, &auth.tenant_id).map_err(ApiError::Forbidden)?;
    let metrics = svc
        .list_step_metrics(&run_id)
        .map_err(|e| ApiError::Internal(format!("Failed to list step metrics: {e}")))?;
    Ok(Json(serde_json::to_value(metrics)?))
}

// ---------------------------------------------------------------------------
// Run metrics
// ---------------------------------------------------------------------------

/// GET /projects/:id/metrics — list run metrics for a project.
pub async fn list_run_metrics(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = state.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id).map_err(ApiError::Forbidden)?;
    let svc = MetricsService::new(&db);
    let metrics = svc
        .list_run_metrics(Some(&project_id))
        .map_err(|e| ApiError::Internal(format!("Failed to list run metrics: {e}")))?;
    Ok(Json(serde_json::to_value(metrics)?))
}

// ---------------------------------------------------------------------------
// Feedback
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct FeedbackInput {
    pub run_id: Option<String>,
    pub step_id: Option<String>,
    pub rating: i32,
    pub comment: Option<String>,
    pub signal: String,
}

/// POST /projects/:id/feedback — submit feedback.
pub async fn add_feedback(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
    Json(input): Json<FeedbackInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = state.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id).map_err(ApiError::Forbidden)?;
    let svc = FeedbackService::new(&db);
    let feedback = svc
        .add_feedback(&NewFeedback {
            project_id: Some(project_id),
            run_id: input.run_id,
            step_id: input.step_id,
            rating: input.rating,
            comment: input.comment,
            signal: input.signal,
        })
        .map_err(|e| ApiError::Internal(format!("Failed to add feedback: {e}")))?;
    Ok(Json(serde_json::to_value(feedback)?))
}

/// GET /projects/:id/feedback — list feedback for a project.
pub async fn list_feedback(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = state.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id).map_err(ApiError::Forbidden)?;
    let svc = FeedbackService::new(&db);
    let items = svc
        .list_feedback(&project_id)
        .map_err(|e| ApiError::Internal(format!("Failed to list feedback: {e}")))?;
    Ok(Json(serde_json::to_value(items)?))
}

// ---------------------------------------------------------------------------
// Analytics / improvement signals
// ---------------------------------------------------------------------------

/// GET /projects/:id/signals — detect improvement signals (tenant-scoped).
pub async fn detect_signals(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = state.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id).map_err(ApiError::Forbidden)?;
    let svc = AnalyticsService::new(&db);
    let signals = svc
        .detect_signals(Some(&project_id))
        .map_err(|e| ApiError::Internal(format!("Failed to detect signals: {e}")))?;
    Ok(Json(serde_json::to_value(signals)?))
}
