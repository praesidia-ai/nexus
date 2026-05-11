//! Collaboration / Multiplayer HTTP handlers.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use nexus_store::CollabService;

use crate::{
    error::{ApiError, ApiResult},
    security::auth::Scope,
    security::project_access::ProjectAccess,
    state::AppState,
};

#[derive(Deserialize)]
pub struct CreateSessionReq {
    pub created_by: Option<String>,
}

#[derive(Deserialize)]
pub struct JoinReq {
    pub name: String,
    pub role: String,
    pub participant_type: Option<String>,
}

#[derive(Deserialize)]
pub struct ActivityReq {
    pub participant_id: String,
    pub activity_type: String,
    pub target: Option<String>,
    pub detail: Option<String>,
}

pub async fn create_session(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(body): Json<CreateSessionReq>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    let project_id = access.project_id.clone();
    let db = app.db.lock().await;
    let svc = CollabService::new(&db);
    let session = svc.create_session(&project_id, body.created_by.as_deref().unwrap_or("user"))?;
    Ok(Json(json!(session)))
}

pub async fn join_session(
    State(app): State<Arc<AppState>>,
    Path((_pid, session_id)): Path<(String, String)>,
    Json(body): Json<JoinReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    let svc = CollabService::new(&db);
    let p = svc.join_session(
        &session_id,
        &body.name,
        &body.role,
        body.participant_type.as_deref().unwrap_or("human"),
    )?;
    Ok(Json(json!(p)))
}

pub async fn list_participants(
    State(app): State<Arc<AppState>>,
    Path((_pid, session_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    let svc = CollabService::new(&db);
    Ok(Json(json!(svc.list_participants(&session_id)?)))
}

pub async fn post_activity(
    State(app): State<Arc<AppState>>,
    Path((_pid, session_id)): Path<(String, String)>,
    Json(body): Json<ActivityReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    let svc = CollabService::new(&db);
    let a = svc.post_activity(
        &session_id,
        &body.participant_id,
        &body.activity_type,
        body.target.as_deref(),
        body.detail.as_deref(),
    )?;
    Ok(Json(json!(a)))
}

pub async fn recent_activity(
    State(app): State<Arc<AppState>>,
    Path((_pid, session_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    let svc = CollabService::new(&db);
    Ok(Json(json!(svc.recent_activity(&session_id, 50)?)))
}

/// `GET /projects/:id/collab/:session_id/presence` — SSE stream of presence events.
///
/// Each connected client receives heartbeats with current participant list.
/// Clients must send periodic pings or the stream will close after 120s idle.
pub async fn presence_stream(
    State(app): State<Arc<AppState>>,
    Path((_pid, session_id)): Path<(String, String)>,
) -> axum::response::sse::Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>> {
    use axum::response::sse::{Event, KeepAlive};
    use futures::stream;
    use std::time::Duration;

    let session_id = session_id.clone();
    let app = app.clone();

    let stream = stream::unfold(0u64, move |tick| {
        let app = app.clone();
        let sid = session_id.clone();
        async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let participants = {
                let db = app.db.lock().await;
                let svc = CollabService::new(&db);
                svc.list_participants(&sid).unwrap_or_default()
            };
            let event = Event::default()
                .event("presence")
                .json_data(json!({
                    "tick": tick,
                    "participants": participants,
                    "count": participants.len(),
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }))
                .ok()?;
            Some((Ok(event), tick + 1))
        }
    });

    axum::response::sse::Sse::new(stream).keep_alive(KeepAlive::default())
}
