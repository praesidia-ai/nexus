//! HTTP endpoints for the Super Agents performance optimization system.

use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::json;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use crate::super_agents::types::{OrchestratorMode, SuperAgentKind};

/// GET /super-agents/status — orchestrator status and summary.
pub async fn get_status(
    State(app): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let orch = app
        .orchestrator
        .get()
        .ok_or_else(|| ApiError::Internal("Orchestrator not initialized".into()))?;
    let status = orch.status().await;
    Ok(Json(serde_json::to_value(status).unwrap_or_default()))
}

/// GET /super-agents/history — recent agent run history.
pub async fn get_history(
    State(app): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let orch = app
        .orchestrator
        .get()
        .ok_or_else(|| ApiError::Internal("Orchestrator not initialized".into()))?;
    let history = orch.history(50).await;
    Ok(Json(serde_json::to_value(history).unwrap_or_default()))
}

/// GET /super-agents/metrics — current system snapshot.
pub async fn get_metrics(
    State(app): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let bus = &app.metrics_bus;
    let snapshot = bus.snapshot().await;
    let names = bus.metric_names().await;
    Ok(Json(json!({
        "snapshot": snapshot,
        "total_samples": bus.total_samples(),
        "metric_count": names.len(),
        "metrics": names,
        "uptime_secs": bus.uptime_secs(),
    })))
}

#[derive(Deserialize)]
pub struct SetModeReq {
    pub mode: OrchestratorMode,
}

/// POST /super-agents/mode — change orchestrator mode.
pub async fn set_mode(
    State(app): State<Arc<AppState>>,
    Json(body): Json<SetModeReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let orch = app
        .orchestrator
        .get()
        .ok_or_else(|| ApiError::Internal("Orchestrator not initialized".into()))?;
    orch.set_mode(body.mode).await;
    Ok(Json(json!({"mode": body.mode})))
}

#[derive(Deserialize)]
pub struct PauseReq {
    pub paused: bool,
}

/// POST /super-agents/pause — pause or resume the orchestrator.
pub async fn set_pause(
    State(app): State<Arc<AppState>>,
    Json(body): Json<PauseReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let orch = app
        .orchestrator
        .get()
        .ok_or_else(|| ApiError::Internal("Orchestrator not initialized".into()))?;
    orch.set_paused(body.paused).await;
    Ok(Json(json!({"paused": body.paused})))
}

#[derive(Deserialize)]
pub struct TriggerReq {
    pub agent: String,
}

/// POST /super-agents/trigger — manually trigger a specific agent.
pub async fn trigger_agent(
    State(app): State<Arc<AppState>>,
    Json(body): Json<TriggerReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let orch = app
        .orchestrator
        .get()
        .ok_or_else(|| ApiError::Internal("Orchestrator not initialized".into()))?;

    let kind = parse_agent_kind(&body.agent)
        .ok_or_else(|| ApiError::BadRequest(format!("Unknown agent: {}", body.agent)))?;

    let record = orch
        .trigger_agent(kind)
        .await
        .ok_or_else(|| ApiError::NotFound("Agent not found in registry".into()))?;

    Ok(Json(serde_json::to_value(record).unwrap_or_default()))
}

/// GET /super-agents/agents — list all registered agents with their config.
pub async fn list_agents(
    State(_app): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let agents: Vec<serde_json::Value> = SuperAgentKind::all()
        .iter()
        .map(|kind| {
            json!({
                "kind": kind.as_str(),
                "priority": kind.default_priority(),
                "conflict_groups": kind.conflict_groups()
                    .iter()
                    .map(|g| format!("{:?}", g))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();

    Ok(Json(json!({"agents": agents})))
}

fn parse_agent_kind(name: &str) -> Option<SuperAgentKind> {
    match name {
        "latency_optimizer" => Some(SuperAgentKind::LatencyOptimizer),
        "concurrency_optimizer" => Some(SuperAgentKind::ConcurrencyOptimizer),
        "llm_cost_optimizer" => Some(SuperAgentKind::LlmCostOptimizer),
        "context_compressor" => Some(SuperAgentKind::ContextCompressor),
        "pipeline_bottleneck_detector" => Some(SuperAgentKind::PipelineBottleneckDetector),
        "cache_optimizer" => Some(SuperAgentKind::CacheOptimizer),
        "database_optimizer" => Some(SuperAgentKind::DatabaseOptimizer),
        "sse_stream_optimizer" => Some(SuperAgentKind::SseStreamOptimizer),
        "agent_efficiency_optimizer" => Some(SuperAgentKind::AgentEfficiencyOptimizer),
        "build_runtime_optimizer" => Some(SuperAgentKind::BuildRuntimeOptimizer),
        _ => None,
    }
}
