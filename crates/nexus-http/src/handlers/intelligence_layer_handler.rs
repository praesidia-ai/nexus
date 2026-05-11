//! Intelligence Layer handlers — causal learning, strategy, runtime observer, marketplace.

use std::sync::Arc;

use axum::{
    extract::State,
    Json,
};
use serde::Deserialize;

use crate::{
    security::project_access::ProjectAccess,
    state::AppState,
};

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct StrategyRequest {
    pub description: String,
}

#[derive(Deserialize)]
pub struct RuntimeEventRequest {
    pub event_type: String,
    pub severity: Option<u32>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct MarketplaceSearchRequest {
    pub query: String,
}

#[derive(Deserialize)]
pub struct PluginSuggestRequest {
    pub description: String,
}

// ---------------------------------------------------------------------------
// Causal Learning
// ---------------------------------------------------------------------------

/// GET /causal/graph — Get the full causal graph.
pub async fn get_causal_graph(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let graph = crate::causal_learning::build_graph(&state).await;
    Json(serde_json::json!(graph))
}

/// GET /causal/overrides — Get causal decision overrides.
pub async fn get_causal_overrides(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let overrides = crate::causal_learning::infer_best_decisions(&state).await;
    Json(serde_json::json!({ "overrides": overrides }))
}

// ---------------------------------------------------------------------------
// Strategy Engine
// ---------------------------------------------------------------------------

/// POST /projects/:id/strategy — Get strategic feature suggestions.
pub async fn get_strategy(
    State(state): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(req): Json<StrategyRequest>,
) -> Json<serde_json::Value> {
    let project_id = access.project_id.clone();
    let intent = crate::intent_engine::analyze_flat(&req.description);
    let plan = crate::strategy_engine::suggest_strategy(&state, &project_id, &intent).await;
    Json(serde_json::json!(plan))
}

/// POST /projects/:id/strategy/next — Get the single next best feature.
pub async fn next_best_feature(
    State(state): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(req): Json<StrategyRequest>,
) -> Json<serde_json::Value> {
    let project_id = access.project_id.clone();
    let intent = crate::intent_engine::analyze_flat(&req.description);
    let next = crate::strategy_engine::suggest_next_best_feature(&state, &project_id, &intent).await;
    Json(serde_json::json!({ "next_feature": next }))
}

// ---------------------------------------------------------------------------
// Runtime Observer
// ---------------------------------------------------------------------------

/// POST /projects/:id/observe — Record a runtime event.
pub async fn record_runtime_event(
    State(state): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(req): Json<RuntimeEventRequest>,
) -> Json<serde_json::Value> {
    let project_id = access.project_id.clone();
    let event_type = match req.event_type.as_str() {
        "api_call" => crate::runtime_observer::RuntimeEventType::ApiCall,
        "api_error" => crate::runtime_observer::RuntimeEventType::ApiError,
        "latency_spike" => crate::runtime_observer::RuntimeEventType::LatencySpike,
        "app_crash" => crate::runtime_observer::RuntimeEventType::AppCrash,
        "health_failure" => crate::runtime_observer::RuntimeEventType::HealthFailure,
        "build_failure" => crate::runtime_observer::RuntimeEventType::BuildFailure,
        _ => crate::runtime_observer::RuntimeEventType::ApiCall,
    };

    let event = crate::runtime_observer::RuntimeEvent {
        event_type,
        timestamp: chrono::Utc::now().to_rfc3339(),
        severity: req.severity.unwrap_or(1),
        project_id: project_id.clone(),
        metadata: req.metadata.unwrap_or(serde_json::json!({})),
    };

    crate::runtime_observer::record_event(&state, &event).await;
    Json(serde_json::json!({ "recorded": true }))
}

/// GET /projects/:id/health — Get runtime health report.
pub async fn runtime_health(
    State(state): State<Arc<AppState>>,
    access: ProjectAccess,
) -> Json<serde_json::Value> {
    let project_id = access.project_id.clone();
    let report = crate::runtime_observer::analyze_runtime_health(&state, &project_id).await;
    Json(serde_json::json!(report))
}

// ---------------------------------------------------------------------------
// Plugin Marketplace
// ---------------------------------------------------------------------------

/// GET /marketplace — List all marketplace plugins.
pub async fn list_marketplace(
    State(_state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    // REVIEW(audit): plugin_marketplace was removed as dead code. Use marketplace handler instead.
    Json(serde_json::json!({
        "plugins": [],
        "total": 0,
        "note": "Use /marketplace endpoints for plugin catalog"
    }))
}

/// POST /marketplace/search — Search marketplace.
pub async fn search_marketplace(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<MarketplaceSearchRequest>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "results": [],
        "query": req.query,
        "note": "Use /marketplace endpoints for plugin search"
    }))
}

/// POST /marketplace/suggest — Auto-suggest plugins based on intent.
pub async fn suggest_marketplace_plugins(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<PluginSuggestRequest>,
) -> Json<serde_json::Value> {
    let _intent = crate::intent_engine::analyze_flat(&req.description);
    Json(serde_json::json!({
        "suggestions": [],
        "count": 0,
        "note": "Use /marketplace endpoints for plugin suggestions"
    }))
}
