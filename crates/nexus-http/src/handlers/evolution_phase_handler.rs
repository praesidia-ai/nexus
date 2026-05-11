//! Evolution Phase handlers — collective intelligence, anticipation, prompt evolution.

use std::sync::Arc;

use axum::{
    extract::State,
    Json,
};
use serde::Deserialize;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CouncilRequest {
    pub description: String,
}

#[derive(Deserialize)]
pub struct AnticipateRequest {
    pub recent_messages: Option<Vec<String>>,
    pub project_id: Option<String>,
    pub action_history: Option<Vec<ActionRecord>>,
}

#[derive(Deserialize)]
pub struct ActionRecord {
    pub action_type: String,
    pub target: String,
    pub timestamp_ms: u64,
}

#[derive(Deserialize)]
pub struct EvolvePromptRequest {
    pub prompt: String,
    pub purpose: String,
}

// ---------------------------------------------------------------------------
// Collective Intelligence
// ---------------------------------------------------------------------------

/// POST /council/deliberate — Run the Agent Council on a task.
pub async fn council_deliberate(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CouncilRequest>,
) -> Json<serde_json::Value> {
    let intent = crate::intent_engine::analyze_flat(&req.description);
    let arch = crate::decision_engine::decide_with_learning(&intent, &state).await;

    let decision = crate::collective_intelligence::deliberate(&intent, &arch.0, &req.description);

    Json(serde_json::json!(decision))
}

// ---------------------------------------------------------------------------
// Anticipation Engine
// ---------------------------------------------------------------------------

/// POST /anticipate — Analyze behavior and anticipate next actions.
pub async fn anticipate(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<AnticipateRequest>,
) -> Json<serde_json::Value> {
    let ctx = crate::predictive_intent::PredictionContext {
        recent_messages: req.recent_messages.unwrap_or_default(),
        project_phase: 2,
        app_type: None,
        has_generated_code: req.project_id.is_some(),
        app_running: false,
        taste_score: None,
        mutations_count: 0,
    };

    let history: Vec<crate::anticipation_engine::UserAction> = req
        .action_history
        .unwrap_or_default()
        .into_iter()
        .map(|a| crate::anticipation_engine::UserAction {
            action_type: a.action_type,
            target: a.target,
            timestamp_ms: a.timestamp_ms,
        })
        .collect();

    let report = crate::anticipation_engine::anticipate(&ctx, &history);

    Json(serde_json::json!(report))
}

// ---------------------------------------------------------------------------
// Prompt Evolution
// ---------------------------------------------------------------------------

/// POST /prompts/evolve — Evolve a prompt based on historical data.
pub async fn evolve_prompt(
    State(state): State<Arc<AppState>>,
    Json(req): Json<EvolvePromptRequest>,
) -> Json<serde_json::Value> {
    let (evolved, changes) =
        crate::prompt_evolution::evolve_prompt(&state, &req.prompt, &req.purpose).await;

    Json(serde_json::json!({
        "original_length": req.prompt.len(),
        "evolved_length": evolved.len(),
        "changes": changes,
        "evolved_prompt": evolved,
    }))
}

/// GET /prompts/performance — Get prompt performance summary.
pub async fn prompt_performance(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let summary = crate::prompt_evolution::get_performance_summary(&state).await;
    Json(serde_json::json!({ "prompts": summary }))
}
