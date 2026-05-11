//! Team management HTTP handlers.
//!
//! Endpoints for creating, listing, and executing multi-agent teams.
//! Team execution streams events via SSE.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::SystemTime;

use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::error::{ApiError, ApiResult};
use crate::security::auth::{AuthContext, Scope};
use crate::state::AppState;
use crate::team_orchestrator::{TeamEvent, TeamOrchestrator};
use nexus_agents_core::teams::*;

/// Request to create a new team definition.
#[derive(Debug, Deserialize)]
pub struct CreateTeamRequest {
    /// The full team definition.
    pub team: TeamDefinition,
}

/// Request to execute a team on a task.
#[derive(Debug, Deserialize)]
pub struct RunTeamRequest {
    /// The task description for the team to work on.
    pub task: String,
}

/// Request to inject a human message into a running team.
#[derive(Debug, Deserialize)]
pub struct InjectMessageRequest {
    /// The message content.
    pub message: String,
    /// Target member ID or "broadcast".
    pub target: String,
}

/// Response for team listing and retrieval.
#[derive(Debug, Serialize)]
pub struct TeamResponse {
    /// The team definition.
    pub team: TeamDefinition,
}

/// Response for listing available templates.
#[derive(Debug, Serialize)]
pub struct TeamTemplateResponse {
    /// Template identifier.
    pub id: String,
    /// Template name.
    pub name: String,
    /// Template description.
    pub description: String,
    /// The team definition (without agent definitions filled in).
    pub template: TeamDefinition,
}

/// `POST /teams` — Create a new team definition.
///
/// Stores the team definition in the database. Returns the created team.
pub async fn create_team(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(req): Json<CreateTeamRequest>,
) -> ApiResult<Json<TeamResponse>> {
    auth.require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;
    let team = req.team;

    if team.members.is_empty() {
        return Err(ApiError::BadRequest(
            "Team must have at least one member".into(),
        ));
    }

    let team_json = serde_json::to_string(&team)
        .map_err(|e| ApiError::Internal(format!("Failed to serialize team: {e}")))?;

    let db = app.db.lock().await;
    db.execute(
        "INSERT OR REPLACE INTO teams (id, name, definition_json) VALUES (?1, ?2, ?3)",
        rusqlite::params![team.id, team.name, team_json],
    )
    .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    Ok(Json(TeamResponse { team }))
}

/// `GET /teams` — List all team definitions.
pub async fn list_teams(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
) -> ApiResult<Json<Vec<TeamResponse>>> {
    auth.require_scope(&Scope::ProjectRead)
        .map_err(|_| ApiError::Forbidden("project:read scope required".into()))?;
    let db = app.db.lock().await;

    let mut stmt = db
        .prepare("SELECT definition_json FROM teams ORDER BY name")
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    let teams: Vec<TeamResponse> = stmt
        .query_map([], |row| {
            let json_str: String = row.get(0)?;
            Ok(json_str)
        })
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?
        .filter_map(|r| r.ok())
        .filter_map(|json_str| {
            serde_json::from_str::<TeamDefinition>(&json_str)
                .ok()
                .map(|team| TeamResponse { team })
        })
        .collect();

    Ok(Json(teams))
}

/// `GET /teams/:id` — Get a specific team definition.
pub async fn get_team(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(team_id): Path<String>,
) -> ApiResult<Json<TeamResponse>> {
    auth.require_scope(&Scope::ProjectRead)
        .map_err(|_| ApiError::Forbidden("project:read scope required".into()))?;
    let db = app.db.lock().await;

    let json_str: String = db
        .query_row(
            "SELECT definition_json FROM teams WHERE id = ?1",
            rusqlite::params![team_id],
            |row| row.get(0),
        )
        .map_err(|_| ApiError::NotFound(format!("Team {team_id} not found")))?;

    let team: TeamDefinition = serde_json::from_str(&json_str)
        .map_err(|e| ApiError::Internal(format!("Failed to parse team: {e}")))?;

    Ok(Json(TeamResponse { team }))
}

