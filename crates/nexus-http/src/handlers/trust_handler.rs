//! Trust & Control HTTP handlers — safety, feedback, testing, memory, plugins.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;

use crate::error::ApiError;
use crate::security::auth::{AuthContext, Scope};
use crate::security::project_access::ProjectAccess;
use crate::security::tenant::validate_project_access;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SetModeRequest {
    pub mode: crate::control_plane::NexusMode,
    pub limits: Option<crate::control_plane::ExecutionLimits>,
}

#[derive(Deserialize)]
pub struct FeedbackRequest {
    pub feedback_type: crate::feedback_engine::FeedbackType,
    pub context: crate::feedback_engine::FeedbackContext,
    pub message: String,
    pub target: Option<String>,
    pub project_id: Option<String>,
}

#[derive(Deserialize)]
pub struct InstallPluginRequest {
    pub manifest: crate::plugin_installer::PluginManifest,
}

// ---------------------------------------------------------------------------
// Control Plane handlers
// ---------------------------------------------------------------------------

/// POST /control/mode — Set Nexus operating mode.
pub async fn set_mode(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<SetModeRequest>,
) -> Json<serde_json::Value> {
    // In a full implementation, this would be stored on AppState.
    // For now, return the configuration that would be applied.
    let limits = req.limits.unwrap_or_default();
    Json(serde_json::json!({
        "mode": req.mode,
        "limits": limits,
        "applied": true,
    }))
}

/// GET /control/status — Get current control plane status.
pub async fn control_status(
    State(_state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let default_limits = crate::control_plane::ExecutionLimits::default();
    Json(serde_json::json!({
        "mode": "assisted",
        "limits": default_limits,
        "capabilities": {
            "safe_mode": "Read-only analysis and suggestions",
            "assisted_mode": "Auto-execute low risk; approve medium/high",
            "autonomous_mode": "Full execution with limits enforced",
        }
    }))
}

// ---------------------------------------------------------------------------
// Feedback handlers
// ---------------------------------------------------------------------------

/// POST /feedback — Submit user feedback.
pub async fn submit_feedback(
    State(state): State<Arc<AppState>>,
    Json(req): Json<FeedbackRequest>,
) -> Json<serde_json::Value> {
    let feedback = crate::feedback_engine::Feedback {
        feedback_type: req.feedback_type,
        context: req.context,
        message: req.message,
        target: req.target,
    };

    let result = crate::feedback_engine::process(
        &state,
        req.project_id.as_deref(),
        &feedback,
    )
    .await;

    Json(serde_json::json!(result))
}

// ---------------------------------------------------------------------------
// Runtime testing handlers
// ---------------------------------------------------------------------------

/// POST /projects/:id/test — Run real HTTP tests on a running app.
pub async fn run_runtime_tests(
    State(state): State<Arc<AppState>>,
    access: ProjectAccess,
) -> Json<serde_json::Value> {
    let project_id = access.project_id.clone();
    // Find the running app instance port
    let db = state.db.lock().await;
    let port: Option<i64> = db
        .query_row(
            "SELECT port FROM app_instances WHERE project_id = ?1 AND status = 'running' ORDER BY started_at DESC LIMIT 1",
            rusqlite::params![project_id],
            |r| r.get(0),
        )
        .ok();
    drop(db);

    let Some(port) = port else {
        return Json(serde_json::json!({
            "error": "No running app instance found. Start the app first.",
        }));
    };

    let base_url = format!("http://localhost:{}", port);

    // Generate and run tests
    let suite = crate::runtime_tester::generate_test_suite(
        &base_url,
        true,  // assume auth for safety
        true,  // assume database
        &["Dashboard".into(), "Settings".into()],
    );

    let result = crate::runtime_tester::run_tests(&base_url, &suite).await;

    Json(serde_json::json!(result))
}

// ---------------------------------------------------------------------------
// Memory handlers
// ---------------------------------------------------------------------------

/// GET /memory/unified — Get unified memory across all systems.
pub async fn get_unified_memory(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let memory = crate::memory_unification::load(&state, None).await;
    let context = crate::memory_unification::to_context(&memory);

    Json(serde_json::json!({
        "memory": memory,
        "context_preview": context,
    }))
}

/// GET /projects/:id/memory — Get project-specific unified memory.
pub async fn get_project_memory(
    State(state): State<Arc<AppState>>,
    access: ProjectAccess,
) -> Json<serde_json::Value> {
    let project_id = access.project_id.clone();
    let memory = crate::memory_unification::load(&state, Some(&project_id)).await;
    let context = crate::memory_unification::to_context(&memory);

    Json(serde_json::json!({
        "memory": memory,
        "context_preview": context,
    }))
}

/// POST /memory/decay — Trigger decay of old patterns.
pub async fn decay_memory(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    crate::memory_unification::decay_old_patterns(&state).await;
    Json(serde_json::json!({
        "decayed": true,
        "message": "Old patterns decayed successfully",
    }))
}

// ---------------------------------------------------------------------------
// Plugin installation handlers
// ---------------------------------------------------------------------------

/// POST /projects/:id/plugins/install — Install a real plugin.
pub async fn install_plugin(
    State(state): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(req): Json<InstallPluginRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    access
        .require_scope(&Scope::PluginManage)
        .map_err(|_| ApiError::Forbidden("plugin:manage scope required".into()))?;
    let project_id = access.project_id.clone();
    let project_dir = state
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");

    let result = crate::plugin_installer::install(&req.manifest, &project_dir);
    Ok(Json(serde_json::json!(result)))
}

/// POST /projects/:id/plugins/uninstall/:plugin_id — Uninstall a plugin.
pub async fn uninstall_plugin(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, plugin_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    {
        let db = state.db.lock().await;
        validate_project_access(&db, &project_id, &auth.tenant_id)
            .map_err(ApiError::Forbidden)?;
    }
    auth.require_scope(&Scope::PluginManage)
        .map_err(|_| ApiError::Forbidden("plugin:manage scope required".into()))?;
    let project_dir = state
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");

    Ok(match crate::plugin_installer::uninstall(&plugin_id, &project_dir) {
        Ok(removed) => Json(serde_json::json!({
            "success": true,
            "files_removed": removed,
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e,
        })),
    })
}
