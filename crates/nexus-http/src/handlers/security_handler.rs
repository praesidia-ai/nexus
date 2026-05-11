//! Security management endpoints — API key CRUD and audit log queries.
//!
//! These endpoints require authentication and appropriate scopes:
//! - API key management: `SettingsManage` or `SystemAdmin`
//! - Audit log: `SystemAdmin`

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    error::{ApiError, ApiResult},
    security::{
        api_keys::{self, ApiKeyInfo},
        audit::{self, AuditEntry},
        auth::{AuthContext, Scope},
    },
    state::AppState,
};

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

/// Request body for creating a new API key.
#[derive(Debug, Deserialize)]
pub struct CreateApiKeyReq {
    /// Human-readable name for the key.
    pub name: String,
    /// Permission scopes to grant. Defaults to `["project:read"]` if empty.
    pub scopes: Option<Vec<String>>,
}

/// Response returned when an API key is created.
/// The `raw_key` field is only populated once — on creation.
#[derive(Debug, Serialize)]
pub struct CreateApiKeyResponse {
    /// The raw API key — shown exactly once. Store it securely.
    pub raw_key: String,
    /// Key metadata.
    pub key: ApiKeyInfo,
}

/// Query parameters for the audit log endpoint.
#[derive(Debug, Deserialize)]
pub struct AuditLogQuery {
    /// Maximum number of entries to return (default: 50, max: 500).
    pub limit: Option<usize>,
    /// Offset for pagination (default: 0).
    pub offset: Option<usize>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `POST /auth/keys` — Create a new API key for the authenticated tenant.
///
/// Requires `SettingsManage` scope.
pub async fn create_key(
    State(app): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthContext>,
    Json(body): Json<CreateApiKeyReq>,
) -> ApiResult<(StatusCode, Json<CreateApiKeyResponse>)> {
    auth.require_scope(&Scope::SettingsManage)
        .map_err(|_| ApiError::Forbidden("Missing scope: settings:manage".into()))?;

    if body.name.is_empty() || body.name.len() > 255 {
        return Err(ApiError::BadRequest(
            "Key name must be 1-255 characters".into(),
        ));
    }

    let scopes: Vec<Scope> = match &body.scopes {
        Some(s) if !s.is_empty() => s
            .iter()
            .map(|s| {
                Scope::parse(s).ok_or_else(|| ApiError::BadRequest(format!("Unknown scope: {s}")))
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => vec![Scope::ProjectRead],
    };

    // Privilege escalation guard: a caller may only mint keys with scopes they
    // already hold. Without this check, anyone with `settings:manage` could
    // grant themselves `system:admin` (which short-circuits every other scope
    // check via `has_scope`).
    let caller_is_admin = auth.scopes.contains(&Scope::SystemAdmin);
    for s in &scopes {
        if matches!(s, Scope::SystemAdmin) && !caller_is_admin {
            return Err(ApiError::Forbidden(
                "Only system:admin keys may grant system:admin".into(),
            ));
        }
        // For non-admin callers, require they explicitly hold every requested
        // scope. Admins are allowed to provision lesser-scoped keys for
        // delegation.
        if !caller_is_admin && !auth.scopes.contains(s) {
            return Err(ApiError::Forbidden(format!(
                "Cannot grant scope {s} — not held by caller"
            )));
        }
    }

    let (raw_key, key_info) = {
        let db = app.db.lock().await;
        let (raw_key, key_info) =
            api_keys::create_api_key(&db, &auth.tenant_id, &body.name, &scopes)
                .map_err(ApiError::Internal)?;

        // Record audit entry in the per-tenant SQLite log (fast query path).
        let audit_entry = AuditEntry::new(
            &auth.tenant_id,
            &auth.user_id,
            "api_key.create",
            "api_key",
            &key_info.id,
        )
        .with_details(serde_json::json!({ "name": body.name }));
        let _ = audit::record_audit(&db, &audit_entry);
        (raw_key, key_info)
    };

    // Also record in the signed Merkle chain (tamper-evident path). Granted
    // scopes are recorded so a forensic replay can answer
    // "what privileges were created and when?"
    let scope_strs: Vec<String> = scopes.iter().map(|s| s.to_string()).collect();
    app.audit_log
        .append(
            "system",
            "api_key.create",
            serde_json::json!({
                "tenant_id": auth.tenant_id,
                "actor": auth.user_id,
                "key_id": key_info.id,
                "key_name": body.name,
                "scopes": scope_strs,
            }),
            &app.audit_keypair,
        )
        .await;

    info!(
        tenant_id = %auth.tenant_id,
        key_id = %key_info.id,
        key_name = %body.name,
        "API key created"
    );

    Ok((
        StatusCode::CREATED,
        Json(CreateApiKeyResponse {
            raw_key,
            key: key_info,
        }),
    ))
}

/// `GET /auth/keys` — List all API keys for the authenticated tenant.
///
/// Keys are masked — only prefix and metadata are returned.
/// Requires `SettingsManage` scope.
pub async fn list_keys(
    State(app): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthContext>,
) -> ApiResult<Json<Vec<ApiKeyInfo>>> {
    auth.require_scope(&Scope::SettingsManage)
        .map_err(|_| ApiError::Forbidden("Missing scope: settings:manage".into()))?;

    let db = app.db.lock().await;
    let keys = api_keys::list_api_keys(&db, &auth.tenant_id);
    Ok(Json(keys))
}

/// `DELETE /auth/keys/:id` — Revoke an API key.
///
/// Requires `SettingsManage` scope. The key must belong to the authenticated tenant.
pub async fn revoke_key(
    State(app): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthContext>,
    Path(key_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::SettingsManage)
        .map_err(|_| ApiError::Forbidden("Missing scope: settings:manage".into()))?;

    {
        let db = app.db.lock().await;
        api_keys::revoke_api_key(&db, &key_id, &auth.tenant_id).map_err(ApiError::NotFound)?;

        // Record in the per-tenant SQLite log (fast query path).
        let audit_entry = AuditEntry::new(
            &auth.tenant_id,
            &auth.user_id,
            "api_key.revoke",
            "api_key",
            &key_id,
        );
        let _ = audit::record_audit(&db, &audit_entry);
    }

    // Also record in the signed Merkle chain (tamper-evident path).
    app.audit_log
        .append(
            "system",
            "api_key.revoke",
            serde_json::json!({
                "tenant_id": auth.tenant_id,
                "actor": auth.user_id,
                "key_id": key_id,
            }),
            &app.audit_keypair,
        )
        .await;

    info!(
        tenant_id = %auth.tenant_id,
        key_id = %key_id,
        "API key revoked"
    );

    Ok(Json(
        serde_json::json!({ "status": "revoked", "id": key_id }),
    ))
}

/// `GET /audit/log` — Query the audit log for the authenticated tenant.
///
/// Requires `SystemAdmin` scope. Supports `limit` and `offset` query parameters.
pub async fn get_audit_log(
    State(app): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthContext>,
    Query(params): Query<AuditLogQuery>,
) -> ApiResult<Json<Vec<AuditEntry>>> {
    auth.require_scope(&Scope::SystemAdmin)
        .map_err(|_| ApiError::Forbidden("Missing scope: system:admin".into()))?;

    let limit = params.limit.unwrap_or(50).min(500);
    let offset = params.offset.unwrap_or(0);

    let db = app.db.lock().await;
    let entries = audit::list_audit(&db, &auth.tenant_id, limit, offset);
    Ok(Json(entries))
}