/// `POST /teams/:id/run` — Execute a team on a task, streaming events via SSE.
///
/// Returns a server-sent event stream with [`TeamEvent`]s. The team
/// execution runs in the background; the stream stays open until the
/// team completes or fails.
pub async fn run_team(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(team_id): Path<String>,
    Json(req): Json<RunTeamRequest>,
) -> ApiResult<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>> {
    auth.require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;

    // Per-tenant fair-share before kicking off a team run; the orchestrator
    // can spawn many internal LLM calls so this is the right gating point.
    if let Err(reason) = app.tenant_rate_limiter.check(&auth.tenant_id, "agent") {
        return Err(ApiError::TooManyRequests(reason));
    }

    let (_run_id, rx) = spawn_team_run(app.clone(), &team_id, &req.task, &auth.tenant_id).await?;

    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(event) => {
            let json = serde_json::to_string(&event).unwrap_or_default();
            Some(Ok(Event::default().data(json)))
        }
        Err(_) => None,
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

// ---------------------------------------------------------------------------
// Persistent team runs
// ---------------------------------------------------------------------------

/// Starts a team run: inserts a `team_runs` row, kicks off the orchestrator in
/// the background, registers it in `team_run_registry` keyed by run_id, and
/// spawns a collector task that updates the row on completion.
///
/// Returns the new `run_id` and a broadcast receiver for live events. The
/// caller decides whether to stream those events to the client (SSE) or
/// discard them (the collector task holds its own subscription).
async fn spawn_team_run(
    app: Arc<AppState>,
    team_id: &str,
    task: &str,
    tenant_id: &str,
) -> ApiResult<(String, tokio::sync::broadcast::Receiver<TeamEvent>)> {
    // Load the team definition.
    let team = {
        let db = app.db.lock().await;
        let json_str: String = db
            .query_row(
                "SELECT definition_json FROM teams WHERE id = ?1",
                rusqlite::params![team_id],
                |row| row.get(0),
            )
            .map_err(|_| ApiError::NotFound(format!("Team {team_id} not found")))?;

        serde_json::from_str::<TeamDefinition>(&json_str)
            .map_err(|e| ApiError::Internal(format!("Failed to parse team: {e}")))?
    };

    // Allocate the run_id first so the durable event sink can be wired
    // into the orchestrator's bus + workspace (ADR-002 §4).
    let run_id = uuid::Uuid::new_v4().to_string();
    let started_at = chrono::Utc::now().to_rfc3339();
    let started_at_ms = chrono::Utc::now().timestamp_millis();

    // Register the run in `team_run_state` so subsequent `append`/`put_state`
    // calls have a parent row to update.
    let sink = Arc::new(crate::team_event_sink::TeamEventSink::new(
        app.db.clone(),
        run_id.clone(),
    ));
    sink.register_run(team_id, /*project_id*/ "", tenant_id, started_at_ms)
        .await
        .map_err(|e| ApiError::Internal(format!("team_run_state register: {e}")))?;

    let orchestrator = Arc::new(TeamOrchestrator::new_with_sink(app.clone(), sink.clone()));
    let event_rx = orchestrator
        .run(&team, task)
        .await
        .map_err(|e| ApiError::Internal(format!("Team execution failed: {e}")))?;

    // Persist the legacy run record (status/result/etc).
    {
        let db = app.db.lock().await;
        db.execute(
            "INSERT INTO team_runs (id, team_id, task, status, started_at) \
             VALUES (?1, ?2, ?3, 'running', ?4)",
            rusqlite::params![run_id, team_id, task, started_at],
        )
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    }

    // Register the orchestrator so HITL inject + SSE stream can reach it.
    {
        let mut registry = app.team_run_registry.write().await;
        registry.retain(|_, orc| !orc.is_finished());
        registry.insert(run_id.clone(), Arc::clone(&orchestrator));
    }

    // Spawn a collector task that listens for the terminal event and writes
    // the final snapshot back to the database. It holds its own subscription
    // so it works even if no client is streaming.
    let collector_rx = orchestrator.subscribe_events();
    let app_bg = app.clone();
    let run_id_bg = run_id.clone();
    let start_instant = SystemTime::now();
    tokio::spawn(async move {
        collect_and_finalize(app_bg, run_id_bg, collector_rx, start_instant).await;
    });

    Ok((run_id, event_rx))
}

/// Drains events for a run, collecting messages/artifacts, and writes the
/// final snapshot to `team_runs` when a terminal event is seen (or the
/// stream closes).
async fn collect_and_finalize(
    app: Arc<AppState>,
    run_id: String,
    mut rx: tokio::sync::broadcast::Receiver<TeamEvent>,
    start: SystemTime,
) {
    let mut messages: Vec<serde_json::Value> = Vec::new();
    let mut artifacts: Vec<serde_json::Value> = Vec::new();
    let mut final_status: Option<&'static str> = None;
    let mut final_cost: f32 = 0.0;
    let mut summary = String::new();

    loop {
        match rx.recv().await {
            Ok(event) => match &event {
                TeamEvent::MessageSent {
                    from,
                    to,
                    content_type,
                    summary: msg_summary,
                } => {
                    messages.push(serde_json::json!({
                        "id": uuid::Uuid::new_v4().to_string(),
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                        "from": from,
                        "to": to,
                        "type": content_type,
                        "content": msg_summary,
                    }));
                }
                TeamEvent::ArtifactPublished {
                    artifact_id,
                    name,
                    by,
                } => {
                    artifacts.push(serde_json::json!({
                        "id": artifact_id,
                        "name": name,
                        "type": "text",
                        "content": "",
                        "createdBy": by,
                        "createdAt": chrono::Utc::now().to_rfc3339(),
                        "size": 0,
                    }));
                }
                TeamEvent::TeamCompleted { cost_usd, .. } => {
                    final_status = Some("completed");
                    final_cost = *cost_usd;
                    break;
                }
                TeamEvent::TeamFailed { reason, .. } => {
                    final_status = Some("failed");
                    summary = reason.clone();
                    break;
                }
                _ => {}
            },
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(
                    run_id = %run_id,
                    skipped,
                    "team-run collector lagged behind broadcast — events dropped"
                );
                continue;
            }
        }
    }

    let status = final_status.unwrap_or("failed");
    let duration_secs = start.elapsed().map(|d| d.as_secs() as i64).unwrap_or(0);
    let completed_at = chrono::Utc::now().to_rfc3339();
    let result_json = serde_json::json!({
        "messages": messages,
        "artifacts": artifacts,
        "cost_breakdown": [],
    })
    .to_string();

    let message_count = messages.len() as i64;
    let artifact_count = artifacts.len() as i64;

    {
        let db = app.db.lock().await;
        if let Err(e) = db.execute(
            "UPDATE team_runs SET status = ?1, completed_at = ?2, duration_secs = ?3, \
             total_cost_usd = ?4, message_count = ?5, artifact_count = ?6, summary = ?7, \
             result_json = ?8 WHERE id = ?9",
            rusqlite::params![
                status,
                completed_at,
                duration_secs,
                final_cost as f64,
                message_count,
                artifact_count,
                summary,
                result_json,
                run_id
            ],
        ) {
            tracing::error!(run_id, error = %e, "failed to finalize team_runs row");
        }
    }

    // Drop the orchestrator from the in-memory registry once it's terminal.
    // Without this, finished orchestrators (each holding a 256-event broadcast
    // buffer + agent state) accumulate forever in low-traffic deployments
    // until the next start_run triggers the `retain` sweep.
    {
        let mut registry = app.team_run_registry.write().await;
        registry.remove(&run_id);
    }
}

