//! Execution endpoints — validate and apply proposals.

use std::sync::Arc;

use axum::{
    extract::State,
    Json,
};
use serde_json::json;

use crate::{
    error::{ApiError, ApiResult},
    execution_core::{ExecutionCore, Proposal},
    security::auth::Scope,
    security::project_access::ProjectAccess,
    state::AppState,
};

/// POST /projects/:id/execute/validate — validate a proposal without executing.
pub async fn validate_proposal(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(proposal): Json<Proposal>,
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
    let core = ExecutionCore::new(dir);
    let result = core.validate(&proposal);
    Ok(Json(json!(result)))
}

/// POST /projects/:id/execute/apply — validate + execute a proposal.
pub async fn apply_proposal(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(proposal): Json<Proposal>,
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
        return Err(ApiError::BadRequest("No generated code".into()));
    }
    let core = ExecutionCore::new(dir);

    // Validate first
    let validation = core.validate(&proposal);
    if !validation.valid {
        return Ok(Json(json!({
            "status": "rejected",
            "validation": validation,
        })));
    }

    // Execute
    let receipt = core.execute(&proposal).await;

    // Audit log
    {
        let db = app.db.lock().await;
        let obs = nexus_store::ObservabilityService::new(&db);
        for file in &receipt.files_changed {
            let _ = obs.audit(
                &project_id,
                "execution_core",
                "file_modify",
                Some(file),
                None,
                &validation.risk_level,
            );
        }
    }

    Ok(Json(json!({
        "status": receipt.status,
        "receipt": receipt,
        "validation": validation,
    })))
}
