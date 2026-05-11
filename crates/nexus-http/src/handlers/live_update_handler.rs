//! Handlers for the Live Incremental Update Engine.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::json;

use crate::{
    error::{ApiError, ApiResult},
    live_update::{LiveUpdateEngine, LiveUpdateRequest},
    security::auth::{AuthContext, Scope},
    security::project_access::ProjectAccess,
    security::tenant::validate_project_access,
    state::AppState,
};

/// POST /projects/:id/live-update — apply live update
pub async fn apply_update(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(request): Json<LiveUpdateRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    let project_id = access.project_id.clone();
    let project_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");
    if !project_dir.exists() {
        return Err(ApiError::BadRequest("No generated code".into()));
    }

    let engine = LiveUpdateEngine::new(project_dir);
    let result = engine.apply(&request, None).await;

    Ok(Json(json!(result)))
}

/// GET /projects/:id/live-update/history — list update history
pub async fn list_updates(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let project_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");

    let engine = LiveUpdateEngine::new(project_dir);
    let records = engine.list_updates();

    Ok(Json(json!({ "updates": records })))
}

/// POST /projects/:id/live-update/:update_id/rollback — rollback a specific update
pub async fn rollback_update(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, update_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    {
        let db = app.db.lock().await;
        validate_project_access(&db, &project_id, &auth.tenant_id)
            .map_err(ApiError::Forbidden)?;
    }
    let project_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");

    let engine = LiveUpdateEngine::new(project_dir);
    let restored = engine
        .rollback_update(&update_id)
        .map_err(ApiError::BadRequest)?;

    Ok(Json(json!({
        "rolled_back": true,
        "files_restored": restored,
    })))
}
