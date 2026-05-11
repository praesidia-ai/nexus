//! HTTP handlers for sandboxed code execution.
//!
//! Provides endpoints to create, execute in, inspect, and destroy sandbox instances.
//! Automatically selects the Docker backend when available, falling back to process-based
//! sandboxing otherwise.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use nexus_sandbox::{
    docker::DockerSandbox,
    process::ProcessSandbox,
    sandbox::{ExecResult, Sandbox, SandboxConfig, SandboxInstance},
    wasm::{WasmCapabilities, WasmSandbox, WasmSandboxConfig},
};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::security::auth::AuthContext;
use crate::state::AppState;

/// Reject when `sandbox_id` was created by a different tenant. SystemAdmin
/// implies cross-tenant access in this codebase, but we still attribute every
/// exec/destroy/logs call to a tenant — so the same admin acting under tenant
/// A cannot replay a sandbox ID created under tenant B without explicit
/// scope. The check is "best effort"; sandboxes that pre-date this server
/// process are not in the map and are rejected with 404 (the user must
/// recreate them).
async fn require_sandbox_owner(
    state: &Arc<AppState>,
    auth: &AuthContext,
    sandbox_id: &str,
) -> Result<(), (StatusCode, String)> {
    let owners = state.sandbox_owners.read().await;
    match owners.get(sandbox_id) {
        Some(t) if t == &auth.tenant_id => Ok(()),
        Some(_) => Err((
            StatusCode::NOT_FOUND,
            format!("sandbox {sandbox_id} not found in tenant"),
        )),
        None => Err((
            StatusCode::NOT_FOUND,
            format!("sandbox {sandbox_id} unknown to this server"),
        )),
    }
}

/// Request body for `POST /sandbox/create`.
#[derive(Debug, Deserialize)]
pub struct CreateSandboxRequest {
    /// Optional sandbox configuration. Uses defaults if omitted.
    #[serde(default)]
    pub config: Option<SandboxConfig>,
}

