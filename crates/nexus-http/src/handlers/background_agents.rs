//! Background agent endpoints — long-running autonomous agents.

use std::sync::Arc;
use axum::{extract::{Path, State}, Json};
use serde::Deserialize;
use serde_json::json;
use nexus_store::BackgroundAgentService;
use crate::{
    error::{ApiError, ApiResult},
    security::auth::{AuthContext, Scope},
    security::project_access::ProjectAccess,
    security::tenant::validate_project_access,
    state::AppState,
};

#[derive(Deserialize)]
pub struct CreateBgAgentReq { pub agent_type: String, pub config: Option<serde_json::Value>, pub interval_secs: Option<i32> }

pub async fn list_bg_agents(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let db = app.db.lock().await;
    let svc = BackgroundAgentService::new(&db);
    Ok(Json(json!(svc.list(&project_id)?)))
}

pub async fn create_bg_agent(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(body): Json<CreateBgAgentReq>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    let project_id = access.project_id.clone();
    let db = app.db.lock().await;
    let svc = BackgroundAgentService::new(&db);
    let agent = svc.create(
        &project_id,
        &body.agent_type,
        &body.config.unwrap_or(json!({})),
        body.interval_secs.unwrap_or(3600),
    )?;
    Ok(Json(json!(agent)))
}

pub async fn toggle_bg_agent(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, agent_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let svc = BackgroundAgentService::new(&db);
    // Toggle: if running -> stop, if stopped -> start
    let agents = svc.list(&project_id)?;
    let agent = agents
        .iter()
        .find(|a| a.id == agent_id)
        .ok_or_else(|| ApiError::NotFound("Agent not found".into()))?;
    let new_status = if agent.status == "running" { "stopped" } else { "running" };
    svc.update_status(&agent_id, new_status)?;
    Ok(Json(json!({"id": agent_id, "status": new_status})))
}

pub async fn delete_bg_agent(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, agent_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let svc = BackgroundAgentService::new(&db);
    svc.delete(&agent_id)?;
    Ok(Json(json!({"deleted": true})))
}
