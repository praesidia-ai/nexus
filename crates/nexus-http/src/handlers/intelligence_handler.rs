//! Intelligence Handler — HTTP endpoints for the perception & intelligence layer.
//!
//! Endpoints:
//! - POST /intelligence/predict  — predictive preprocessing (partial input)
//! - POST /intelligence/explain  — full explain trace for a pipeline run
//! - GET  /projects/:id/post-build — post-build analysis
//! - POST /projects/:id/improve   — run continuous improvement cycle
//! - GET  /intelligence/personality/:profile — get personality config
//! - POST /intelligence/mode      — set execution mode

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    adaptive_control::{self, ExecutionMode},
    error::{ApiError, ApiResult},
    intent_engine,
    nexus_intelligence,
    personality::{self, Personality},
    security::auth::Scope,
    security::project_access::ProjectAccess,
    state::AppState,
};

// ---------------------------------------------------------------------------
// Predictive Preprocessing
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct PredictInput {
    pub partial_input: String,
}

/// POST /intelligence/predict — pre-process partial user input (mind reading).
///
/// Call this while the user types (debounce 300ms on frontend).
/// Returns predictions, smart suggestions, and auto-expanded prompt.
/// Cache persists on AppState — consumed by oneshot pipeline to skip redundant work.
pub async fn predict(
    State(app): State<Arc<AppState>>,
    Json(body): Json<PredictInput>,
) -> ApiResult<Json<Value>> {
    let result = app.predictor.predict(&body.partial_input).await;

    Ok(Json(json!({
        "updated": result.updated,
        "predictions": result.cache,
        "suggestions": result.suggestions,
        "expanded_prompt": result.expanded_prompt,
        "skeleton_paths": result.skeleton_paths,
        "compute_ms": result.compute_ms,
    })))
}

// ---------------------------------------------------------------------------
// Explain Trace
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ExplainInput {
    pub description: String,
}

/// POST /intelligence/explain — run full intelligence analysis and return explain trace.
///
/// Uses the unified `nexus_brain::analyze()` — same codepath as oneshot.
pub async fn explain_decisions(
    State(app): State<Arc<AppState>>,
    Json(body): Json<ExplainInput>,
) -> ApiResult<Json<Value>> {
    let brain_output = crate::nexus_brain::analyze(&app, &body.description).await;
    let report = brain_output.to_intelligence_report();

    Ok(Json(json!({
        "trace": report.explain_trace,
        "intent": report.intent,
        "architecture": report.architecture,
        "personality": report.personality_config,
        "hidden_requirements": brain_output.hidden_requirements,
        "risk_analysis": brain_output.risk_analysis,
        "agent_plan": brain_output.agent_plan,
        "execution_mode": {
            "suggested": report.execution_mode,
            "params": report.pipeline_params,
        },
        "from_cache": report.from_cache,
    })))
}

// ---------------------------------------------------------------------------
// Post-Build Intelligence
// ---------------------------------------------------------------------------

/// GET /projects/:id/post-build — run post-build analysis.
pub async fn post_build_analysis(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<Value>> {
    let project_id = access.project_id.clone();
    let project_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");

    if !project_dir.exists() {
        return Err(ApiError::BadRequest("No generated code found".into()));
    }

    let intent = load_project_intent_pub(&app, &project_id).await.unwrap_or_else(|| {
        intent_engine::analyze_flat("a web application")
    });

    let report = nexus_intelligence::post_build(&app, &project_id, &project_dir, &intent, false, false).await;

    Ok(Json(json!({
        "analysis": report.analysis,
    })))
}

// ---------------------------------------------------------------------------
// Continuous Improvement
// ---------------------------------------------------------------------------

/// POST /projects/:id/improve — run post-build analysis + improvement cycle.
pub async fn run_improvement(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
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

    let intent = load_project_intent_pub(&app, &project_id).await.unwrap_or_else(|| {
        intent_engine::analyze_flat("a web application")
    });

    let report = nexus_intelligence::post_build(&app, &project_id, &project_dir, &intent, true, false).await;

    Ok(Json(json!({
        "analysis": report.analysis,
        "improvement": report.improvement,
    })))
}

// ---------------------------------------------------------------------------
// Personality
// ---------------------------------------------------------------------------

/// GET /intelligence/personality/:profile — get personality configuration.
pub async fn get_personality(
    Path(profile_str): Path<String>,
) -> ApiResult<Json<Value>> {
    let profile = match profile_str.as_str() {
        "startup" => Personality::Startup,
        "enterprise" => Personality::Enterprise,
        "creative" => Personality::Creative,
        _ => return Err(ApiError::BadRequest(format!(
            "Unknown personality: {}. Use startup, enterprise, or creative.",
            profile_str
        ))),
    };

    let config = personality::configure(profile);

    Ok(Json(json!({
        "personality": config,
    })))
}

// ---------------------------------------------------------------------------
// Execution Mode
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ModeInput {
    pub mode: ExecutionMode,
    pub complexity: Option<String>,
}

/// POST /intelligence/mode — compute pipeline params for a mode.
pub async fn compute_mode(
    Json(body): Json<ModeInput>,
) -> ApiResult<Json<Value>> {
    let complexity = match body.complexity.as_deref() {
        Some("simple") => intent_engine::Complexity::Simple,
        Some("complex") => intent_engine::Complexity::Complex,
        _ => intent_engine::Complexity::Medium,
    };

    let params = adaptive_control::compute_params(body.mode, &complexity);

    Ok(Json(json!({
        "mode": body.mode,
        "params": params,
    })))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Try to load a project's stored intent from metadata (public for cross-handler use).
pub async fn load_project_intent_pub(
    app: &AppState,
    project_id: &str,
) -> Option<intent_engine::FlatIntent> {
    let metadata_path = app
        .data_dir
        .join("projects")
        .join(project_id)
        .join("intent.json");

    if let Ok(content) = std::fs::read_to_string(&metadata_path) {
        serde_json::from_str(&content).ok()
    } else {
        None
    }
}
