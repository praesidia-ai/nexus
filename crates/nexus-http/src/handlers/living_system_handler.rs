//! Living System HTTP handlers — runtime intelligence, predictions, autonomous mode.

use std::sync::Arc;

use axum::{
    extract::State,
    Json,
};
use serde::Deserialize;

use crate::{
    error::ApiError,
    security::auth::Scope,
    security::project_access::ProjectAccess,
    state::AppState,
};

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct PredictRequest {
    pub recent_messages: Option<Vec<String>>,
    pub project_id: Option<String>,
}

#[derive(Deserialize)]
pub struct ModelRouteRequest {
    pub task_type: String,
    pub complexity: Option<String>,
    pub is_retry: Option<bool>,
}

#[derive(Deserialize)]
pub struct AutonomousRequest {
    pub max_iterations: Option<u32>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /living/predict — Predict user's next actions.
pub async fn predict_next(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PredictRequest>,
) -> Json<serde_json::Value> {
    // Load real project state if project_id is provided
    let (has_code, app_running, taste_score) = if let Some(ref pid) = req.project_id {
        let db = state.db.lock().await;
        let running: bool = db
            .query_row(
                "SELECT COUNT(*) FROM app_instances WHERE project_id = ?1 AND status = 'running'",
                rusqlite::params![pid],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0) > 0;
        let taste: Option<u32> = db
            .query_row(
                "SELECT overall FROM taste_history WHERE project_id = ?1 ORDER BY created_at DESC LIMIT 1",
                rusqlite::params![pid],
                |r| r.get::<_, u32>(0),
            )
            .ok();
        drop(db);
        let gen_dir = state.data_dir.join("projects").join(pid).join("generated");
        (gen_dir.exists(), running, taste)
    } else {
        (false, false, None)
    };

    let ctx = crate::predictive_intent::PredictionContext {
        recent_messages: req.recent_messages.unwrap_or_default(),
        project_phase: if app_running { 3 } else if has_code { 2 } else { 1 },
        app_type: None,
        has_generated_code: has_code,
        app_running,
        taste_score,
        mutations_count: 0,
    };
    let prediction = crate::predictive_intent::predict(&ctx);
    Json(serde_json::json!(prediction))
}

/// POST /living/route-model — Select optimal model for a task.
pub async fn route_model(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ModelRouteRequest>,
) -> Json<serde_json::Value> {
    let task_type = match req.task_type.as_str() {
        "reasoning" => crate::model_router::TaskType::Reasoning,
        "codegen" => crate::model_router::TaskType::Codegen,
        "ui" => crate::model_router::TaskType::UiGeneration,
        "validation" => crate::model_router::TaskType::Validation,
        "retry" => crate::model_router::TaskType::Retry,
        "compression" => crate::model_router::TaskType::Compression,
        "agent" => crate::model_router::TaskType::AgentLoop,
        _ => crate::model_router::TaskType::Codegen,
    };

    let complexity = match req.complexity.as_deref() {
        Some("low") => crate::model_router::TaskComplexity::Low,
        Some("high") => crate::model_router::TaskComplexity::High,
        _ => crate::model_router::TaskComplexity::Medium,
    };

    let ctx = crate::model_router::RoutingContext {
        task_type,
        complexity,
        latency_target_ms: 0,
        budget_remaining_pct: 1.0,
        is_retry: req.is_retry.unwrap_or(false),
        preferred_provider: String::new(),
        has_anthropic: state.anthropic_api_key.is_some(),
        has_openai: !state.openai_api_key.is_empty(),
    };

    let selection = crate::model_router::route(&ctx);
    Json(serde_json::json!(selection))
}

/// POST /projects/:id/autonomous — Run autonomous improvement.
pub async fn run_autonomous(
    State(state): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(req): Json<AutonomousRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    let project_id = access.project_id.clone();
    let project_dir = state
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");

    let max_iterations = req.max_iterations.unwrap_or(5);

    let result = crate::autonomous_engine::run_autonomous(
        &state,
        &project_id,
        &project_dir,
        max_iterations,
    )
    .await;

    Ok(Json(serde_json::json!(result)))
}

/// GET /projects/:id/project-intelligence — Get persistent project intelligence.
pub async fn get_project_intelligence(
    State(state): State<Arc<AppState>>,
    access: ProjectAccess,
) -> Result<Json<serde_json::Value>, ApiError> {
    let project_id = access.project_id.clone();
    let project_dir = state
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");

    let intel = crate::project_brain::ProjectIntelligence::load(&project_dir);
    let context = intel.to_context();

    Ok(Json(serde_json::json!({
        "intelligence": intel,
        "context_preview": context,
    })))
}

/// GET /living/runtime-snapshot — Get adaptive runtime metrics (placeholder).
pub async fn runtime_snapshot(
    State(_state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    // The actual runtime is per-pipeline-execution, so we return system-level info
    Json(serde_json::json!({
        "status": "active",
        "capabilities": [
            "adaptive_model_switching",
            "context_compression",
            "retry_adjustment",
            "deterministic_fallback",
            "dynamic_parallelization"
        ],
        "thresholds": {
            "slow_step_ms": 20000,
            "max_consecutive_failures": 2,
            "token_budget_warn_pct": 0.2,
            "max_agent_iterations": 10
        }
    }))
}
