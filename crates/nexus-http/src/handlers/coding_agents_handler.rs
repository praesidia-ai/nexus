//! HTTP handlers for the Super Coding Agent System.

use std::convert::Infallible;
use std::sync::Arc;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;
use tracing::info;

use nexus_store::{GenerationLockService, ProjectService};

use crate::{
    coding_agents::{
        orchestrator::{self, OrchestratorConfig},
        types::*,
    },
    error::{ApiError, ApiResult},
    evolution::{optimizer, tracker::ImprovementTracker},
    handlers::agent_run::{api_base_for_provider, default_model_for_provider},
    security::auth::Scope,
    security::project_access::ProjectAccess,
    state::AppState,
};

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RunCodingAgentReq {
    pub task: String,
    pub mode: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub enable_review: Option<bool>,
    pub enable_test: Option<bool>,
    pub enable_verification: Option<bool>,
    pub enable_performance: Option<bool>,
    pub enable_ux: Option<bool>,
    pub enable_devops: Option<bool>,
    pub enable_refactor: Option<bool>,
    pub enable_product: Option<bool>,
    pub max_retries: Option<u32>,
    pub max_iterations: Option<u32>,
}

// ---------------------------------------------------------------------------
// POST /projects/:id/coding-agent/run — start the full coding pipeline (SSE)
// ---------------------------------------------------------------------------

pub async fn run_coding_agent(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(body): Json<RunCodingAgentReq>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    access
        .require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;
    let project_id = access.project_id.clone();
    tracing::warn!(
        project_id = %project_id,
        "DEPRECATED: run_coding_agent is deprecated — use the unified agent runtime at \
         POST /projects/:id/agents/:agent_id/run with role='coder' instead"
    );
    let project_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");

    if !project_dir.exists() {
        std::fs::create_dir_all(&project_dir)
            .map_err(|e| ApiError::Internal(format!("Failed to create project directory: {e}")))?;
    }

    let (provider, model, api_key, api_base) = {
        let db = app.db.lock().await;
        let svc = ProjectService::new(&db);

        let provider = body.provider.clone().unwrap_or_else(|| {
            svc.get_setting("llm.default.provider")
                .ok()
                .flatten()
                .unwrap_or_else(|| "ollama".to_string())
        });

        let model = body.model.clone().unwrap_or_else(|| {
            svc.get_setting("llm.default.model")
                .ok()
                .flatten()
                .unwrap_or_else(|| default_model_for_provider(&provider))
        });

        let api_key = crate::handlers::agent_run::resolve_api_key_for_tenant(
            &svc,
            Some(&access.tenant_id),
            &provider,
            &app,
        );
        let api_base = api_base_for_provider(&provider);

        (provider, model, api_key, api_base)
    };

    let task_id = uuid::Uuid::new_v4().to_string();

    let task = CodingTask {
        id: task_id.clone(),
        description: body.task.clone(),
        mode: match body.mode.as_deref() {
            Some("pipeline") => CodingMode::Pipeline,
            Some("full_pipeline") | Some("full") => CodingMode::FullPipeline,
            Some("architect") => CodingMode::Architect,
            Some("coder") => CodingMode::CoderOnly,
            Some("review") => CodingMode::ReviewOnly,
            Some("debug") => CodingMode::DebugOnly,
            Some("performance") => CodingMode::PerformanceOnly,
            Some("ux") => CodingMode::UxOnly,
            Some("devops") => CodingMode::DevOpsOnly,
            Some("refactor") => CodingMode::RefactorOnly,
            Some("product") => CodingMode::ProductOnly,
            _ => CodingMode::Pipeline,
        },
        constraints: Vec::new(),
        priority: TaskPriority::Normal,
        max_iterations: body.max_iterations.unwrap_or(30),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    let llm_config = CodingLlmConfig {
        provider: provider.clone(),
        model: model.clone(),
        api_key,
        api_base,
        max_tokens: 16384,
        temperature: 0.1,
    };

    let is_full = matches!(body.mode.as_deref(), Some("full_pipeline") | Some("full"));

    let config = OrchestratorConfig {
        task,
        project_dir: project_dir.clone(),
        llm_config,
        enable_review: body.enable_review.unwrap_or(true),
        enable_test: body.enable_test.unwrap_or(true),
        enable_verification: body.enable_verification.unwrap_or(true),
        enable_performance: body.enable_performance.unwrap_or(is_full),
        enable_ux: body.enable_ux.unwrap_or(is_full),
        enable_devops: body.enable_devops.unwrap_or(is_full),
        enable_refactor: body.enable_refactor.unwrap_or(is_full),
        enable_product: body.enable_product.unwrap_or(is_full),
        max_retries: body.max_retries.unwrap_or(3),
        project_id: Some(project_id.clone()),
    };

    info!(
        project = %project_id,
        provider = %provider,
        model = %model,
        task_id = %task_id,
        "Starting coding agent pipeline"
    );

    // Acquire per-project generation lock to prevent concurrent agent runs
    // from clobbering each other's file writes.
    {
        let db = app.db.lock().await;
        let lock_svc = GenerationLockService::new(&db);
        match lock_svc.try_acquire(&project_id, &body.task) {
            Ok(true) => {}
            Ok(false) => {
                return Err(ApiError::Conflict(
                    "A generation is already running for this project. \
                     Wait for it to finish or check /projects/:id/coding-agent/status."
                        .into(),
                ));
            }
            Err(e) => {
                tracing::warn!(error = %e, "Could not check generation lock, proceeding anyway");
            }
        }
    }

    // Per-tenant fair-share + global LLM concurrency slot. Held for the
    // lifetime of the spawned coding pipeline.
    if let Err(reason) = app.tenant_rate_limiter.check(&access.tenant_id, "agent") {
        return Err(ApiError::TooManyRequests(reason));
    }
    let llm_guard = app
        .rate_limiter
        .acquire_llm_slot()
        .await
        .map_err(|e| ApiError::TooManyRequests(format!("coding queue is full: {e}")))?;

    let (tx, mut rx) = mpsc::channel::<CodingEvent>(200);

    let app_bg = app.clone();
    let task_desc = body.task.clone();
    let pid = project_id.clone();

    tokio::spawn(async move {
        let _slot = llm_guard;
        let summary = orchestrator::run_coding_pipeline(config, app_bg.clone(), tx).await;

        // Release the generation lock now that the pipeline is done.
        {
            if let Ok(db) = app_bg.db.try_lock() {
                let lock_svc = GenerationLockService::new(&db);
                let _ = lock_svc.release(&pid);
            }
        }

        let tracker = ImprovementTracker::new(app_bg.data_dir.join("projects").join(&pid));
        let _ = tracker.record_execution(&summary, &task_desc, &pid);
    });

    // Fan-out to Agent TV — mirrors the wave-orchestrator bridge so the
    // classic pipeline path also populates per-agent thoughts, tool calls,
    // and token/cost tickers on the live grid.
    let fanout_app = app.clone();
    let fanout_pid = project_id.clone();
    let stream = async_stream::stream! {
        while let Some(event) = rx.recv().await {
            match &event {
                CodingEvent::Thinking { agent, content } => {
                    crate::handlers::live_build_handler::emit_agent_thought(
                        &fanout_app,
                        &fanout_pid,
                        agent.as_str(),
                        content.clone(),
                    )
                    .await;
                }
                CodingEvent::ToolCall { agent, tool, arguments } => {
                    crate::handlers::live_build_handler::emit_agent_tool_call(
                        &fanout_app,
                        &fanout_pid,
                        agent.as_str(),
                        tool,
                        arguments,
                    )
                    .await;
                }
                CodingEvent::LlmUsage {
                    agent,
                    tokens_in,
                    tokens_out,
                    cost_usd,
                } => {
                    crate::handlers::live_build_handler::emit_agent_tokens(
                        &fanout_app,
                        &fanout_pid,
                        agent.as_str(),
                        *tokens_in,
                        *tokens_out,
                        *cost_usd,
                    )
                    .await;
                }
                _ => {}
            }

            let json = serde_json::to_string(&event).unwrap_or_default();
            yield Ok::<_, Infallible>(Event::default().data(json));
        }
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    ))
}

