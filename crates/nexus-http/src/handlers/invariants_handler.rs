use std::sync::Arc;

use axum::{extract::State, Json};
use serde_json::json;

use crate::{
    error::{ApiError, ApiResult},
    invariants,
    security::{auth::Scope, project_access::ProjectAccess},
    state::AppState,
};

/// GET /projects/:id/invariants — check all invariants
pub async fn check(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    access.require_scope(&Scope::ProjectRead).map_err(|_| {
        ApiError::Forbidden("project:read scope required".into())
    })?;
    let dir = app
        .data_dir
        .join("projects")
        .join(&access.project_id)
        .join("generated");
    if !dir.exists() {
        return Err(ApiError::BadRequest("No generated code".into()));
    }
    let result = invariants::check_invariants(&dir);
    Ok(Json(json!(result)))
}

/// POST /projects/:id/invariants/fix — auto-fix fixable violations
pub async fn auto_fix_endpoint(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    access.require_scope(&Scope::ProjectWrite).map_err(|_| {
        ApiError::Forbidden("project:write scope required".into())
    })?;
    let dir = app
        .data_dir
        .join("projects")
        .join(&access.project_id)
        .join("generated");
    if !dir.exists() {
        return Err(ApiError::BadRequest("No generated code".into()));
    }
    let before = invariants::check_invariants(&dir);
    let fixed = invariants::auto_fix(&dir, &before);
    let after = invariants::check_invariants(&dir);
    Ok(Json(json!({
        "fixed": fixed,
        "before": {"score": before.score, "violations": before.total_violations},
        "after":  {"score": after.score,  "violations": after.total_violations},
    })))
}
