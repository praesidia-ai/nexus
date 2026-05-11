//! User preferences HTTP handlers — display mode, onboarding, theme, quality.

use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use tracing::info;

use nexus_store::UserPrefsService;

use crate::{
    error::ApiResult,
    security::auth::AuthContext,
    state::AppState,
};

/// GET /preferences — get user preferences
pub async fn get_preferences(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    let svc = UserPrefsService::new(&db);
    let prefs = svc.get_or_create(&auth.tenant_id)?;

    Ok(Json(serde_json::json!({
        "display_mode": prefs.display_mode,
        "onboarding_completed": prefs.onboarding_completed,
        "onboarding_step": prefs.onboarding_step,
        "preferred_quality": prefs.preferred_quality,
        "theme": prefs.theme,
    })))
}

#[derive(Debug, Deserialize)]
pub struct UpdatePrefsReq {
    pub display_mode: Option<String>,
    pub preferred_quality: Option<String>,
    pub theme: Option<String>,
}

/// PUT /preferences — update preferences
///
/// All field updates apply to the same `user_profile_prefs` row and are
/// committed atomically so a partial failure cannot leave inconsistent state.
pub async fn update_preferences(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<UpdatePrefsReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let mut db = app.db.lock().await;
    let tx = db
        .transaction()
        .map_err(|e| crate::error::ApiError::Internal(format!("begin tx: {e}")))?;

    {
        let svc = UserPrefsService::new(&tx);
        if let Some(ref mode) = body.display_mode {
            svc.update_mode(&auth.tenant_id, mode)?;
        }
        if let Some(ref quality) = body.preferred_quality {
            svc.set_preferred_quality(&auth.tenant_id, quality)?;
        }
        if let Some(ref theme) = body.theme {
            tx.execute(
                "UPDATE user_profile_prefs SET theme = ?1, updated_at = datetime('now') WHERE tenant_id = ?2",
                rusqlite::params![theme, auth.tenant_id],
            )?;
        }
    }

    tx.commit()
        .map_err(|e| crate::error::ApiError::Internal(format!("commit tx: {e}")))?;

    // Re-read updated preferences using the now-released connection.
    let svc = UserPrefsService::new(&db);
    let prefs = svc.get_or_create(&auth.tenant_id)?;
    info!(tenant_id = %auth.tenant_id, "Preferences updated");

    Ok(Json(serde_json::json!({
        "display_mode": prefs.display_mode,
        "onboarding_completed": prefs.onboarding_completed,
        "onboarding_step": prefs.onboarding_step,
        "preferred_quality": prefs.preferred_quality,
        "theme": prefs.theme,
    })))
}

#[derive(Debug, Deserialize)]
pub struct OnboardingStepReq {
    pub step: i32,
}

/// POST /preferences/onboarding — complete an onboarding step
pub async fn complete_onboarding_step(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<OnboardingStepReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    let svc = UserPrefsService::new(&db);
    svc.complete_onboarding_step(&auth.tenant_id, body.step)?;
    let prefs = svc.get_or_create(&auth.tenant_id)?;
    info!(tenant_id = %auth.tenant_id, step = body.step, "Onboarding step completed");

    Ok(Json(serde_json::json!({
        "onboarding_step": prefs.onboarding_step,
        "onboarding_completed": prefs.onboarding_completed,
    })))
}

/// POST /preferences/onboarding/complete — mark onboarding as done
pub async fn complete_onboarding(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    let svc = UserPrefsService::new(&db);
    svc.mark_onboarding_complete(&auth.tenant_id)?;
    info!(tenant_id = %auth.tenant_id, "Onboarding completed");

    Ok(Json(serde_json::json!({
        "onboarding_completed": true,
        "status": "ok",
    })))
}

// ---------------------------------------------------------------------------
//  Quality-to-model mapping
// ---------------------------------------------------------------------------

/// Maps a quality preference to (provider, model) based on available API keys.
pub fn quality_to_model(quality: &str, app: &AppState) -> (String, String) {
    match quality {
        "fast" => {
            if app.anthropic_api_key.is_some() {
                ("anthropic".into(), "claude-haiku-4-5-20251001".into())
            } else {
                ("openai".into(), "gpt-4.1-mini".into())
            }
        }
        "best" => {
            if app.anthropic_api_key.is_some() {
                ("anthropic".into(), "claude-opus-4-6".into())
            } else {
                ("openai".into(), "gpt-4.1".into())
            }
        }
        _ => {
            // "balanced" default
            if app.anthropic_api_key.is_some() {
                ("anthropic".into(), "claude-sonnet-4-6".into())
            } else {
                ("openai".into(), "gpt-4.1".into())
            }
        }
    }
}