/// Request body for `POST /sandbox/:id/exec`.
#[derive(Debug, Deserialize)]
pub struct ExecRequest {
    /// Shell command to execute inside the sandbox.
    pub command: String,
    /// Hard timeout in seconds. Defaults to 30, capped at 300.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// Envelope for sandbox API responses.
#[derive(Debug, Serialize)]
pub struct SandboxResponse<T: Serialize> {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: Serialize> SandboxResponse<T> {
    fn success(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }
}

fn error_response<T: Serialize>(msg: String) -> (StatusCode, Json<SandboxResponse<T>>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(SandboxResponse {
            ok: false,
            data: None,
            error: Some(msg),
        }),
    )
}

/// Get the best available sandbox backend.
fn get_sandbox() -> Box<dyn Sandbox> {
    let docker = DockerSandbox::new();
    if docker.is_available() {
        info!("using Docker sandbox backend");
        Box::new(docker)
    } else {
        info!("Docker not available, falling back to process sandbox");
        Box::new(ProcessSandbox::new())
    }
}

/// `POST /sandbox/create`
///
/// Create a new sandbox instance. Returns the instance metadata including its ID.
pub async fn create_sandbox(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<CreateSandboxRequest>,
) -> Result<Json<SandboxResponse<SandboxInstance>>, (StatusCode, Json<SandboxResponse<SandboxInstance>>)>
{
    let sandbox = get_sandbox();
    let config = body.config.unwrap_or_default();

    match sandbox.create(config).await {
        Ok(instance) => {
            // Track the creating tenant so subsequent exec/destroy/logs can
            // verify the caller is allowed to touch this sandbox.
            state
                .sandbox_owners
                .write()
                .await
                .insert(instance.id.clone(), auth.tenant_id.clone());
            Ok(Json(SandboxResponse::success(instance)))
        }
        Err(e) => Err(error_response(e.to_string())),
    }
}

/// `POST /sandbox/:id/exec`
///
/// Execute a command inside an existing sandbox instance.
pub async fn exec_in_sandbox(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(sandbox_id): Path<String>,
    Json(body): Json<ExecRequest>,
) -> Result<Json<SandboxResponse<ExecResult>>, (StatusCode, Json<SandboxResponse<ExecResult>>)> {
    if let Err((code, msg)) = require_sandbox_owner(&state, &auth, &sandbox_id).await {
        return Err((code, Json(SandboxResponse { ok: false, data: None, error: Some(msg) })));
    }
    let sandbox = get_sandbox();

    // Reconstruct a minimal instance handle from the ID.
    // In a production system, we would store instance metadata in the database.
    let instance = SandboxInstance {
        id: sandbox_id,
        status: nexus_sandbox::SandboxStatus::Running,
        config: SandboxConfig::default(),
        created_at: String::new(),
    };

    // Hard wall-clock timeout so a runaway `while true; do :; done` cannot
    // pin a Tokio worker indefinitely. The inner sandbox may have its own
    // timeout, but this one is always-on regardless of backend.
    let timeout = std::time::Duration::from_secs(body.timeout_secs.unwrap_or(30).min(300));
    match tokio::time::timeout(timeout, sandbox.exec(&instance, &body.command)).await {
        Ok(Ok(result)) => Ok(Json(SandboxResponse::success(result))),
        Ok(Err(e)) => Err(error_response(e.to_string())),
        Err(_) => Err((
            StatusCode::GATEWAY_TIMEOUT,
            Json(SandboxResponse {
                ok: false,
                data: None,
                error: Some(format!(
                    "sandbox exec exceeded {}s timeout",
                    timeout.as_secs()
                )),
            }),
        )),
    }
}

/// `POST /sandbox/:id/destroy`
///
/// Destroy a sandbox instance, releasing all associated resources.
pub async fn destroy_sandbox(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(sandbox_id): Path<String>,
) -> Result<Json<SandboxResponse<String>>, (StatusCode, Json<SandboxResponse<String>>)> {
    if let Err((code, msg)) = require_sandbox_owner(&state, &auth, &sandbox_id).await {
        return Err((code, Json(SandboxResponse { ok: false, data: None, error: Some(msg) })));
    }
    let sandbox = get_sandbox();

    let instance = SandboxInstance {
        id: sandbox_id.clone(),
        status: nexus_sandbox::SandboxStatus::Running,
        config: SandboxConfig::default(),
        created_at: String::new(),
    };

    let result = sandbox.destroy(&instance).await;
    // Drop the ownership entry regardless of destroy outcome so a flaky
    // backend cannot pin the registry forever.
    state.sandbox_owners.write().await.remove(&sandbox_id);
    match result {
        Ok(()) => Ok(Json(SandboxResponse::success(format!(
            "sandbox {} destroyed",
            sandbox_id
        )))),
        Err(e) => Err(error_response(e.to_string())),
    }
}

/// `GET /sandbox/:id/logs`
///
/// Retrieve all logs from a sandbox instance.
pub async fn get_sandbox_logs(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(sandbox_id): Path<String>,
) -> Result<Json<SandboxResponse<String>>, (StatusCode, Json<SandboxResponse<String>>)> {
    if let Err((code, msg)) = require_sandbox_owner(&state, &auth, &sandbox_id).await {
        return Err((code, Json(SandboxResponse { ok: false, data: None, error: Some(msg) })));
    }
    let sandbox = get_sandbox();

    let instance = SandboxInstance {
        id: sandbox_id,
        status: nexus_sandbox::SandboxStatus::Running,
        config: SandboxConfig::default(),
        created_at: String::new(),
    };

    match sandbox.logs(&instance).await {
        Ok(logs) => Ok(Json(SandboxResponse::success(logs))),
        Err(e) => Err(error_response(e.to_string())),
    }
}

// ---------------------------------------------------------------------------
// WASM sandbox endpoints
// ---------------------------------------------------------------------------

/// Request body for `POST /sandbox/wasm/exec`.
#[derive(Debug, Deserialize)]
pub struct WasmExecRequest {
    /// Base64-encoded WASM binary.
    pub wasm_base64: String,
    /// Command-line arguments passed to the WASM module.
    #[serde(default)]
    pub args: Vec<String>,
    /// Maximum fuel (instruction budget). Default: 10,000,000.
    pub fuel_limit: Option<u64>,
    /// Timeout in seconds. Default: 30.
    pub timeout_secs: Option<u64>,
}

/// `POST /sandbox/wasm/exec` — Execute a WASM binary in the WASM sandbox.
pub async fn exec_wasm(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<WasmExecRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    use base64::Engine as _;

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&req.wasm_base64)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("Invalid base64: {e}") })),
            )
        })?;

    let config = WasmSandboxConfig {
        default_fuel: req.fuel_limit.unwrap_or(10_000_000),
        default_timeout: std::time::Duration::from_secs(req.timeout_secs.unwrap_or(30)),
        ..WasmSandboxConfig::default()
    };

    let sandbox = WasmSandbox::new(config).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;

    let arg_refs: Vec<&str> = req.args.iter().map(|s| s.as_str()).collect();
    let caps = WasmCapabilities {
        fuel_limit: req.fuel_limit,
        timeout: req.timeout_secs.map(std::time::Duration::from_secs),
        ..WasmCapabilities::default()
    };

    sandbox
        .exec_bytes(&bytes, &arg_refs, caps)
        .await
        .map(|r| {
            Json(serde_json::json!({
                "stdout": r.stdout,
                "stderr": r.stderr,
                "exit_code": r.exit_code,
                "fuel_consumed": r.fuel_consumed,
                "duration_ms": r.duration_ms,
            }))
        })
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })
}
