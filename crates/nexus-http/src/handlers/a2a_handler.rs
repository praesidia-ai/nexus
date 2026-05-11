//! A2A protocol HTTP handlers.
//!
//! Exposes two endpoints:
//!   GET  /.well-known/agent.json  — Agent Card discovery
//!   POST /a2a                     — JSON-RPC 2.0 task dispatch

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use tracing::info;

use nexus_a2a::{AgentCapabilities, AgentCard, AgentProvider, AgentSkill, AuthScheme};

use crate::security::auth::AuthContext;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// GET /.well-known/agent.json
// ---------------------------------------------------------------------------

pub async fn agent_card(State(app): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let card = app.a2a_registry.local_card().await.unwrap_or_else(|| {
        build_default_card(
            &std::env::var("NEXUS_PUBLIC_URL")
                .unwrap_or_else(|_| "http://localhost:8020".to_string()),
        )
    });
    Json(serde_json::to_value(card).unwrap_or_default())
}

// ---------------------------------------------------------------------------
// POST /a2a — JSON-RPC dispatch
// ---------------------------------------------------------------------------

/// JSON-RPC dispatch entrypoint for the A2A protocol.
///
/// SECURITY: this surface invokes `app.a2a_server.dispatch`, which can run
/// `tasks/send` against the platform-wide LLM key. Without authentication +
/// rate limiting any caller could drain the platform budget and execute
/// agent tasks anonymously. We therefore:
///   1. Require an `AuthContext` so the call has an attributable tenant.
///   2. Acquire an LLM concurrency slot for any RPC method that may dispatch
///      a `tasks/send` — the slot is held for the full request, which is the
///      conservative choice (the dispatcher does not currently surface
///      whether a given call will hit an LLM).
///   3. Log every call with tenant + RPC method for forensic review.
pub async fn dispatch(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let method = body
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("<unknown>")
        .to_string();
    info!(
        tenant_id = %auth.tenant_id,
        user_id = %auth.user_id,
        rpc_method = %method,
        "a2a dispatch",
    );

    if let Err(reason) = app.tenant_rate_limiter.check(&auth.tenant_id, "agent") {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": body.get("id"),
                "error": { "code": -32000, "message": reason },
            })),
        );
    }

    let _slot = match app.rate_limiter.acquire_llm_slot().await {
        Ok(g) => g,
        Err(e) => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": body.get("id"),
                    "error": { "code": -32000, "message": e },
                })),
            );
        }
    };

    let response = app.a2a_server.dispatch(body).await;
    (StatusCode::OK, Json(response))
}

// ---------------------------------------------------------------------------
// GET /a2a/peers — list discovered remote agents
// ---------------------------------------------------------------------------

pub async fn list_peers(State(app): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let peers = app.a2a_registry.list_remote().await;
    Json(serde_json::json!({ "peers": peers }))
}

// ---------------------------------------------------------------------------
// POST /a2a/peers/discover — fetch a remote Agent Card by base URL
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
pub struct DiscoverReq {
    pub base_url: String,
}

pub async fn discover_peer(
    State(app): State<Arc<AppState>>,
    Json(body): Json<DiscoverReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // SSRF guard: refuse loopback / RFC1918 / link-local / non-http(s) URLs.
    // Without this, an authenticated tenant can pivot the server into
    // http://169.254.169.254 (AWS IMDS), http://localhost:..., or any
    // internal service and read its response back as the "agent card"
    // error body. The federation peer registration handler already does
    // this; the bare /a2a/peers discover used to skip it.
    if let Err(e) = crate::security::url_guard::is_public_url(&body.base_url) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Peer URL rejected by SSRF guard: {e}")
            })),
        ));
    }
    match app.a2a_registry.discover(&body.base_url).await {
        Ok(card) => Ok(Json(serde_json::to_value(card).unwrap_or_default())),
        Err(e) => Err((
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": e.to_string() })),
        )),
    }
}

// ---------------------------------------------------------------------------
// DELETE /a2a/peers/:url_encoded — remove a remote peer
// ---------------------------------------------------------------------------

pub async fn remove_peer(
    State(app): State<Arc<AppState>>,
    axum::extract::Path(url_encoded): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let url = urlencoding_decode(&url_encoded);
    match app.a2a_registry.remove_remote(&url).await {
        Ok(_) => Ok(Json(serde_json::json!({ "status": "removed", "url": url }))),
        Err(e) => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e.to_string() })),
        )),
    }
}

// ---------------------------------------------------------------------------
// Federation endpoints
// ---------------------------------------------------------------------------

/// `GET /a2a/federation/peers` — list federated Nexus peer instances.
pub async fn federation_list_peers(State(app): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let peers = app.federation.list_peers().await;
    Json(serde_json::json!({ "peers": peers, "count": peers.len() }))
}

#[derive(serde::Deserialize)]
pub struct FederateReq {
    pub url: String,
}

/// `POST /a2a/federation/peers` — register a remote Nexus instance for federation.
pub async fn federation_register_peer(
    State(app): State<Arc<AppState>>,
    Json(body): Json<FederateReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // SSRF guard: reject loopback, RFC1918, link-local, and non-http(s) schemes.
    if let Err(e) = crate::security::url_guard::is_public_url(&body.url) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Peer URL rejected by SSRF guard: {e}")
            })),
        ));
    }
    match app.federation.register_peer(&body.url).await {
        Ok(peer) => Ok(Json(serde_json::to_value(peer).unwrap_or_default())),
        Err(e) => Err((
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": e.to_string() })),
        )),
    }
}

