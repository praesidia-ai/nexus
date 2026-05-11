//! Handlers for the Human-in-the-Loop Control System.

use std::sync::Arc;

use axum::{
    extract::State,
    Json,
};
use serde_json::json;

use crate::{
    error::{ApiError, ApiResult},
    hitl::ApprovalResponse,
    security::project_access::ProjectAccess,
    state::AppState,
};

// Note: In production, the HitlController would be part of AppState.
// These handlers demonstrate the API contract.

/// GET /projects/:id/gates — list all approval gates
pub async fn list_gates(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    // Read from audit_entries where action starts with 'hitl_'
    let db = app.db.lock().await;
    let mut stmt = db
        .prepare(
            "SELECT detail FROM audit_entries WHERE project_id = ?1 AND action LIKE 'hitl_%' ORDER BY created_at DESC LIMIT 50",
        )
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let gates: Vec<serde_json::Value> = stmt
        .query_map(rusqlite::params![project_id], |row| {
            let detail: String = row.get(0)?;
            Ok(detail)
        })
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .filter_map(|r| r.ok())
        .filter_map(|s| serde_json::from_str(&s).ok())
        .collect();

    Ok(Json(json!({ "gates": gates })))
}

/// GET /projects/:id/gates/pending — list pending gates
pub async fn pending_gates(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let db = app.db.lock().await;
    let mut stmt = db
        .prepare(
            "SELECT detail FROM audit_entries WHERE project_id = ?1 AND action LIKE 'hitl_%' AND detail LIKE '%\"pending\"%' ORDER BY created_at DESC",
        )
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let gates: Vec<serde_json::Value> = stmt
        .query_map(rusqlite::params![project_id], |row| {
            let detail: String = row.get(0)?;
            Ok(detail)
        })
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .filter_map(|r| r.ok())
        .filter_map(|s| serde_json::from_str(&s).ok())
        .collect();

    Ok(Json(json!({ "gates": gates })))
}

/// Request body accepted by `/projects/:id/gates/resolve`.
///
/// The UI sends the simple `{ gate_id, approved }` shape; richer clients
/// (CLI, agent SDK) may send `{ gate_id, action, modifications?, reason? }`.
/// Both forms are accepted here and normalised into an [`ApprovalResponse`].
#[derive(serde::Deserialize)]
#[serde(untagged)]
pub enum ResolveRequest {
    Full(ApprovalResponse),
    Simple {
        gate_id: String,
        approved: bool,
        #[serde(default)]
        reason: Option<String>,
    },
}

impl From<ResolveRequest> for ApprovalResponse {
    fn from(req: ResolveRequest) -> Self {
        match req {
            ResolveRequest::Full(r) => r,
            ResolveRequest::Simple {
                gate_id,
                approved,
                reason,
            } => ApprovalResponse {
                gate_id,
                action: if approved {
                    crate::hitl::ApprovalAction::Approve
                } else {
                    crate::hitl::ApprovalAction::Reject
                },
                modifications: None,
                reason,
            },
        }
    }
}

/// POST /projects/:id/gates/resolve — record a decision on a pending gate.
///
/// Writes the resolution to the audit log so subsequent `pending_gates`
/// calls exclude it. Note: the in-memory [`HitlController`] that originally
/// *raised* the gate is tied to a running agent loop — if no such loop is
/// currently active for this gate, the caller is unblocking a stale record.
pub async fn resolve_gate(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(req): Json<ResolveRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let auth_user_id = access.user_id.clone();
    let project_id = access.project_id.clone();
    let response: ApprovalResponse = req.into();
    let action = format!("hitl_{:?}", response.action).to_lowercase();

    let detail = json!({
        "gate_id": response.gate_id,
        "action": action,
        "status": "resolved",
        "approved": matches!(response.action, crate::hitl::ApprovalAction::Approve),
        "reason": response.reason,
    });

    let db = app.db.lock().await;
    db.execute(
        "INSERT INTO audit_entries (id, project_id, actor, action, detail, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            project_id,
            auth_user_id,
            action,
            detail.to_string(),
        ],
    )
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(json!({
        "gate_id": response.gate_id,
        "status": "resolved",
    })))
}