/// A single persisted team run as returned to the UI.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamRunView {
    pub id: String,
    pub team_id: String,
    pub task: String,
    pub status: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub duration: i64,
    pub cost: f64,
    pub artifact_count: i64,
    pub message_count: i64,
}

fn row_to_run_view(row: &rusqlite::Row<'_>) -> rusqlite::Result<TeamRunView> {
    Ok(TeamRunView {
        id: row.get(0)?,
        team_id: row.get(1)?,
        task: row.get(2)?,
        status: row.get(3)?,
        started_at: row.get(4)?,
        completed_at: row.get(5)?,
        duration: row.get(6)?,
        cost: row.get(7)?,
        artifact_count: row.get(8)?,
        message_count: row.get(9)?,
    })
}

const RUN_COLS: &str = "id, team_id, task, status, started_at, completed_at, \
     duration_secs, total_cost_usd, artifact_count, message_count";

/// `POST /teams/:id/runs` — Start a run and return its handle. The run
/// continues in the background; clients tail progress via `GET /teams/runs/:id/stream`.
pub async fn start_run(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(team_id): Path<String>,
    Json(req): Json<RunTeamRequest>,
) -> ApiResult<Json<TeamRunView>> {
    auth.require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;

    // Per-tenant fair-share before starting a team run. (No global LLM slot
    // here — same reasoning as `run_team`: a team orchestrator can run for
    // minutes/hours and shouldn't hold the global slot for that duration;
    // gating belongs at each LLM call site inside the orchestrator.)
    if let Err(reason) = app.tenant_rate_limiter.check(&auth.tenant_id, "agent") {
        return Err(ApiError::TooManyRequests(reason));
    }

    let (run_id, _rx) = spawn_team_run(app.clone(), &team_id, &req.task, &auth.tenant_id).await?;

    let db = app.db.lock().await;
    let run = db
        .query_row(
            &format!("SELECT {RUN_COLS} FROM team_runs WHERE id = ?1"),
            rusqlite::params![run_id],
            row_to_run_view,
        )
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    Ok(Json(run))
}

