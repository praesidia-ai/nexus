//! Vault (secrets) handlers for storing project-scoped key/value pairs.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use nexus_store::VaultService;

use crate::{
    error::{ApiError, ApiResult},
    input_limits::{require_bounded, MAX_VAULT_KEY_BYTES, MAX_VAULT_VALUE_BYTES},
    security::auth::AuthContext,
    security::tenant::validate_project_access,
    state::AppState,
};

/// Vault keys must look like env-var identifiers. We require an uppercase
/// letter or underscore prefix and only `A-Z`, `0-9`, `_` thereafter, so
/// downstream consumers (env files, shell scripts) cannot be tricked into
/// interpreting newlines or shell metacharacters.
fn validate_vault_key(key: &str) -> Result<(), ApiError> {
    require_bounded("vault.key", key, MAX_VAULT_KEY_BYTES)?;
    let mut chars = key.chars();
    let first_ok = chars
        .next()
        .map(|c| c == '_' || c.is_ascii_uppercase())
        .unwrap_or(false);
    let rest_ok = chars.all(|c| c == '_' || c.is_ascii_digit() || c.is_ascii_uppercase());
    if !first_ok || !rest_ok {
        return Err(ApiError::BadRequest(
            "vault.key must match ^[A-Z_][A-Z0-9_]*$".into(),
        ));
    }
    Ok(())
}

fn validate_vault_value(value: &str) -> Result<(), ApiError> {
    if value.len() > MAX_VAULT_VALUE_BYTES {
        return Err(ApiError::BadRequest(format!(
            "vault.value exceeds {MAX_VAULT_VALUE_BYTES} bytes (got {})",
            value.len()
        )));
    }
    Ok(())
}

/// GET /projects/:id/vault
pub async fn list_vault(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id).map_err(ApiError::Forbidden)?;
    let vs = VaultService::new(&db);
    let entries = vs.list(&project_id)?;
    // Mask values in list response
    let masked: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| {
            serde_json::json!({
                "id": e.id,
                "project_id": e.project_id,
                "key": e.key,
                "value": "••••••••",
                "created_at": e.created_at
            })
        })
        .collect();
    Ok(Json(serde_json::Value::Array(masked)))
}

#[derive(Deserialize)]
pub struct SetVaultReq {
    pub key: String,
    pub value: String,
}

/// POST /projects/:id/vault
pub async fn set_vault(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
    Json(body): Json<SetVaultReq>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    validate_vault_key(&body.key)?;
    validate_vault_value(&body.value)?;
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id).map_err(ApiError::Forbidden)?;
    let vs = VaultService::new(&db);
    let entry = vs.set(&project_id, &body.key, &body.value)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"id": entry.id, "key": entry.key})),
    ))
}

#[derive(Deserialize)]
pub struct SetVaultValueReq {
    pub value: String,
}

/// POST /projects/:id/vault/:key — set a single entry. Used by the Vault UI.
pub async fn set_vault_by_key(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, key)): Path<(String, String)>,
    Json(body): Json<SetVaultValueReq>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    validate_vault_key(&key)?;
    validate_vault_value(&body.value)?;
    let entry = {
        let db = app.db.lock().await;
        validate_project_access(&db, &project_id, &auth.tenant_id).map_err(ApiError::Forbidden)?;
        let vs = VaultService::new(&db);
        vs.set(&project_id, &key, &body.value)?
    };
    // Forensic audit: record the secret-write into the signed Merkle chain.
    // The VALUE is intentionally NOT logged — only metadata (project, key,
    // actor) so the audit trail does not become a secret-leak surface.
    app.audit_log
        .append(
            "system",
            "vault_set",
            serde_json::json!({
                "project_id": project_id,
                "key": key,
                "actor": auth.user_id.clone(),
                "tenant": auth.tenant_id.clone(),
                "entry_id": entry.id,
            }),
            &app.audit_keypair,
        )
        .await;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"id": entry.id, "key": entry.key})),
    ))
}

/// GET /projects/:id/vault/:key
pub async fn get_vault(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, key)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id).map_err(ApiError::Forbidden)?;
    let vs = VaultService::new(&db);
    match vs.get(&project_id, &key)? {
        Some(value) => Ok(Json(serde_json::json!({
            "key": key,
            "value": value
        }))),
        None => Err(ApiError::NotFound(format!("vault key '{key}' not found"))),
    }
}

/// DELETE /projects/:id/vault/:key
pub async fn delete_vault(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, key)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    {
        let db = app.db.lock().await;
        validate_project_access(&db, &project_id, &auth.tenant_id).map_err(ApiError::Forbidden)?;
        let vs = VaultService::new(&db);
        vs.delete(&project_id, &key)?;
    }
    app.audit_log
        .append(
            "system",
            "vault_delete",
            serde_json::json!({
                "project_id": project_id,
                "key": key,
                "actor": auth.user_id.clone(),
                "tenant": auth.tenant_id.clone(),
            }),
            &app.audit_keypair,
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}