// ---------------------------------------------------------------------------
// GET /projects/:id/coding-agent/status — get evolution state & metrics
// ---------------------------------------------------------------------------

pub async fn get_coding_status(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let data_dir = app.data_dir.join("projects").join(&project_id);
    let tracker = ImprovementTracker::new(data_dir);
    let state = tracker.load_state();
    let metrics = tracker.compute_metrics();

    Ok(Json(json!({
        "evolution_version": state.version,
        "total_executions": state.total_executions,
        "success_rate": state.success_rate,
        "avg_duration_ms": state.avg_duration_ms,
        "avg_iterations": state.avg_iterations,
        "patterns_count": state.patterns.len(),
        "improvements_count": state.improvements.len(),
        "metrics": metrics,
        "last_evolution_at": state.last_evolution_at,
    })))
}

// ---------------------------------------------------------------------------
// POST /projects/:id/coding-agent/evolve — trigger evolution cycle
// ---------------------------------------------------------------------------

pub async fn trigger_evolution(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    let project_id = access.project_id.clone();
    let data_dir = app.data_dir.join("projects").join(&project_id);
    let tracker = ImprovementTracker::new(data_dir);
    let result = optimizer::evolve(&tracker);

    Ok(Json(json!({
        "new_version": result.new_version,
        "patterns_detected": result.patterns_detected.len(),
        "improvements_proposed": result.improvements_proposed.len(),
        "improvements_applied": result.improvements_applied.len(),
        "patterns": result.patterns_detected,
        "improvements": result.improvements_proposed,
    })))
}

// ---------------------------------------------------------------------------
// GET /projects/:id/coding-agent/history — list execution records
// ---------------------------------------------------------------------------

pub async fn list_executions(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let data_dir = app.data_dir.join("projects").join(&project_id);
    let tracker = ImprovementTracker::new(data_dir);
    let records = tracker.load_records();

    let recent: Vec<_> = records.into_iter().rev().take(50).collect();

    Ok(Json(json!({
        "executions": recent,
        "count": recent.len(),
    })))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// Local `resolve_api_key` removed — see
// `crate::handlers::agent_run::resolve_api_key_for_tenant`, which scopes
// the lookup to the caller's tenant before falling back to global.
