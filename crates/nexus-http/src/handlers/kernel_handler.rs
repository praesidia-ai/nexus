//! HTTP handlers for the nexus-kernel Agent OS subsystem.
//!
//! Covers:
//! - Process management (list, get, spawn, suspend, resume, kill)
//! - Reactive agents (list, register, unregister)
//! - Collaboration sessions (Layer 8)
//! - Federation peers (Layer 9)

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;

use nexus_kernel::{
    CollaborationManager, CollaborationMessage, FederationManager, FederationTrust, Participant,
    ParticipantRole, Priority, ReactiveAgentDefinition, ResourceAllocation,
};
use nexus_agents_core::definition::AgentDefinition as CoreAgentDefinition;

use crate::error::{ApiError, ApiResult};
use crate::security::auth::{AuthContext, Scope};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Lazy global singletons for the kernel managers (in-memory, no DB).
// In a production setup these would live inside AppState.
// ---------------------------------------------------------------------------

use std::sync::OnceLock;

fn collab_manager() -> &'static CollaborationManager {
    static INSTANCE: OnceLock<CollaborationManager> = OnceLock::new();
    INSTANCE.get_or_init(CollaborationManager::new)
}

fn federation_manager() -> &'static FederationManager {
    static INSTANCE: OnceLock<FederationManager> = OnceLock::new();
    INSTANCE.get_or_init(|| {
        let local_id =
            std::env::var("NEXUS_FEDERATION_ID").unwrap_or_else(|_| uuid::Uuid::new_v4().to_string());
        FederationManager::new(local_id)
    })
}

// ===========================================================================
// Process management
// ===========================================================================

/// GET /kernel/processes
pub async fn list_processes(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::ProjectRead)
        .map_err(|_| ApiError::Forbidden("project:read scope required".into()))?;
    let processes = app.scheduler.list_processes().await;
    let procs_json: Vec<serde_json::Value> = processes
        .iter()
        .map(|p| serde_json::to_value(p).unwrap_or_default())
        .collect();
    Ok(Json(json!({ "processes": procs_json })))
}

/// GET /kernel/processes/:pid
pub async fn get_process(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(pid): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::ProjectRead)
        .map_err(|_| ApiError::Forbidden("project:read scope required".into()))?;
    let info = app
        .scheduler
        .get_process(&pid)
        .await
        .map_err(|_| ApiError::NotFound(format!("process {pid} not found")))?;
    Ok(Json(serde_json::to_value(info).unwrap_or_default()))
}

#[derive(Deserialize)]
pub struct SpawnProcessReq {
    pub name: Option<String>,
    pub agent_def: Option<serde_json::Value>,
}

/// POST /kernel/processes
pub async fn spawn_process(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<SpawnProcessReq>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;
    let name = body.name.clone().unwrap_or_else(|| "unnamed".into());
    let agent_def = if let Some(def_json) = body.agent_def {
        serde_json::from_value::<CoreAgentDefinition>(def_json)
            .unwrap_or_else(|_| CoreAgentDefinition::default_with_name(&name))
    } else {
        CoreAgentDefinition::default_with_name(&name)
    };
    let pid = app
        .scheduler
        .spawn(
            agent_def,
            name.clone(),
            Priority::Normal,
            ResourceAllocation::default(),
            None,
        )
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to spawn process: {e}")))?;
    Ok(Json(json!({
        "pid": pid,
        "name": name,
        "state": "ready",
    })))
}

/// POST /kernel/processes/:pid/suspend
pub async fn suspend_process(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(pid): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;
    app.scheduler
        .suspend(&pid)
        .await
        .map_err(|e| ApiError::BadRequest(format!("Cannot suspend process: {e}")))?;
    Ok(Json(json!({ "pid": pid, "state": "suspended" })))
}

/// POST /kernel/processes/:pid/resume
pub async fn resume_process(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(pid): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;
    app.scheduler
        .resume(&pid)
        .await
        .map_err(|e| ApiError::BadRequest(format!("Cannot resume process: {e}")))?;
    Ok(Json(json!({ "pid": pid, "state": "running" })))
}

/// POST /kernel/processes/:pid/kill
pub async fn kill_process(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(pid): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;
    app.scheduler
        .kill(&pid, "Killed via API".into())
        .await
        .map_err(|e| ApiError::BadRequest(format!("Cannot kill process: {e}")))?;
    Ok(Json(json!({ "pid": pid, "state": "terminated" })))
}

// ===========================================================================
// Reactive agents
// ===========================================================================

/// GET /kernel/reactive
pub async fn list_reactive(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::ProjectRead)
        .map_err(|_| ApiError::Forbidden("project:read scope required".into()))?;
    let agents = app.reactive_manager.list().await;
    let agents_json: Vec<serde_json::Value> = agents
        .iter()
        .map(|a| serde_json::to_value(a).unwrap_or_default())
        .collect();
    Ok(Json(json!({ "reactive_agents": agents_json })))
}

#[derive(Deserialize)]
pub struct RegisterReactiveReq {
    pub name: String,
    pub definition: Option<serde_json::Value>,
}

/// POST /kernel/reactive
pub async fn register_reactive(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<RegisterReactiveReq>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;
    let definition = if let Some(def_json) = body.definition {
        serde_json::from_value::<ReactiveAgentDefinition>(def_json)
            .map_err(|e| ApiError::BadRequest(format!("Invalid reactive agent definition: {e}")))?
    } else {
        return Err(ApiError::BadRequest("definition field required".into()));
    };
    let id = app
        .reactive_manager
        .register(definition)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to register reactive agent: {e}")))?;
    Ok(Json(json!({
        "id": id,
        "name": body.name,
        "status": "registered",
    })))
}

