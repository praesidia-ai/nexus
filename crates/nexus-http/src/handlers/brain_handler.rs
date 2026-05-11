//! Brain HTTP handlers — exposes the unified Nexus Brain via REST API.

use std::sync::Arc;

use axum::{
    extract::State,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::nexus_brain;
use crate::security::project_access::ProjectAccess;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct AnalyzeRequest {
    pub description: String,
}

#[derive(Serialize)]
pub struct BrainAnalyzeResponse {
    pub success: bool,
    pub output: nexus_brain::BrainOutput,
}

#[derive(Serialize)]
pub struct TasteGateResponse {
    pub success: bool,
    pub result: crate::taste_gate::TasteGateResult,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /brain/analyze — Run the full brain analysis pipeline.
pub async fn analyze(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AnalyzeRequest>,
) -> Json<BrainAnalyzeResponse> {
    let output = nexus_brain::analyze(&state, &req.description).await;
    Json(BrainAnalyzeResponse {
        success: true,
        output,
    })
}

/// GET /brain/agents/:description — Auto-design agents for a description.
pub async fn design_agents(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<AnalyzeRequest>,
) -> Json<serde_json::Value> {
    let intent = crate::intent_engine::analyze_flat(&req.description);
    let agents = crate::agent_designer::design_agents(&intent, &req.description);
    Json(serde_json::json!({
        "agents": agents,
        "count": agents.len(),
    }))
}

/// POST /projects/:id/taste-gate — Run the taste gate on a project.
pub async fn run_taste_gate(
    State(state): State<Arc<AppState>>,
    access: ProjectAccess,
) -> Json<TasteGateResponse> {
    let project_id = access.project_id.clone();
    let project_dir = state
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");

    let result =
        crate::taste_gate::enforce_taste_gate(&state, &project_id, &project_dir, 90, 3).await;

    Json(TasteGateResponse {
        success: result.passed,
        result,
    })
}

/// GET /brain/hidden-requirements — Infer hidden requirements for a description.
pub async fn hidden_requirements(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<AnalyzeRequest>,
) -> Json<serde_json::Value> {
    let intent = crate::intent_engine::analyze_flat(&req.description);
    let hidden = crate::intent_engine::infer_flat_hidden_requirements(&intent, &req.description);
    Json(serde_json::json!({
        "intent": intent,
        "hidden_requirements": hidden,
    }))
}
