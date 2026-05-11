//! HTTP handlers for the Wave Pipeline (50+ parallel mini-agents).

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

use nexus_store::ProjectService;

use crate::{
    coding_agents::{
        mini_agent,
        types::*,
        wave_orchestrator::{self, WaveEvent, WavePipelineConfig},
    },
    error::{ApiError, ApiResult},
    handlers::agent_run::{api_base_for_provider, default_model_for_provider},
    security::auth::Scope,
    security::project_access::ProjectAccess,
    state::AppState,
};

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RunWavePipelineReq {
    pub task: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    /// Which waves to run (null = all 7)
    pub waves: Option<Vec<u8>>,
    /// Agent IDs to skip
    pub skip_agents: Option<Vec<String>>,
    /// If set, ONLY run these agent IDs
    pub only_agents: Option<Vec<String>>,
    /// Max parallel agents per wave
    pub max_parallel: Option<usize>,
    /// Max total iterations
    pub max_total_iterations: Option<u32>,
    /// Stop on first fatal failure
    pub fail_fast: Option<bool>,
}

// ---------------------------------------------------------------------------
// POST /projects/:id/wave-pipeline/run — start the 50-agent wave pipeline
// ---------------------------------------------------------------------------

pub async fn run_wave_pipeline(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(body): Json<RunWavePipelineReq>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    access
        .require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;
    let project_id = access.project_id.clone();
    tracing::warn!(
        project_id = %project_id,
        "DEPRECATED: run_wave_pipeline is deprecated — use the unified agent runtime at \
         POST /projects/:id/agents/:agent_id/run with AgentRuntime::execute_parallel instead"
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

        // Caller's tenant wins over the global API key — see
        // `settings::llm_key_rows_for_read` for the precedence rules.
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
        mode: CodingMode::FullPipeline,
        constraints: Vec::new(),
        priority: TaskPriority::Normal,
        max_iterations: body.max_total_iterations.unwrap_or(500),
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

    let config = WavePipelineConfig {
        task,
        project_dir,
        llm_config,
        waves: body.waves,
        skip_agents: body.skip_agents.unwrap_or_default(),
        only_agents: body.only_agents,
        max_parallel: body.max_parallel,
        max_total_iterations: body.max_total_iterations.unwrap_or(500),
        fail_fast: body.fail_fast.unwrap_or(true),
        project_id: Some(project_id.clone()),
    };

    info!(
        project = %project_id,
        provider = %provider,
        model = %model,
        task_id = %task_id,
        "Starting wave pipeline"
    );

    // Per-tenant + global LLM concurrency. The wave pipeline can fan out
    // across 50 agents — this slot is held for the entire spawned run so
    // a single tenant can't trigger many simultaneous waves.
    if let Err(reason) = app.tenant_rate_limiter.check(&access.tenant_id, "agent") {
        return Err(ApiError::TooManyRequests(reason));
    }
    let llm_guard = app
        .rate_limiter
        .acquire_llm_slot()
        .await
        .map_err(|e| ApiError::TooManyRequests(format!("wave queue is full: {e}")))?;

    let (tx, mut rx) = mpsc::channel::<WaveEvent>(500);

    tokio::spawn(async move {
        let _slot = llm_guard;
        wave_orchestrator::run_wave_pipeline(config, app.clone(), tx).await;
    });

    let stream = async_stream::stream! {
        while let Some(event) = rx.recv().await {
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
// GET /projects/:id/wave-pipeline/agents — list all available mini-agents
// ---------------------------------------------------------------------------

pub async fn list_wave_agents(
    State(_app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let _project_id = access.project_id;
    let agents = mini_agent::build_registry();
    let waves = mini_agent::build_waves(&agents);

    Ok(Json(json!({
        "total_agents": agents.len(),
        "total_waves": waves.len(),
        "waves": waves,
        "agents": agents,
    })))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// `resolve_api_key` previously lived here and looked up only the global
// settings row; it has been replaced by
// `crate::handlers::agent_run::resolve_api_key_for_tenant`, which tries
// the caller's tenant-scoped row first and only falls back to the legacy
// global row / env vars.