/// DELETE /kernel/reactive/:name
pub async fn unregister_reactive(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(name): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;
    app.reactive_manager
        .unregister(&name)
        .await
        .map_err(|e| ApiError::NotFound(format!("Reactive agent not found: {e}")))?;
    Ok(Json(json!({
        "name": name,
        "status": "unregistered",
    })))
}

// ===========================================================================
// Collaboration (Layer 8)
// ===========================================================================

#[derive(Deserialize)]
pub struct CreateSessionReq {
    pub project_id: String,
    pub creator_user_id: String,
    pub creator_name: String,
}

/// POST /sessions
pub async fn create_session(
    State(_app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<CreateSessionReq>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;
    let creator = Participant {
        user_id: body.creator_user_id,
        name: body.creator_name,
        role: ParticipantRole::Owner,
        connected_at: Utc::now(),
        last_active: Utc::now(),
    };
    let session_id = collab_manager()
        .create_session(&body.project_id, creator)
        .await;
    Ok(Json(json!({ "session_id": session_id })))
}

/// GET /sessions/:id
pub async fn get_session(
    State(_app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::ProjectRead)
        .map_err(|_| ApiError::Forbidden("project:read scope required".into()))?;
    let session = collab_manager()
        .get_session(&id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("session {id} not found")))?;
    Ok(Json(serde_json::to_value(session).unwrap_or_default()))
}

/// Note: `user_id` is intentionally absent here — the participant identity is
/// derived from the authenticated [`AuthContext`] to prevent identity spoofing.
#[derive(Deserialize)]
pub struct JoinSessionReq {
    pub name: String,
    pub role: Option<String>,
}

/// POST /sessions/:id/join
pub async fn join_session(
    State(_app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(id): Path<String>,
    Json(body): Json<JoinSessionReq>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;
    let role = match body.role.as_deref() {
        Some("owner") => ParticipantRole::Owner,
        Some("editor") => ParticipantRole::Editor,
        Some("agent") => ParticipantRole::Agent,
        _ => ParticipantRole::Viewer,
    };
    let participant = Participant {
        user_id: auth.user_id.clone(),
        name: body.name,
        role,
        connected_at: Utc::now(),
        last_active: Utc::now(),
    };
    collab_manager()
        .join_session(&id, participant)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(Json(json!({ "status": "joined", "session_id": id })))
}

/// POST /sessions/:id/leave — body is optional/empty; identity is taken from auth.
pub async fn leave_session(
    State(_app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;
    collab_manager()
        .leave_session(&id, &auth.user_id)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(Json(json!({ "status": "left", "session_id": id })))
}

#[derive(Deserialize)]
pub struct SendMessageReq {
    #[serde(flatten)]
    pub message: CollaborationMessageInput,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CollaborationMessageInput {
    UserMessage { user_id: String, content: String },
    AgentMessage { pid: String, content: String },
    SystemMessage { content: String },
}

/// POST /sessions/:id/message
pub async fn send_message(
    State(_app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(id): Path<String>,
    Json(body): Json<SendMessageReq>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;
    let now = Utc::now();
    let msg = match body.message {
        CollaborationMessageInput::UserMessage { user_id, content } => {
            CollaborationMessage::UserMessage {
                user_id,
                content,
                timestamp: now,
            }
        }
        CollaborationMessageInput::AgentMessage { pid, content } => {
            CollaborationMessage::AgentMessage {
                pid,
                content,
                timestamp: now,
            }
        }
        CollaborationMessageInput::SystemMessage { content } => {
            CollaborationMessage::SystemMessage {
                content,
                timestamp: now,
            }
        }
    };
    collab_manager()
        .send_message(&id, msg)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(Json(json!({ "status": "sent", "session_id": id })))
}

// ===========================================================================
// Federation (Layer 9)
// ===========================================================================

/// GET /kernel/federation/peers
pub async fn list_peers(
    State(_app): State<Arc<AppState>>,
    auth: AuthContext,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::ProjectRead)
        .map_err(|_| ApiError::Forbidden("project:read scope required".into()))?;
    let peers = federation_manager().list_peers().await;
    Ok(Json(serde_json::to_value(peers).unwrap_or_default()))
}

#[derive(Deserialize)]
pub struct ConnectPeerReq {
    pub peer_url: String,
    pub trust: Option<String>,
}

/// POST /kernel/federation/connect
pub async fn connect_peer(
    State(_app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<ConnectPeerReq>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::SystemAdmin)
        .map_err(|_| ApiError::Forbidden("system:admin scope required for federation".into()))?;
    let trust = match body.trust.as_deref() {
        Some("full") => FederationTrust::Full,
        Some("agents_only") => FederationTrust::AgentsOnly,
        _ => FederationTrust::ReadOnly,
    };
    let peer_id = federation_manager()
        .connect(&body.peer_url, trust)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(Json(json!({ "peer_id": peer_id, "status": "connected" })))
}

/// DELETE /kernel/federation/:peer_id
pub async fn disconnect_peer(
    State(_app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(peer_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::SystemAdmin)
        .map_err(|_| ApiError::Forbidden("system:admin scope required for federation".into()))?;
    federation_manager()
        .disconnect(&peer_id)
        .await
        .map_err(|e| ApiError::NotFound(e.to_string()))?;
    Ok(Json(json!({ "peer_id": peer_id, "status": "disconnected" })))
}