/// `GET /teams/:id/runs` — list all runs for a team, newest first.
pub async fn list_runs(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(team_id): Path<String>,
) -> ApiResult<Json<Vec<TeamRunView>>> {
    auth.require_scope(&Scope::ProjectRead)
        .map_err(|_| ApiError::Forbidden("project:read scope required".into()))?;

    let db = app.db.lock().await;
    let mut stmt = db
        .prepare(&format!(
            "SELECT {RUN_COLS} FROM team_runs WHERE team_id = ?1 ORDER BY started_at DESC"
        ))
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    let runs: Vec<TeamRunView> = stmt
        .query_map(rusqlite::params![team_id], row_to_run_view)
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?
        .filter_map(Result::ok)
        .collect();

    Ok(Json(runs))
}

/// `GET /teams/runs/:run_id` — fetch one run by id.
pub async fn get_run(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(run_id): Path<String>,
) -> ApiResult<Json<TeamRunView>> {
    auth.require_scope(&Scope::ProjectRead)
        .map_err(|_| ApiError::Forbidden("project:read scope required".into()))?;

    let db = app.db.lock().await;
    let run = db
        .query_row(
            &format!("SELECT {RUN_COLS} FROM team_runs WHERE id = ?1"),
            rusqlite::params![run_id],
            row_to_run_view,
        )
        .map_err(|_| ApiError::NotFound(format!("Run {run_id} not found")))?;

    Ok(Json(run))
}

/// Full result view matching `TeamRunResult` in `web/src/lib/team-api.ts`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamRunResultView {
    pub run_id: String,
    pub team_id: String,
    pub task: String,
    pub status: String,
    pub duration: i64,
    pub total_cost: f64,
    pub cost_breakdown: Vec<serde_json::Value>,
    pub artifacts: Vec<serde_json::Value>,
    pub messages: Vec<serde_json::Value>,
    pub summary: String,
}

