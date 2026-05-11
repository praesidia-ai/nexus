//! Agent skills handlers — CRUD for skill presets and agent-skill assignments.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use rusqlite::Connection;
use serde::Deserialize;

use nexus_store::{AgentSkillService, NewAgentSkill};

use crate::{
    error::{ApiError, ApiResult},
    security::auth::Scope,
    security::project_access::ProjectAccess,
    state::AppState,
};

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct AssignSkillReq {
    pub skill_id: String,
    #[serde(default)]
    pub priority: i32,
}

// ---------------------------------------------------------------------------
// Tenant-scoping helpers
// ---------------------------------------------------------------------------

/// Verify that `agent_id` belongs to `project_id`; reject otherwise so tenant
/// A cannot assign/unassign/read skills on tenant B's agents.
fn verify_agent_in_project(
    conn: &Connection,
    project_id: &str,
    agent_id: &str,
) -> ApiResult<()> {
    let mut stmt = conn
        .prepare("SELECT 1 FROM agent_definitions WHERE id = ?1 AND project_id = ?2 LIMIT 1")
        .map_err(|e| ApiError::Internal(format!("agent lookup failed: {e}")))?;
    let exists = stmt
        .exists(rusqlite::params![agent_id, project_id])
        .map_err(|e| ApiError::Internal(format!("agent lookup failed: {e}")))?;
    if !exists {
        return Err(ApiError::Forbidden(format!(
            "agent {agent_id} not in project {project_id}"
        )));
    }
    Ok(())
}

/// Verify `skill_id` belongs to `project_id` OR is a built-in (project_id
/// NULL). Rejects foreign project-scoped skills.
fn verify_skill_accessible(
    svc: &AgentSkillService<'_>,
    project_id: &str,
    skill_id: &str,
) -> ApiResult<()> {
    let skill = svc
        .get_skill(skill_id)
        .map_err(|e| ApiError::Internal(format!("skill lookup failed: {e}")))?
        .ok_or_else(|| ApiError::NotFound(format!("skill {skill_id} not found")))?;
    match &skill.project_id {
        Some(pid) if pid != project_id => Err(ApiError::Forbidden(
            "skill does not belong to this project".into(),
        )),
        _ => Ok(()), // None = builtin (readable by all), matching pid = owned
    }
}

// ---------------------------------------------------------------------------
// GET /projects/:id/skills — list all skills (builtins + project custom)
// ---------------------------------------------------------------------------

pub async fn list_skills(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let db = app.db.lock().await;
    let svc = AgentSkillService::new(&db);
    let skills = svc.list_skills(Some(&project_id))?;
    Ok(Json(serde_json::to_value(skills)?))
}

// ---------------------------------------------------------------------------
// POST /projects/:id/skills — create a custom skill
// ---------------------------------------------------------------------------

pub async fn create_skill(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(body): Json<NewAgentSkill>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    let project_id = access.project_id.clone();
    let db = app.db.lock().await;
    let svc = AgentSkillService::new(&db);
    let skill = svc.create_skill(Some(&project_id), &body)?;
    Ok(Json(serde_json::to_value(skill)?))
}

// ---------------------------------------------------------------------------
// PUT /projects/:id/skills/:skill_id — update a skill
// ---------------------------------------------------------------------------

pub async fn update_skill(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Path((_project_id, skill_id)): Path<(String, String)>,
    Json(body): Json<NewAgentSkill>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    let db = app.db.lock().await;
    let svc = AgentSkillService::new(&db);
    verify_skill_accessible(&svc, &access.project_id, &skill_id)?;
    let skill = svc
        .update_skill(&skill_id, &body)
        .map_err(|e| ApiError::BadRequest(format!("failed to update skill: {}", e)))?;
    Ok(Json(serde_json::to_value(skill)?))
}

// ---------------------------------------------------------------------------
// DELETE /projects/:id/skills/:skill_id — delete a custom skill
// ---------------------------------------------------------------------------

pub async fn delete_skill(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Path((_project_id, skill_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    let db = app.db.lock().await;
    let svc = AgentSkillService::new(&db);
    verify_skill_accessible(&svc, &access.project_id, &skill_id)?;
    svc.delete_skill(&skill_id)
        .map_err(|e| ApiError::BadRequest(format!("failed to delete skill: {}", e)))?;
    Ok(Json(serde_json::json!({"deleted": true})))
}

// ---------------------------------------------------------------------------
// GET /skills/builtins — list only built-in skills
// ---------------------------------------------------------------------------

pub async fn list_builtin_skills(
    State(app): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    let svc = AgentSkillService::new(&db);
    // Builtins have project_id = NULL, pass None to get only globals
    let skills = svc.list_skills(None)?;
    let builtins: Vec<_> = skills.into_iter().filter(|s| s.is_builtin).collect();
    Ok(Json(serde_json::to_value(builtins)?))
}

// ---------------------------------------------------------------------------
// POST /projects/:id/agents/:agent_id/skills — assign a skill to an agent
// ---------------------------------------------------------------------------

pub async fn assign_skill(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Path((_project_id, agent_id)): Path<(String, String)>,
    Json(body): Json<AssignSkillReq>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    let db = app.db.lock().await;
    verify_agent_in_project(&db, &access.project_id, &agent_id)?;
    let svc = AgentSkillService::new(&db);
    verify_skill_accessible(&svc, &access.project_id, &body.skill_id)?;
    let assignment = svc.assign_skill(&agent_id, &body.skill_id, body.priority)?;
    Ok(Json(serde_json::to_value(assignment)?))
}

// ---------------------------------------------------------------------------
// DELETE /projects/:id/agents/:agent_id/skills/:skill_id — unassign a skill
// ---------------------------------------------------------------------------

pub async fn unassign_skill(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Path((_project_id, agent_id, skill_id)): Path<(String, String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    let db = app.db.lock().await;
    verify_agent_in_project(&db, &access.project_id, &agent_id)?;
    let svc = AgentSkillService::new(&db);
    svc.unassign_skill(&agent_id, &skill_id)?;
    Ok(Json(serde_json::json!({"removed": true})))
}

// ---------------------------------------------------------------------------
// GET /projects/:id/agents/:agent_id/skills — list skills for an agent
// ---------------------------------------------------------------------------

pub async fn list_agent_skills(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Path((_project_id, agent_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    verify_agent_in_project(&db, &access.project_id, &agent_id)?;
    let svc = AgentSkillService::new(&db);
    let skills = svc.list_agent_skills(&agent_id)?;
    Ok(Json(serde_json::to_value(skills)?))
}

// ---------------------------------------------------------------------------
// GET /projects/:id/agents/:agent_id/prompt — preview composed prompt
// ---------------------------------------------------------------------------

pub async fn preview_agent_prompt(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Path((_project_id, agent_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    verify_agent_in_project(&db, &access.project_id, &agent_id)?;

    // Get the agent's base system prompt (already tenant-scoped).
    let ab = nexus_store::AgentBuilder::new(&db);
    let agents = ab.list_agents(&access.project_id)?;
    let agent = agents
        .into_iter()
        .find(|a| a.id == agent_id)
        .ok_or_else(|| ApiError::NotFound(format!("agent {} not found", agent_id)))?;

    let svc = AgentSkillService::new(&db);
    let composed = svc.compose_agent_prompt(&agent_id, &agent.system_prompt)?;
    let skills = svc.list_agent_skills(&agent_id)?;

    Ok(Json(serde_json::json!({
        "agent_id": agent_id,
        "agent_name": agent.name,
        "skills_count": skills.len(),
        "composed_prompt": composed,
    })))
}
