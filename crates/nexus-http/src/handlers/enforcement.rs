//! Handlers for the Invariant Enforcement System.

use std::sync::Arc;

use axum::{
    extract::State,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    error::{ApiError, ApiResult},
    invariant_enforcer::{
        EnforcementConfig, GateContext, InvariantEnforcer, store_enforcement_result,
    },
    security::auth::Scope,
    security::project_access::ProjectAccess,
    state::AppState,
};

/// GET /projects/:id/enforce — run enforcement gate
#[derive(Deserialize)]
pub struct EnforceQuery {
    pub gate: Option<String>,
}

pub async fn enforce_gate(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    axum::extract::Query(query): axum::extract::Query<EnforceQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");
    if !dir.exists() {
        return Err(ApiError::BadRequest("No generated code".into()));
    }

    let gate = match query.gate.as_deref() {
        Some("pre_commit") => GateContext::PreCommit,
        Some("pre_deploy") => GateContext::PreDeploy,
        Some("continuous") => GateContext::Continuous,
        _ => GateContext::PreDeploy,
    };

    let enforcer = InvariantEnforcer::load_for_project(&dir);
    let result = enforcer.enforce(&dir, gate);

    // Store in audit log
    store_enforcement_result(&app.db, &project_id, &result).await;

    Ok(Json(json!(result)))
}

/// GET /projects/:id/enforce/rules — list enforcement rules
pub async fn list_enforcement_rules(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");

    let enforcer = if dir.exists() {
        InvariantEnforcer::load_for_project(&dir)
    } else {
        InvariantEnforcer::with_defaults()
    };

    let rules = enforcer.list_rules(None);
    Ok(Json(json!({ "rules": rules })))
}

/// PUT /projects/:id/enforce/config — update enforcement config
pub async fn update_enforcement_config(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(config): Json<EnforcementConfig>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    let project_id = access.project_id.clone();
    let dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| ApiError::Internal(e.to_string()))?;
    }

    let enforcer = InvariantEnforcer::new(config);
    enforcer
        .save_config(&dir)
        .map_err(ApiError::Internal)?;

    Ok(Json(json!({ "status": "updated" })))
}
