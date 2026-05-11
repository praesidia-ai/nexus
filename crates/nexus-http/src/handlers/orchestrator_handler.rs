//! Handlers for the Agent Orchestrator.

use std::sync::Arc;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::stream::Stream;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;

use crate::{
    agent_orchestrator::{AgentOrchestrator, OrchestratorEvent, OrchestratorTask},
    error::ApiResult,
    security::project_access::ProjectAccess,
    state::AppState,
};

/// POST /projects/:id/orchestrate — run multi-agent orchestrated task (SSE)
pub async fn orchestrate(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(task): Json<OrchestratorTask>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let project_id = access.project_id.clone();
    let project_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");

    let (tx, rx) = mpsc::channel::<OrchestratorEvent>(50);

    let tool_registry = Arc::new(crate::agents::tools::ToolRegistry::new());

    tokio::spawn(async move {
        let orchestrator = AgentOrchestrator::new(project_dir, app, tool_registry);
        orchestrator.execute(&task, tx).await;
    });

    let stream = async_stream::stream! {
        let mut rx = rx;
        while let Some(event) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&event) {
                yield Ok(Event::default().data(json));
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    )
}

/// POST /projects/:id/orchestrate/decompose — decompose task into subtasks
#[derive(Deserialize)]
pub struct DecomposeRequest {
    pub description: String,
}

pub async fn decompose_task(
    access: ProjectAccess,
    Json(request): Json<DecomposeRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let _project_id = access.project_id;
    let subtasks = AgentOrchestrator::decompose(&request.description);
    Ok(Json(json!({ "subtasks": subtasks })))
}
