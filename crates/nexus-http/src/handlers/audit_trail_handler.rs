//! HTTP handlers for the cryptographic audit trail.
//!
//! Routes (mounted from `server.rs`):
//!   GET  /audit/chain          — recent audit entries (authenticated)
//!   GET  /audit/chain/verify   — verify chain integrity (authenticated)
//!   GET  /audit/pubkey         — get the node's public signing key (authenticated)
//!   POST /audit/chain          — manual append; mounted under `admin_routes()`
//!                                so it requires the SystemAdmin scope.
//!                                Without that gate an authenticated tenant
//!                                could forge "system"-attributed entries
//!                                (e.g. a fake `kill_switch_activated`).

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;

use crate::state::AppState;

/// `GET /audit/chain` — list recent audit entries.
pub async fn list_audit_entries(State(app): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let entries = app.audit_log.recent(100).await;
    let count = app.audit_log.len().await;
    Json(serde_json::json!({
        "total_entries": count,
        "entries": entries,
    }))
}

/// `GET /audit/chain/verify` — verify the audit chain integrity.
pub async fn verify_audit_chain(State(app): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let valid = app.audit_log.verify_chain().await;
    let count = app.audit_log.len().await;
    Json(serde_json::json!({
        "chain_valid": valid,
        "entry_count": count,
    }))
}

/// `GET /audit/pubkey` — get the node's public signing key.
pub async fn get_pubkey(State(app): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "public_key": app.audit_keypair.public_key_hex(),
        "algorithm": "Ed25519",
    }))
}

/// Request body for manual audit entry.
#[derive(Deserialize)]
pub struct AppendAuditReq {
    pub agent_id: String,
    pub action: String,
    pub payload: Option<serde_json::Value>,
}

/// `POST /audit/chain` — manually append an audit entry (admin only).
pub async fn append_audit_entry(
    State(app): State<Arc<AppState>>,
    Json(req): Json<AppendAuditReq>,
) -> (StatusCode, Json<serde_json::Value>) {
    let payload = req.payload.unwrap_or(serde_json::Value::Null);
    let hash = app
        .audit_log
        .append(&req.agent_id, &req.action, payload, &app.audit_keypair)
        .await;

    (
        StatusCode::CREATED,
        Json(serde_json::json!({ "hash": hash, "status": "appended" })),
    )
}
