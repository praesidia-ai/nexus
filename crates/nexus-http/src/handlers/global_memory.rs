//! Global memory endpoints — cross-project learning and user preferences.
//!
//! Reads are available to any authenticated caller; writes require
//! `system:admin` scope because the store is shared across tenants.

use std::sync::Arc;
use axum::{extract::{Path, State}, Json};
use serde::Deserialize;
use serde_json::json;
use nexus_store::GlobalMemoryService;
use crate::{
    error::{ApiError, ApiResult},
    security::auth::{AuthContext, Scope},
    state::AppState,
};

#[derive(Deserialize)]
pub struct RememberReq { pub category: String, pub key: String, pub value: String, pub source: Option<String> }

pub async fn list_memories(
    State(app): State<Arc<AppState>>,
    _auth: AuthContext,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    let svc = GlobalMemoryService::new(&db);
    Ok(Json(json!(svc.list_all()?)))
}

pub async fn remember(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<RememberReq>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::SystemAdmin)
        .map_err(|_| ApiError::Forbidden("system:admin scope required".into()))?;
    let db = app.db.lock().await;
    let svc = GlobalMemoryService::new(&db);
    let entry = svc.remember(&body.category, &body.key, &body.value, body.source.as_deref())?;
    Ok(Json(json!(entry)))
}

pub async fn forget(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(key): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::SystemAdmin)
        .map_err(|_| ApiError::Forbidden("system:admin scope required".into()))?;
    let db = app.db.lock().await;
    let svc = GlobalMemoryService::new(&db);
    svc.forget(&key)?;
    Ok(Json(json!({"deleted": true})))
}
