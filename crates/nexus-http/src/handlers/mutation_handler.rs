use crate::{
    error::{ApiError, ApiResult},
    mutation_engine,
    security::auth::Scope,
    security::project_access::ProjectAccess,
    state::AppState,
};
use axum::{extract::State, Json};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct MutateReq {
    pub change: String,
    pub target_file: Option<String>,
}

/// POST /projects/:id/mutate — apply an incremental change.
///
/// Tenant ownership of the project is enforced by `ProjectAccess`. The caller
/// must additionally hold `project:write` scope because this endpoint mutates
/// generated source files.
pub async fn mutate(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(body): Json<MutateReq>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    crate::input_limits::require_bounded(
        "change",
        &body.change,
        crate::input_limits::MAX_MUTATION_INSTRUCTION_BYTES,
    )?;
    let project_id = access.project_id.clone();
    let dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");
    if !dir.exists() {
        return Err(ApiError::BadRequest("No generated code".into()));
    }

    // Per-tenant + global rate-limit before any LLM IO. Mutations drive
    // LLM calls inside `mutation_engine::mutate`; without a slot the global
    // LLM concurrency budget is unprotected here.
    if let Err(reason) = app
        .tenant_rate_limiter
        .check(&access.tenant_id, "generation")
    {
        return Err(ApiError::TooManyRequests(reason));
    }
    let _llm_slot = app
        .rate_limiter
        .acquire_llm_slot()
        .await
        .map_err(|e| ApiError::TooManyRequests(format!("mutation queue is full: {e}")))?;

    let request = mutation_engine::MutationRequest {
        change: body.change,
        target_file: body.target_file,
    };
    let result = mutation_engine::mutate(&app, &project_id, &dir, &request)
        .await
        .map_err(ApiError::Internal)?;
    Ok(Json(serde_json::to_value(result)?))
}