/// `GET /teams/runs/:run_id/result` — full result payload for a completed run.
pub async fn get_run_result(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(run_id): Path<String>,
) -> ApiResult<Json<TeamRunResultView>> {
    auth.require_scope(&Scope::ProjectRead)
        .map_err(|_| ApiError::Forbidden("project:read scope required".into()))?;

    let db = app.db.lock().await;
    let (team_id, task, status, duration, total_cost, summary, result_json): (
        String,
        String,
        String,
        i64,
        f64,
        String,
        Option<String>,
    ) = db
        .query_row(
            "SELECT team_id, task, status, duration_secs, total_cost_usd, summary, result_json \
             FROM team_runs WHERE id = ?1",
            rusqlite::params![run_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .map_err(|_| ApiError::NotFound(format!("Run {run_id} not found")))?;

    let (messages, artifacts, cost_breakdown) = match result_json
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
    {
        Some(v) => (
            v.get("messages")
                .and_then(|m| m.as_array())
                .cloned()
                .unwrap_or_default(),
            v.get("artifacts")
                .and_then(|a| a.as_array())
                .cloned()
                .unwrap_or_default(),
            v.get("cost_breakdown")
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default(),
        ),
        None => (Vec::new(), Vec::new(), Vec::new()),
    };

    Ok(Json(TeamRunResultView {
        run_id,
        team_id,
        task,
        status,
        duration,
        total_cost,
        cost_breakdown,
        artifacts,
        messages,
        summary,
    }))
}

/// `GET /teams/runs/:run_id/stream` — SSE stream of a run's events.
///
/// If the run is still live, subscribes to the orchestrator's broadcast bus.
/// If the run has finished, replays a synthetic terminal event from storage
/// so the client UI can render the final state without a second request.
pub async fn stream_run(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(run_id): Path<String>,
) -> ApiResult<
    Sse<std::pin::Pin<Box<dyn tokio_stream::Stream<Item = Result<Event, Infallible>> + Send>>>,
> {
    auth.require_scope(&Scope::ProjectRead)
        .map_err(|_| ApiError::Forbidden("project:read scope required".into()))?;

    let live = {
        let registry = app.team_run_registry.read().await;
        registry.get(&run_id).cloned()
    };

    if let Some(orc) = live {
        let rx = orc.subscribe_events();
        let stream = BroadcastStream::new(rx).filter_map(|result| match result {
            Ok(event) => {
                let json = serde_json::to_string(&event).unwrap_or_default();
                Some(Ok(Event::default().data(json)))
            }
            Err(_) => None,
        });
        let boxed: std::pin::Pin<
            Box<dyn tokio_stream::Stream<Item = Result<Event, Infallible>> + Send>,
        > = Box::pin(stream);
        return Ok(Sse::new(boxed).keep_alive(KeepAlive::default()));
    }

    // Run is finished — replay a single terminal event from storage so the
    // UI can render the final state on a late connect.
    let db = app.db.lock().await;
    let (status, result_json): (String, Option<String>) = db
        .query_row(
            "SELECT status, result_json FROM team_runs WHERE id = ?1",
            rusqlite::params![run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| ApiError::NotFound(format!("Run {run_id} not found")))?;
    drop(db);

    let event_type = if status == "completed" {
        "completed"
    } else {
        "failed"
    };
    let data = serde_json::json!({
        "type": event_type,
        "data": result_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .unwrap_or(serde_json::json!({})),
    })
    .to_string();

    let once = tokio_stream::once(Ok::<_, Infallible>(Event::default().data(data)));
    let boxed: std::pin::Pin<
        Box<dyn tokio_stream::Stream<Item = Result<Event, Infallible>> + Send>,
    > = Box::pin(once);
    Ok(Sse::new(boxed).keep_alive(KeepAlive::default()))
}

/// `DELETE /teams/:id` — remove a team definition (runs are retained).
pub async fn delete_team(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(team_id): Path<String>,
) -> ApiResult<axum::http::StatusCode> {
    auth.require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;

    let db = app.db.lock().await;
    let affected = db
        .execute(
            "DELETE FROM teams WHERE id = ?1",
            rusqlite::params![team_id],
        )
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if affected == 0 {
        return Err(ApiError::NotFound(format!("Team {team_id} not found")));
    }
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// `POST /teams/:id/runs/:run_id/inject` — Inject a human message into a running team.
///
/// This is used for HITL interactions where the human provides guidance
/// to a running team.
pub async fn inject_message(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((_team_id, run_id)): Path<(String, String)>,
    Json(req): Json<InjectMessageRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;

    let orchestrator = {
        let registry = app.team_run_registry.read().await;
        registry.get(&run_id).cloned()
    };

    match orchestrator {
        Some(orc) => {
            orc.inject_message(&req.message, &req.target).await;
            Ok(Json(serde_json::json!({
                "status": "injected",
                "run_id": run_id,
                "message": req.message,
                "target": req.target,
            })))
        }
        None => Err(ApiError::NotFound(format!(
            "Run {run_id} is not active. It may have already finished."
        ))),
    }
}

/// `POST /teams/runs/:run_id/pause` — Pause a running team.
///
/// New members wait at the gate; in-flight members keep running. The orchestrator
/// stays in the registry until [`resume_run`] is called or the run terminates.
pub async fn pause_run(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(run_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;

    let orchestrator = {
        let registry = app.team_run_registry.read().await;
        registry.get(&run_id).cloned()
    };

    match orchestrator {
        Some(orc) => {
            orc.pause();
            Ok(Json(serde_json::json!({
                "status": "paused",
                "run_id": run_id,
            })))
        }
        None => Err(ApiError::NotFound(format!(
            "Run {run_id} is not active. It may have already finished."
        ))),
    }
}

/// `POST /teams/runs/:run_id/resume` — Resume a paused team run.
pub async fn resume_run(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(run_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;

    let orchestrator = {
        let registry = app.team_run_registry.read().await;
        registry.get(&run_id).cloned()
    };

    match orchestrator {
        Some(orc) => {
            orc.resume();
            Ok(Json(serde_json::json!({
                "status": "resumed",
                "run_id": run_id,
            })))
        }
        None => Err(ApiError::NotFound(format!(
            "Run {run_id} is not active. It may have already finished."
        ))),
    }
}

/// `GET /teams/templates` — List pre-built team templates.
///
/// Templates provide starting points for common team configurations.
pub async fn list_templates() -> Json<Vec<TeamTemplateResponse>> {
    Json(build_templates())
}

/// Build the pre-defined team templates.
fn build_templates() -> Vec<TeamTemplateResponse> {
    use nexus_agents_core::definition::{AgentDefinition, ExecutionMode};

    let placeholder_agent = |id: &str, name: &str, prompt: &str| AgentDefinition {
        id: id.to_string(),
        name: name.to_string(),
        system_prompt: prompt.to_string(),
        skills: Vec::new(),
        tools: vec![
            "read_file".to_string(),
            "write_file".to_string(),
            "search".to_string(),
        ],
        model_preference: None,
        execution_mode: ExecutionMode::Interactive,
        max_iterations: 20,
        timeout_secs: 300,
    };

    vec![
        TeamTemplateResponse {
            id: "full-stack".to_string(),
            name: "Full-Stack Development Team".to_string(),
            description: "Lead architect + frontend specialist + backend specialist + code reviewer. Hierarchical coordination.".to_string(),
            template: TeamDefinition {
                id: "full-stack".to_string(),
                name: "Full-Stack Team".to_string(),
                description: "Builds full-stack features with architecture review.".to_string(),
                members: vec![
                    TeamMember {
                        id: "architect".to_string(),
                        role: TeamRole::Lead,
                        agent: placeholder_agent("architect", "Architect", "You are a senior software architect."),
                        responsibilities: vec!["Design system architecture".to_string(), "Decompose tasks".to_string()],
                        can_delegate_to: vec!["frontend".to_string(), "backend".to_string()],
                        can_veto: true,
                    },
                    TeamMember {
                        id: "frontend".to_string(),
                        role: TeamRole::Specialist { domain: "frontend".to_string() },
                        agent: placeholder_agent("frontend", "Frontend Dev", "You are a frontend specialist using React and TypeScript."),
                        responsibilities: vec!["Build UI components".to_string(), "Implement pages".to_string()],
                        can_delegate_to: vec![],
                        can_veto: false,
                    },
                    TeamMember {
                        id: "backend".to_string(),
                        role: TeamRole::Specialist { domain: "backend".to_string() },
                        agent: placeholder_agent("backend", "Backend Dev", "You are a backend specialist using Node.js and PostgreSQL."),
                        responsibilities: vec!["Build APIs".to_string(), "Design database schema".to_string()],
                        can_delegate_to: vec![],
                        can_veto: false,
                    },
                    TeamMember {
                        id: "reviewer".to_string(),
                        role: TeamRole::Reviewer,
                        agent: placeholder_agent("reviewer", "Code Reviewer", "You are a meticulous code reviewer focused on quality and security."),
                        responsibilities: vec!["Review all code changes".to_string(), "Check for security issues".to_string()],
                        can_delegate_to: vec![],
                        can_veto: true,
                    },
                ],
                coordination: CoordinationProtocol::Hierarchical { lead_id: "architect".to_string() },
                budget: TeamBudget {
                    max_total_cost_usd: 10.0,
                    max_cost_per_member_usd: 3.0,
                    max_total_tokens: 200_000,
                    max_duration_secs: 600,
                    max_iterations_per_member: 20,
                },
                hitl: HitlConfig {
                    enabled: true,
                    checkpoints: vec![HitlCheckpoint::AfterPlanning, HitlCheckpoint::BeforeCommit],
                },
                completion: CompletionCriteria {
                    all_tasks_done: true,
                    lead_approval_required: true,
                    all_reviews_passed: true,
                    completion_phrase: None,
                },
            },
        },
        TeamTemplateResponse {
            id: "code-review".to_string(),
            name: "Code Review Pipeline".to_string(),
            description: "Sequential pipeline: analyzer -> reviewer -> fixer. Each step builds on the previous.".to_string(),
            template: TeamDefinition {
                id: "code-review".to_string(),
                name: "Review Pipeline".to_string(),
                description: "Automated code analysis, review, and fix pipeline.".to_string(),
                members: vec![
                    TeamMember {
                        id: "analyzer".to_string(),
                        role: TeamRole::Specialist { domain: "analysis".to_string() },
                        agent: placeholder_agent("analyzer", "Code Analyzer", "You analyze code for patterns, anti-patterns, and improvement opportunities."),
                        responsibilities: vec!["Analyze code structure".to_string()],
                        can_delegate_to: vec![],
                        can_veto: false,
                    },
                    TeamMember {
                        id: "reviewer".to_string(),
                        role: TeamRole::Reviewer,
                        agent: placeholder_agent("reviewer", "Senior Reviewer", "You perform thorough code reviews with constructive feedback."),
                        responsibilities: vec!["Review and provide feedback".to_string()],
                        can_delegate_to: vec![],
                        can_veto: true,
                    },
                    TeamMember {
                        id: "fixer".to_string(),
                        role: TeamRole::Specialist { domain: "refactoring".to_string() },
                        agent: placeholder_agent("fixer", "Code Fixer", "You implement code fixes and refactoring based on review feedback."),
                        responsibilities: vec!["Apply fixes".to_string()],
                        can_delegate_to: vec![],
                        can_veto: false,
                    },
                ],
                coordination: CoordinationProtocol::Sequential {
                    order: vec!["analyzer".to_string(), "reviewer".to_string(), "fixer".to_string()],
                },
                budget: TeamBudget {
                    max_total_cost_usd: 5.0,
                    max_cost_per_member_usd: 2.0,
                    max_total_tokens: 100_000,
                    max_duration_secs: 300,
                    max_iterations_per_member: 15,
                },
                hitl: HitlConfig {
                    enabled: false,
                    checkpoints: vec![HitlCheckpoint::Never],
                },
                completion: CompletionCriteria {
                    all_tasks_done: true,
                    lead_approval_required: false,
                    all_reviews_passed: true,
                    completion_phrase: None,
                },
            },
        },
        TeamTemplateResponse {
            id: "brainstorm".to_string(),
            name: "Brainstorm & Decide".to_string(),
            description: "Consensus-based: all members propose ideas, vote, and the winner executes.".to_string(),
            template: TeamDefinition {
                id: "brainstorm".to_string(),
                name: "Brainstorm Team".to_string(),
                description: "Collaborative brainstorming with democratic decision-making.".to_string(),
                members: vec![
                    TeamMember {
                        id: "creative".to_string(),
                        role: TeamRole::Specialist { domain: "creative".to_string() },
                        agent: placeholder_agent("creative", "Creative Thinker", "You generate novel and creative solutions."),
                        responsibilities: vec!["Generate creative ideas".to_string()],
                        can_delegate_to: vec![],
                        can_veto: false,
                    },
                    TeamMember {
                        id: "pragmatist".to_string(),
                        role: TeamRole::Specialist { domain: "engineering".to_string() },
                        agent: placeholder_agent("pragmatist", "Pragmatist", "You focus on practical, implementable solutions."),
                        responsibilities: vec!["Ensure feasibility".to_string()],
                        can_delegate_to: vec![],
                        can_veto: false,
                    },
                    TeamMember {
                        id: "critic".to_string(),
                        role: TeamRole::Specialist { domain: "analysis".to_string() },
                        agent: placeholder_agent("critic", "Devil's Advocate", "You challenge assumptions and find weaknesses in proposals."),
                        responsibilities: vec!["Challenge assumptions".to_string()],
                        can_delegate_to: vec![],
                        can_veto: false,
                    },
                ],
                coordination: CoordinationProtocol::Consensus {
                    quorum: 0.67,
                    max_rounds: 3,
                },
                budget: TeamBudget {
                    max_total_cost_usd: 8.0,
                    max_cost_per_member_usd: 3.0,
                    max_total_tokens: 150_000,
                    max_duration_secs: 600,
                    max_iterations_per_member: 15,
                },
                hitl: HitlConfig {
                    enabled: true,
                    checkpoints: vec![HitlCheckpoint::OnConflict],
                },
                completion: CompletionCriteria {
                    all_tasks_done: false,
                    lead_approval_required: false,
                    all_reviews_passed: false,
                    completion_phrase: None,
                },
            },
        },
    ]
}