/// `DELETE /a2a/federation/peers/:id` — remove a federated peer.
pub async fn federation_remove_peer(
    State(app): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Json<serde_json::Value> {
    let removed = app.federation.remove_peer(&id).await;
    Json(serde_json::json!({ "removed": removed, "id": id }))
}

#[derive(serde::Deserialize)]
pub struct DelegateReq {
    pub message: String,
    pub required_skill: Option<String>,
}

/// `POST /a2a/federation/delegate` — delegate a task to a remote Nexus peer.
///
/// Requires `agent:execute` (delegating to a remote peer is, semantically,
/// running an agent). The originating tenant_id is attached to the message
/// metadata so federated peers can attribute work back to its source — this
/// also feeds the audit log on the receiving end.
pub async fn federation_delegate(
    State(app): State<Arc<AppState>>,
    auth: crate::security::auth::AuthContext,
    Json(body): Json<DelegateReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if let Err(resp) = auth.require_scope(&crate::security::auth::Scope::AgentExecute) {
        // Bubble up the response built by `require_scope` (proper 403 + envelope).
        let _ = resp;
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                serde_json::json!({"error": "agent:execute scope required for federation delegate"}),
            ),
        ));
    }

    let mut metadata: std::collections::HashMap<String, serde_json::Value> =
        std::collections::HashMap::new();
    metadata.insert(
        "originating_tenant_id".into(),
        serde_json::Value::String(auth.tenant_id.clone()),
    );
    metadata.insert(
        "originating_user_id".into(),
        serde_json::Value::String(auth.user_id.clone()),
    );

    let msg = nexus_a2a::Message {
        role: nexus_a2a::MessageRole::User,
        parts: vec![nexus_a2a::Part::Text { text: body.message }],
        metadata,
    };
    match app
        .federation
        .delegate(&msg, body.required_skill.as_deref())
        .await
    {
        Ok(task) => {
            // Audit so we have a tamper-evident record of every cross-instance
            // delegation initiated from this server.
            app.audit_log
                .append(
                    &auth.tenant_id,
                    "federation.delegate",
                    serde_json::json!({
                        "user_id": auth.user_id,
                        "required_skill": body.required_skill,
                    }),
                    &app.audit_keypair,
                )
                .await;
            Ok(Json(serde_json::to_value(task).unwrap_or_default()))
        }
        Err(e) => Err((
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": e.to_string() })),
        )),
    }
}

/// `POST /a2a/federation/health-check` — ping all federated peers.
pub async fn federation_health_check(State(app): State<Arc<AppState>>) -> Json<serde_json::Value> {
    app.federation.health_check_all().await;
    let peers = app.federation.list_peers().await;
    let healthy = peers.iter().filter(|p| p.healthy).count();
    Json(serde_json::json!({
        "total": peers.len(),
        "healthy": healthy,
        "unhealthy": peers.len() - healthy,
    }))
}

fn urlencoding_decode(s: &str) -> String {
    // Use a real percent-decoder so any escape (`%23`, `%3D`, etc.) round-trips
    // correctly. The previous hand-rolled four-escape replacement caused
    // `remove_peer` to silently fail to match URLs containing other escapes.
    percent_encoding::percent_decode_str(s)
        .decode_utf8_lossy()
        .into_owned()
}

// ---------------------------------------------------------------------------
// Helper: build a default Agent Card from env / config
// ---------------------------------------------------------------------------

pub fn build_default_card(base_url: &str) -> AgentCard {
    AgentCard {
        name: "Nexus Agent OS".into(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Open-source Agent Operating System — generate apps, run agent teams, manage memory and workflows.".into(),
        url: format!("{}/a2a", base_url.trim_end_matches('/')),
        provider: Some(AgentProvider {
            organization: "Nexus Contributors".into(),
            url: Some("https://github.com/YOUR_ORG/nexus".into()),
        }),
        icon_url: None,
        protocol_version: "0.2.1".into(),
        capabilities: AgentCapabilities {
            streaming: true,
            push_notifications: false,
            state_transition_history: true,
        },
        authentication: vec![AuthScheme { scheme: "bearer".into() }],
        default_input_modes: vec!["text/plain".into(), "application/json".into()],
        default_output_modes: vec!["text/plain".into(), "application/json".into()],
        skills: vec![
            AgentSkill {
                id: "generate_app".into(),
                name: "Generate Application".into(),
                description: "Generate a full-stack application from a description.".into(),
                input_modes: vec!["text/plain".into()],
                output_modes: vec!["text/plain".into(), "application/json".into()],
                tags: vec!["codegen".into(), "app".into()],
                examples: Some(vec!["Build a SaaS dashboard for a fitness app".into()]),
            },
            AgentSkill {
                id: "run_agent".into(),
                name: "Run Agent".into(),
                description: "Execute a named Nexus agent on a task.".into(),
                input_modes: vec!["text/plain".into()],
                output_modes: vec!["text/plain".into()],
                tags: vec!["agent".into(), "task".into()],
                examples: None,
            },
            AgentSkill {
                id: "run_team".into(),
                name: "Run Agent Team".into(),
                description: "Orchestrate a multi-agent team on a complex task.".into(),
                input_modes: vec!["text/plain".into(), "application/json".into()],
                output_modes: vec!["text/plain".into(), "application/json".into()],
                tags: vec!["team".into(), "orchestration".into()],
                examples: None,
            },
            AgentSkill {
                id: "memory_recall".into(),
                name: "Memory Recall".into(),
                description: "Recall episodic and semantic memories.".into(),
                input_modes: vec!["text/plain".into()],
                output_modes: vec!["application/json".into()],
                tags: vec!["memory".into()],
                examples: None,
            },
        ],
    }
}
