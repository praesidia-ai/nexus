//! Plugin system HTTP handlers.

use std::sync::Arc;

use axum::{extract::{Path, State}, Json};
use serde::Deserialize;
use serde_json::json;

use crate::{error::ApiResult, state::AppState};

/// GET /plugins — list all loaded plugins
pub async fn list_plugins(
    State(app): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let registry = app.legacy_plugin_registry.read().await;
    let summary = registry.summary();
    Ok(Json(serde_json::to_value(summary).unwrap_or(json!({}))))
}

/// POST /plugins/:id/enable — enable a plugin
pub async fn enable_plugin(
    State(app): State<Arc<AppState>>,
    Path(plugin_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let mut registry = app.legacy_plugin_registry.write().await;
    let ok = registry.enable(&plugin_id);
    Ok(Json(json!({ "plugin_id": plugin_id, "enabled": ok })))
}

/// POST /plugins/:id/disable — disable a plugin
pub async fn disable_plugin(
    State(app): State<Arc<AppState>>,
    Path(plugin_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let mut registry = app.legacy_plugin_registry.write().await;
    let ok = registry.disable(&plugin_id);
    Ok(Json(json!({ "plugin_id": plugin_id, "disabled": ok })))
}

/// DELETE /plugins/:id — uninstall a plugin
pub async fn uninstall_plugin(
    State(app): State<Arc<AppState>>,
    Path(plugin_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let mut registry = app.legacy_plugin_registry.write().await;
    let ok = registry.uninstall(&plugin_id);
    Ok(Json(json!({ "plugin_id": plugin_id, "uninstalled": ok })))
}

#[derive(Deserialize)]
pub struct ImportReq {
    pub bundle: serde_json::Value,
}

/// POST /plugins/import — import a plugin from JSON bundle
pub async fn import_plugin(
    State(app): State<Arc<AppState>>,
    Json(body): Json<ImportReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let mut registry = app.legacy_plugin_registry.write().await;
    match registry.import(&body.bundle) {
        Ok(id) => Ok(Json(json!({ "plugin_id": id, "status": "imported" }))),
        Err(e) => Ok(Json(json!({ "error": e }))),
    }
}

/// GET /plugins/:id/export — export plugin as portable bundle
pub async fn export_plugin(
    State(app): State<Arc<AppState>>,
    Path(plugin_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let registry = app.legacy_plugin_registry.read().await;
    match registry.export(&plugin_id) {
        Some(bundle) => Ok(Json(json!({ "plugin_id": plugin_id, "bundle": bundle }))),
        None => Ok(Json(json!({ "error": "Plugin not found" }))),
    }
}
