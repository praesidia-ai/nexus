//! Live Build Mode — SSE stream of file generation events.
//!
//! Streams real-time events as code files are generated:
//! - `agent_action` — what agent is working and on what
//! - `file_created` — a new file was written
//! - `file_updated` — an existing file was modified
//! - `component_ready` — a UI component is ready to preview
//! - `build_step` — build pipeline step (install, compile, lint, etc.)
//! - `complete` — generation finished
//! - `error` — a recoverable error occurred

use std::sync::Arc;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::broadcast;
use tracing::info;

use crate::{
    error::ApiError,
    security::project_access::ProjectAccess,
    state::AppState,
};

// ---------------------------------------------------------------------------
// Event types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiveBuildEvent {
    AgentAction {
        agent: String,
        action: String,
        target: Option<String>,
    },
    FileCreated {
        path: String,
        language: String,
        lines: usize,
        component_type: Option<String>,
    },
    FileUpdated {
        path: String,
        change_summary: String,
    },
    ComponentReady {
        component: String,
        path: String,
        preview_hint: String,
    },
    BuildStep {
        step: String,
        status: String,
        duration_ms: Option<u64>,
    },
    Progress {
        percent: u32,
        message: String,
        files_created: usize,
        files_total: Option<usize>,
    },
    TasteScore {
        overall: u32,
        message: String,
    },
    /// Streamed text chunk attributed to a named agent. The UI concatenates.
    AgentThought {
        agent: String,
        content: String,
    },
    /// A tool (shell, file_write, search, ...) was invoked by a named agent.
    AgentToolCall {
        agent: String,
        tool: String,
        args_preview: String,
    },
    /// Incremental token + cost usage recorded for a named agent. These are
    /// delta values for a single LLM call — the Agent TV reducer accumulates.
    AgentTokens {
        agent: String,
        tokens_in: u64,
        tokens_out: u64,
        cost_usd: f64,
    },
    Complete {
        files_created: usize,
        duration_ms: u64,
        taste_score: Option<u32>,
    },
    Error {
        message: String,
        recoverable: bool,
    },
}

// ---------------------------------------------------------------------------
// Global build event bus
// ---------------------------------------------------------------------------

/// A per-project broadcast channel for live build events.
/// Projects publish to this; SSE connections consume from it.
pub type BuildEventBus = Arc<tokio::sync::RwLock<std::collections::HashMap<String, broadcast::Sender<LiveBuildEvent>>>>;

pub fn create_event_bus() -> BuildEventBus {
    Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()))
}

/// Emit a live build event for a project. No-op if no one is listening.
pub async fn emit_event(
    app: &Arc<AppState>,
    project_id: &str,
    event: LiveBuildEvent,
) {
    // Fast path: take a read lock, send the event. `broadcast::send` returns
    // `Err(SendError)` when the channel has no active receivers — at that
    // point the entry is dead weight in the HashMap and accumulates across
    // projects. Record "dead" so we can upgrade to a write lock and remove it.
    let dead = {
        let bus = app.build_event_bus.read().await;
        match bus.get(project_id) {
            Some(tx) => tx.send(event).is_err(),
            None => false,
        }
    };
    if dead {
        let mut bus = app.build_event_bus.write().await;
        if let Some(tx) = bus.get(project_id) {
            if tx.receiver_count() == 0 {
                bus.remove(project_id);
            }
        }
    }
}

/// Publish an [`AgentThought`] chunk for `agent` on `project_id`. No-op if no
/// one is listening. Safe to call from hot streaming paths.
pub async fn emit_agent_thought(
    app: &Arc<AppState>,
    project_id: &str,
    agent: &str,
    content: String,
) {
    if content.is_empty() {
        return;
    }
    emit_event(
        app,
        project_id,
        LiveBuildEvent::AgentThought {
            agent: agent.to_string(),
            content,
        },
    )
    .await;
}

