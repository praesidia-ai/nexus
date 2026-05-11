//! Credit system HTTP handlers — balance, usage history, spend, and check.

use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use tracing::info;

use nexus_store::CreditService;

use crate::{
    error::{ApiError, ApiResult},
    security::auth::AuthContext,
    state::AppState,
};

/// GET /credits — get current credit balance
pub async fn get_credits(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    let svc = CreditService::new(&db);
    let account = svc.get_or_create_account(&auth.tenant_id)?;

    Ok(Json(serde_json::json!({
        "credits_total": account.credits_total,
        "credits_used": account.credits_used,
        "credits_remaining": account.credits_total - account.credits_used,
        "plan": account.plan,
        "resets_at": account.credits_reset_at,
    })))
}

/// GET /credits/usage — get credit usage history
pub async fn get_credit_usage(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    let svc = CreditService::new(&db);
    let usage = svc.get_usage(&auth.tenant_id, 100)?;

    Ok(Json(serde_json::json!({ "usage": usage })))
}

#[derive(Debug, Deserialize)]
pub struct SpendCreditsReq {
    pub credits: i32,
    pub action_type: String,
    pub description: Option<String>,
    pub project_id: Option<String>,
}

/// POST /credits/spend — spend credits (called by LLM middleware)
pub async fn spend_credits(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<SpendCreditsReq>,
) -> ApiResult<Json<serde_json::Value>> {
    if body.credits <= 0 {
        return Err(ApiError::BadRequest("credits must be positive".into()));
    }

    let db = app.db.lock().await;

    // If the caller is billing against a specific project, make sure it
    // belongs to their tenant — otherwise a user could attribute their own
    // spend to another tenant's project (or spend their tenant's credits
    // for work done on behalf of a project they don't control).
    if let Some(pid) = body.project_id.as_deref() {
        crate::security::tenant::validate_project_access(&db, pid, &auth.tenant_id)
            .map_err(ApiError::Forbidden)?;
    }

    let svc = CreditService::new(&db);
    let result = svc.try_spend_credits(
        &auth.tenant_id,
        body.credits,
        &body.action_type,
        body.description.as_deref(),
        body.project_id.as_deref(),
    )?;

    let account = match result {
        Some(a) => a,
        None => {
            // Insufficient balance — do NOT partially update.
            let remaining = {
                let svc = CreditService::new(&db);
                let a = svc.get_or_create_account(&auth.tenant_id)?;
                a.credits_total - a.credits_used
            };
            return Err(ApiError::TooManyRequests(format!(
                "insufficient credits: need {}, have {}",
                body.credits, remaining
            )));
        }
    };

    info!(
        tenant_id = %auth.tenant_id,
        credits = body.credits,
        action = %body.action_type,
        "Credits spent"
    );

    Ok(Json(serde_json::json!({
        "credits_remaining": account.credits_total - account.credits_used,
        "success": true,
    })))
}

#[derive(Debug, Deserialize)]
pub struct CheckCreditsReq {
    pub credits_needed: i32,
}

/// POST /credits/check — check if enough credits available
pub async fn check_credits(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<CheckCreditsReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    let svc = CreditService::new(&db);
    let sufficient = svc.check_credits(&auth.tenant_id, body.credits_needed)?;
    let account = svc.get_or_create_account(&auth.tenant_id)?;

    Ok(Json(serde_json::json!({
        "sufficient": sufficient,
        "credits_remaining": account.credits_total - account.credits_used,
        "credits_needed": body.credits_needed,
    })))
}
