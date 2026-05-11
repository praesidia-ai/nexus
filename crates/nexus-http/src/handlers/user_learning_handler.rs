//! User Learning System HTTP handlers.

use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::json;

use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
    user_learning,
};

#[derive(Debug, Deserialize)]
pub struct SetPreferenceRequest {
    pub category: String,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct ObserveRequest {
    pub category: String,
    pub key: String,
    pub value: String,
    pub source: String,
    #[serde(default = "default_signal")]
    pub signal_strength: f64,
}

fn default_signal() -> f64 { 0.5 }

/// GET /learning/profile — get the current user preference profile
pub async fn get_profile(
    State(app): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let profile = user_learning::get_profile(&app).await;
    Ok(Json(serde_json::to_value(profile).unwrap_or(json!({}))))
}

/// GET /learning/context — get preference context for LLM injection
pub async fn get_context(
    State(app): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let ctx = user_learning::build_adapted_context(&app).await;
    Ok(Json(serde_json::to_value(ctx).unwrap_or(json!({}))))
}

/// POST /learning/preferences — explicitly set a preference
pub async fn set_preference(
    State(app): State<Arc<AppState>>,
    Json(body): Json<SetPreferenceRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let valid_categories = ["ui_style", "tech_stack", "product_pattern"];
    if !valid_categories.contains(&body.category.as_str()) {
        return Err(ApiError::BadRequest(format!(
            "Invalid category '{}'. Use: {}",
            body.category,
            valid_categories.join(", ")
        )));
    }

    user_learning::set_explicit(&app, &body.category, &body.key, &body.value).await;

    Ok(Json(json!({
        "set": true,
        "category": body.category,
        "key": body.key,
        "value": body.value,
        "source": "explicit",
        "confidence": 1.0,
    })))
}

/// POST /learning/observe — record a preference observation from behavior
pub async fn observe_preference(
    State(app): State<Arc<AppState>>,
    Json(body): Json<ObserveRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    user_learning::observe(&app, &user_learning::PreferenceObservation {
        category: body.category.clone(),
        key: body.key.clone(),
        value: body.value.clone(),
        source: body.source.clone(),
        signal_strength: body.signal_strength,
    })
    .await;

    Ok(Json(json!({ "observed": true })))
}

/// POST /learning/decay — manually trigger confidence decay
pub async fn trigger_decay(
    State(app): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    user_learning::decay_confidences(&app).await;
    Ok(Json(json!({ "decayed": true })))
}

/// DELETE /learning/preferences/:category/:key — remove a preference
pub async fn delete_preference(
    State(app): State<Arc<AppState>>,
    axum::extract::Path((category, key)): axum::extract::Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    let n = db
        .execute(
            "DELETE FROM user_preferences WHERE category = ?1 AND key = ?2",
            rusqlite::params![category, key],
        )
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(json!({ "deleted": n > 0 })))
}