/// Publish an [`AgentToolCall`] for `agent` on `project_id`. `args_preview` is
/// truncated to 200 chars before sending to keep the stream cheap.
pub async fn emit_agent_tool_call(
    app: &Arc<AppState>,
    project_id: &str,
    agent: &str,
    tool: &str,
    args: &serde_json::Value,
) {
    let preview_raw = if args.is_string() {
        args.as_str().unwrap_or("").to_string()
    } else {
        args.to_string()
    };
    let preview = if preview_raw.chars().count() > 200 {
        let mut s: String = preview_raw.chars().take(199).collect();
        s.push('…');
        s
    } else {
        preview_raw
    };
    emit_event(
        app,
        project_id,
        LiveBuildEvent::AgentToolCall {
            agent: agent.to_string(),
            tool: tool.to_string(),
            args_preview: preview,
        },
    )
    .await;
}

/// Publish an incremental [`AgentTokens`] sample for `agent` on `project_id`.
/// `cost_usd` is the incremental cost of this one LLM call.
pub async fn emit_agent_tokens(
    app: &Arc<AppState>,
    project_id: &str,
    agent: &str,
    tokens_in: u64,
    tokens_out: u64,
    cost_usd: f64,
) {
    emit_event(
        app,
        project_id,
        LiveBuildEvent::AgentTokens {
            agent: agent.to_string(),
            tokens_in,
            tokens_out,
            cost_usd,
        },
    )
    .await;
}

/// Remove entries from the build event bus that have no active receivers.
///
/// Called after generation completes so the bus does not accumulate
/// per-project channels indefinitely.
pub async fn evict_dead_channels(app: &Arc<AppState>) {
    let mut bus = app.build_event_bus.write().await;
    bus.retain(|_project_id, tx| tx.receiver_count() > 0);
}

// ---------------------------------------------------------------------------
// SSE handler
// ---------------------------------------------------------------------------

/// GET /projects/:id/live-build/stream — SSE stream of live build events
pub async fn stream_build_events(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> Result<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>, ApiError> {
    let project_id = access.project_id.clone();
    info!(project_id = %project_id, "Live build stream connected");

    // Register or join the channel for this project. 4096 absorbs token-burst
    // workloads where 256 used to lag instantly on slow consumers.
    let rx = {
        let mut bus = app.build_event_bus.write().await;
        let tx = bus
            .entry(project_id.clone())
            .or_insert_with(|| broadcast::channel(4096).0);
        tx.subscribe()
    };

    // Convert the broadcast receiver into an SSE stream
    let stream = stream::unfold(rx, move |mut rx| async move {
        match rx.recv().await {
            Ok(event) => {
                let data = serde_json::to_string(&event).unwrap_or_default();
                let event_type = event_type_name(&event);
                let sse = Event::default().event(event_type).data(data);
                Some((Ok(sse), rx))
            }
            Err(broadcast::error::RecvError::Lagged(_)) => {
                let sse = Event::default()
                    .event("lag")
                    .data(json!({"message": "stream lagged, some events missed"}).to_string());
                Some((Ok(sse), rx))
            }
            Err(broadcast::error::RecvError::Closed) => None,
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// POST /projects/:id/live-build/status — get current build status (non-streaming)
pub async fn get_build_status(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> axum::Json<serde_json::Value> {
    let project_id = access.project_id.clone();
    let bus = app.build_event_bus.read().await;
    let has_channel = bus.contains_key(&project_id);
    let listener_count = bus
        .get(&project_id)
        .map(|tx| tx.receiver_count())
        .unwrap_or(0);

    axum::Json(json!({
        "project_id": project_id,
        "active": has_channel,
        "listeners": listener_count,
    }))
}

fn event_type_name(e: &LiveBuildEvent) -> &'static str {
    match e {
        LiveBuildEvent::AgentAction { .. } => "agent_action",
        LiveBuildEvent::AgentThought { .. } => "agent_thought",
        LiveBuildEvent::AgentToolCall { .. } => "agent_tool_call",
        LiveBuildEvent::AgentTokens { .. } => "agent_tokens",
        LiveBuildEvent::FileCreated { .. } => "file_created",
        LiveBuildEvent::FileUpdated { .. } => "file_updated",
        LiveBuildEvent::ComponentReady { .. } => "component_ready",
        LiveBuildEvent::BuildStep { .. } => "build_step",
        LiveBuildEvent::Progress { .. } => "progress",
        LiveBuildEvent::TasteScore { .. } => "taste_score",
        LiveBuildEvent::Complete { .. } => "complete",
        LiveBuildEvent::Error { .. } => "error",
    }
}
