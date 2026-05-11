//! Evolution Handler — endpoints for agent versioning, skill DNA, quality gate, scheduler.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    agent_versioning,
    error::{ApiError, ApiResult},
    quality_gate,
    security::auth::Scope,
    security::project_access::ProjectAccess,
    skill_dna,
    state::AppState,
};

// ---------------------------------------------------------------------------
// Agent Versioning
// ---------------------------------------------------------------------------

/// GET /evolution/agents/:role/versions — list all versions for an agent role.
pub async fn list_agent_versions(
    State(app): State<Arc<AppState>>,
    Path(role): Path<String>,
) -> ApiResult<Json<Value>> {
    let versions = agent_versioning::list_versions(&app, &role).await;
    Ok(Json(json!({ "versions": versions })))
}

/// GET /evolution/agents/:role/champion — get the current champion version.
pub async fn get_champion(
    State(app): State<Arc<AppState>>,
    Path(role): Path<String>,
) -> ApiResult<Json<Value>> {
    let champion = agent_versioning::get_champion(&app, &role).await;
    match champion {
        Some(v) => Ok(Json(json!({ "champion": v }))),
        None => Ok(Json(json!({ "champion": null, "message": "No champion yet" }))),
    }
}

#[derive(Deserialize)]
pub struct MutateRequest {
    pub mutation_type: String,
    pub description: String,
}

/// POST /evolution/agents/:role/mutate — create a mutated version.
pub async fn mutate_agent(
    State(app): State<Arc<AppState>>,
    Path(role): Path<String>,
    Json(body): Json<MutateRequest>,
) -> ApiResult<Json<Value>> {
    let mutation = match body.mutation_type.as_str() {
        "prompt_tweak" => agent_versioning::MutationType::PromptTweak,
        "tool_add" => agent_versioning::MutationType::ToolAdd,
        "tool_remove" => agent_versioning::MutationType::ToolRemove,
        "iteration_adj" => agent_versioning::MutationType::IterationAdj,
        "model_change" => agent_versioning::MutationType::ModelChange,
        "temperature_adj" => agent_versioning::MutationType::TemperatureAdj,
        _ => return Err(ApiError::BadRequest(format!(
            "Unknown mutation type: {}. Use: prompt_tweak, tool_add, tool_remove, iteration_adj, model_change, temperature_adj",
            body.mutation_type
        ))),
    };

    let id = agent_versioning::mutate_champion(&app, &role, mutation, &body.description)
        .await
        .map_err(ApiError::BadRequest)?;

    Ok(Json(json!({ "id": id, "status": "candidate" })))
}

/// POST /evolution/agents/:role/promote — check and promote candidates.
pub async fn check_promotions(
    State(app): State<Arc<AppState>>,
    Path(role): Path<String>,
) -> ApiResult<Json<Value>> {
    let promoted = agent_versioning::check_promotion(&app, &role).await;
    match promoted {
        Some(id) => Ok(Json(json!({ "promoted": true, "new_champion_id": id }))),
        None => Ok(Json(json!({ "promoted": false, "reason": "No candidate beats current champion" }))),
    }
}

// ---------------------------------------------------------------------------
// Skill DNA
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SkillQuery {
    pub intent: Option<String>,
    pub status: Option<String>,
}

/// GET /evolution/skills — list skills, optionally filtered by intent.
pub async fn list_skills(
    State(app): State<Arc<AppState>>,
    Query(query): Query<SkillQuery>,
) -> ApiResult<Json<Value>> {
    if let Some(intent) = &query.intent {
        let skills = skill_dna::get_skills_for_intent(&app, intent).await;
        Ok(Json(json!({ "skills": skills })))
    } else {
        // List all active skills
        let skills = skill_dna::get_skills_for_intent(&app, "general").await;
        Ok(Json(json!({ "skills": skills, "note": "Pass ?intent=bug_fix for filtered results" })))
    }
}

/// POST /evolution/skills/promote — promote ready draft skills.
pub async fn promote_skills(
    State(app): State<Arc<AppState>>,
) -> ApiResult<Json<Value>> {
    let count = skill_dna::promote_if_ready(&app).await;
    Ok(Json(json!({ "promoted_count": count })))
}

// ---------------------------------------------------------------------------
// Quality Gate
// ---------------------------------------------------------------------------

/// GET /projects/:id/quality-gate — get quality gate history.
pub async fn get_quality_gate(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<Value>> {
    let project_id = access.project_id.clone();
    let history = quality_gate::get_gate_history(&app, &project_id).await;
    Ok(Json(json!({ "history": history })))
}

#[derive(Deserialize)]
pub struct QualityGateRequest {
    pub min_taste_score: Option<u32>,
    pub max_retries: Option<u32>,
    pub auto_improve: Option<bool>,
}

/// POST /projects/:id/quality-gate/run — run quality gate on a project.
pub async fn run_quality_gate(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(body): Json<QualityGateRequest>,
) -> ApiResult<Json<Value>> {
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
        return Err(ApiError::BadRequest("No generated code found".into()));
    }

    let config = quality_gate::QualityGateConfig {
        min_taste_score: body.min_taste_score.unwrap_or(85),
        max_retries: body.max_retries.unwrap_or(3),
        auto_improve: body.auto_improve.unwrap_or(true),
        require_build_pass: true,
    };

    // Load the real intent for accurate analysis
    let intent = super::intelligence_handler::load_project_intent_pub(&app, &project_id).await
        .unwrap_or_else(|| crate::intent_engine::analyze_flat("web application"));

    let result = quality_gate::run_quality_gate(&app, &project_id, &project_dir, &config, &intent).await;

    Ok(Json(json!({ "result": result })))
}

// ---------------------------------------------------------------------------
// Scheduler
// ---------------------------------------------------------------------------

/// GET /evolution/scheduler — list all scheduled tasks.
pub async fn list_scheduled_tasks(
    State(app): State<Arc<AppState>>,
) -> ApiResult<Json<Value>> {
    let db = app.db.lock().await;
    let mut stmt = match db.prepare(
        "SELECT id, name, task_type, interval_secs, priority, conflict_group,
                status, last_run, last_result, last_error, run_count, fail_count
         FROM scheduled_tasks
         ORDER BY priority ASC"
    ) {
        Ok(s) => s,
        Err(e) => return Err(ApiError::Internal(format!("Query failed: {}", e))),
    };

    let rows: Vec<serde_json::Value> = stmt.query_map([], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "name": row.get::<_, String>(1)?,
            "task_type": row.get::<_, String>(2)?,
            "interval_secs": row.get::<_, i64>(3)?,
            "priority": row.get::<_, i64>(4)?,
            "conflict_group": row.get::<_, Option<String>>(5)?,
            "status": row.get::<_, String>(6)?,
            "last_run": row.get::<_, Option<String>>(7)?,
            "last_result": row.get::<_, Option<String>>(8)?,
            "last_error": row.get::<_, Option<String>>(9)?,
            "run_count": row.get::<_, i64>(10)?,
            "fail_count": row.get::<_, i64>(11)?,
        }))
    }).map_err(|e| ApiError::Internal(format!("Query failed: {}", e)))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Json(json!({ "tasks": rows })))
}
