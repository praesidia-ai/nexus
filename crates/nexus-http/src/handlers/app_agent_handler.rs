//! HTTP handlers for the App-as-Agent platform.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};

use crate::app_agent::{AppAgentBinding, BindingStatus, generate_app_tools, infer_domain};
use crate::error::{ApiError, ApiResult};
use crate::security::auth::{AuthContext, Scope};
use crate::security::project_access::ProjectAccess;
use crate::security::tenant::validate_project_access;
use crate::state::AppState;

/// `GET /app-agents` — list all app-agent bindings (admin only).
pub async fn list_bindings(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::SystemAdmin)
        .map_err(|_| ApiError::Forbidden("system:admin scope required".into()))?;
    let bindings = app.app_agent_registry.list_all().await;
    Ok(Json(serde_json::json!({
        "bindings": bindings,
        "count": bindings.len(),
    })))
}

/// `GET /projects/:id/app-agents` — list bindings for a specific project.
pub async fn list_project_bindings(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let bindings = app.app_agent_registry.by_project(&project_id).await;
    Ok(Json(serde_json::json!({ "bindings": bindings })))
}

#[derive(Debug, serde::Deserialize)]
pub struct AttachRequest {
    /// Human-readable app name (defaults to project ID).
    pub app_name: Option<String>,
    /// Override domain detection.
    pub domain: Option<String>,
    /// App description — used for domain inference and agent personality.
    pub description: Option<String>,
}

/// `POST /projects/:id/app-agents` — auto-attach a business agent to a project app.
pub async fn attach_agent(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(body): Json<AttachRequest>,
) -> ApiResult<Json<AppAgentBinding>> {
    access
        .require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;
    let project_id = access.project_id.clone();
    let description = body.description.as_deref().unwrap_or("");
    let domain = body.domain.unwrap_or_else(|| infer_domain(description));
    let app_name = body.app_name.unwrap_or_else(|| project_id.clone());

    let base_url = std::env::var("NEXUS_PUBLIC_URL")
        .unwrap_or_else(|_| "http://localhost:8080".into());

    let tools = generate_app_tools(&domain, &project_id, &base_url);
    let agent_id = uuid::Uuid::new_v4().to_string();
    let skill_id = format!("app_{}", project_id);

    let binding = AppAgentBinding {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: project_id.clone(),
        app_name: app_name.clone(),
        domain: domain.clone(),
        agent_id: agent_id.clone(),
        agent_name: format!("{app_name} Business Agent"),
        tools,
        a2a_skill_id: skill_id,
        created_at: chrono::Utc::now(),
        status: BindingStatus::Active,
    };

    app.app_agent_registry.bind(binding.clone()).await;

    Ok(Json(binding))
}

/// `DELETE /projects/:project_id/app-agents/:binding_id` — detach the business agent.
pub async fn detach_agent(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, binding_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    {
        let db = app.db.lock().await;
        validate_project_access(&db, &project_id, &auth.tenant_id)
            .map_err(ApiError::Forbidden)?;
    }
    auth.require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;
    let removed = app.app_agent_registry.unbind(&binding_id).await;
    if removed {
        Ok(Json(serde_json::json!({ "detached": true, "binding_id": binding_id })))
    } else {
        Err(ApiError::NotFound(format!("binding {binding_id} not found")))
    }
}

/// `GET /projects/:project_id/app-agents/:binding_id/tools` — list app-aware tools for the binding.
pub async fn list_tools(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, binding_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    {
        let db = app.db.lock().await;
        validate_project_access(&db, &project_id, &auth.tenant_id)
            .map_err(ApiError::Forbidden)?;
    }
    let binding = app
        .app_agent_registry
        .get(&binding_id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("binding {binding_id} not found")))?;
    // Ensure the binding actually belongs to the project in the URL.
    if binding.project_id != project_id {
        return Err(ApiError::NotFound(format!("binding {binding_id} not found")));
    }

    Ok(Json(serde_json::json!({
        "tools": binding.tools,
        "count": binding.tools.len(),
        "domain": binding.domain,
    })))
}

#[derive(Debug, serde::Deserialize)]
pub struct InvokeToolRequest {
    pub tool_id: String,
    pub params: serde_json::Value,
    pub agent_message: Option<String>,
}

/// `POST /projects/:project_id/app-agents/:binding_id/invoke` — invoke an app tool via the bound agent.
pub async fn invoke_tool(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, binding_id)): Path<(String, String)>,
    Json(body): Json<InvokeToolRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    {
        let db = app.db.lock().await;
        validate_project_access(&db, &project_id, &auth.tenant_id)
            .map_err(ApiError::Forbidden)?;
    }
    auth.require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;
    let binding = app
        .app_agent_registry
        .get(&binding_id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("binding {binding_id} not found")))?;
    if binding.project_id != project_id {
        return Err(ApiError::NotFound(format!("binding {binding_id} not found")));
    }

    if binding.status != BindingStatus::Active {
        return Err(ApiError::BadRequest("binding is not active".into()));
    }

    let tool = binding
        .tools
        .iter()
        .find(|t| t.id == body.tool_id)
        .ok_or_else(|| ApiError::NotFound(format!("tool {} not found", body.tool_id)))?;

    // If the tool has an HTTP endpoint, forward to it
    if let Some(endpoint) = &tool.endpoint {
        let resp = app
            .http_client
            .post(endpoint)
            .json(&body.params)
            .send()
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;

        let status = resp.status();
        let result: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);

        return Ok(Json(serde_json::json!({
            "tool_id": body.tool_id,
            "status": status.as_u16(),
            "result": result,
        })));
    }

    // Otherwise, stub a successful acknowledgement
    Ok(Json(serde_json::json!({
        "tool_id": body.tool_id,
        "result": "acknowledged",
        "params": body.params,
    })))
}
