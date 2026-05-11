//! Unified agent HTTP endpoints.
//!
//! Replaces the fragmented agent endpoints with a single, consistent surface.
//! All agent types (coding, architect, debugger, reviewer, tester, etc.) are
//! accessible through the same API, differentiated by `role` or inline
//! `definition`.

use std::convert::Infallible;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use serde::Deserialize;

use crate::agents::{
    AgentContext, AgentDefinition, AgentRuntime, ToolRegistry,
};
use crate::error::ApiError;
use crate::state::AppState;

/// Request to run an agent.
#[derive(Debug, Deserialize)]
pub struct RunAgentRequest {
    /// The task/prompt for the agent.
    pub task: String,
    /// Agent definition (inline) — overrides `role` if both provided.
    pub definition: Option<AgentDefinition>,
    /// Role shortcut (e.g., "coder", "architect", "debugger").
    pub role: Option<String>,
    /// Additional context appended to the task prompt.
    #[serde(default)]
    pub context: String,
}

/// `POST /projects/:id/agents/:agent_id/run` — Execute a project-scoped agent (SSE stream).
pub async fn run_unified_agent(
    State(app): State<Arc<AppState>>,
    Path((project_id, _agent_id)): Path<(String, String)>,
    Json(body): Json<RunAgentRequest>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let definition = resolve_definition(&project_id, &body)?;

    let registry = Arc::new(ToolRegistry::new());
    let runtime = AgentRuntime::new(app.clone(), registry);

    let project_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");
    let context = AgentContext {
        project_id: Some(project_id.clone()),
        project_dir: Some(project_dir),
        context: body.context,
        history: Vec::new(),
    };

    let (mut rx, _handle) = runtime
        .execute(&definition, &body.task, context)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to start agent: {}", e)))?;

    let stream = async_stream::stream! {
        while let Some(event) = rx.recv().await {
            let data = serde_json::to_string(&event).unwrap_or_default();
            yield Ok::<_, Infallible>(Event::default().event("agent_event").data(data));
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// `POST /agents/run` — Run a global (non-project) agent (SSE stream).
pub async fn run_global_agent(
    State(app): State<Arc<AppState>>,
    Json(body): Json<RunAgentRequest>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let definition = resolve_definition("global", &body)?;

    let registry = Arc::new(ToolRegistry::new());
    let runtime = AgentRuntime::new(app.clone(), registry);

    let context = AgentContext {
        project_id: None,
        project_dir: None,
        context: body.context,
        history: Vec::new(),
    };

    let (mut rx, _handle) = runtime
        .execute(&definition, &body.task, context)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to start agent: {}", e)))?;

    let stream = async_stream::stream! {
        while let Some(event) = rx.recv().await {
            let data = serde_json::to_string(&event).unwrap_or_default();
            yield Ok::<_, Infallible>(Event::default().event("agent_event").data(data));
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// Resolve an [`AgentDefinition`] from the request body.
///
/// Priority: inline `definition` > `role` shortcut.
fn resolve_definition(project_id: &str, body: &RunAgentRequest) -> Result<AgentDefinition, ApiError> {
    if let Some(def) = &body.definition {
        return Ok(def.clone());
    }

    if let Some(role) = &body.role {
        let def = match role.as_str() {
            "coder" | "coding" => crate::agents::factories::coding_agent(project_id),
            "architect" => crate::agents::factories::architect_agent(project_id),
            "debugger" => crate::agents::factories::debugger_agent(project_id),
            "reviewer" => crate::agents::factories::reviewer_agent(project_id),
            "tester" => crate::agents::factories::tester_agent(project_id),
            "performance" => crate::agents::factories::performance_agent(project_id),
            "devops" => crate::agents::factories::devops_agent(project_id),
            "repair" => crate::agents::factories::repair_agent(project_id),
            "ux" => crate::agents::factories::ux_agent(project_id),
            _ => {
                return Err(ApiError::BadRequest(format!(
                    "Unknown agent role: '{}'. Valid roles: coder, architect, debugger, \
                     reviewer, tester, performance, devops, repair, ux",
                    role
                )));
            }
        };
        return Ok(def);
    }

    Err(ApiError::BadRequest(
        "Must provide either 'definition' or 'role'".into(),
    ))
}
