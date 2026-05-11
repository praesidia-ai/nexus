//! Learning Memory HTTP handlers — unified adaptive learning API.
//!
//! Exposes endpoints for recording learning signals and querying aggregated
//! insights, preferences, and quality baselines.

use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::json;

use crate::{
    error::{ApiError, ApiResult},
    learning_memory::{LearningMemory, LearningSignal, SignalType},
    security::auth::AuthContext,
    state::AppState,
};

// ---------------------------------------------------------------------------
// Request / query types
// ---------------------------------------------------------------------------

/// Request body for `POST /learning/signal`.
///
/// Note: `tenant_id` is intentionally NOT in this struct — it is sourced from
/// the authenticated [`AuthContext`] to prevent cross-tenant signal injection.
#[derive(Debug, Deserialize)]
pub struct RecordSignalRequest {
    /// Unique signal id (generated client-side or server-side).
    pub id: Option<String>,
    /// Optional project scope.
    pub project_id: Option<String>,
    /// Signal type payload.
    pub signal_type: SignalType,
    /// Arbitrary context (JSON object).
    #[serde(default)]
    pub context: serde_json::Value,
    /// Arbitrary outcome (JSON object).
    #[serde(default)]
    pub outcome: serde_json::Value,
    /// Quality score.
    #[serde(default)]
    pub quality: f32,
    /// ISO-8601 timestamp; defaults to now if omitted.
    pub timestamp: Option<String>,
}

/// Query parameters for `GET /learning/best-choice`.
#[derive(Debug, Deserialize)]
pub struct BestChoiceQuery {
    /// Decision area (e.g. "framework", "database").
    pub area: String,
    /// Application type to scope the recommendation.
    #[serde(default)]
    pub app_type: String,
    /// Tenant identifier.
    #[serde(default)]
    pub tenant_id: String,
}

/// Query parameters for `GET /learning/style`.
#[derive(Debug, Deserialize)]
pub struct StyleQuery {
    /// Tenant identifier.
    #[serde(default)]
    pub tenant_id: String,
}

/// Query parameters for `GET /learning/quality-baseline`.
#[derive(Debug, Deserialize)]
pub struct QualityBaselineQuery {
    /// Application type to scope the baseline.
    #[serde(default)]
    pub app_type: String,
}

/// Query parameters for `GET /learning/insights`.
#[derive(Debug, Deserialize)]
pub struct InsightsQuery {
    /// Tenant identifier.
    #[serde(default)]
    pub tenant_id: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `POST /learning/signal` — record a single learning signal.
pub async fn record_signal(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<RecordSignalRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let mem = LearningMemory::new(&app.data_dir);
    mem.init_tables().map_err(ApiError::Internal)?;

    let id = body
        .id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let timestamp = body
        .timestamp
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    let signal = LearningSignal {
        id: id.clone(),
        // tenant_id is sourced from the authenticated context to prevent
        // cross-tenant signal injection from request bodies.
        tenant_id: auth.tenant_id.clone(),
        project_id: body.project_id.clone(),
        signal_type: body.signal_type,
        context: body.context,
        outcome: body.outcome,
        quality: body.quality,
        timestamp,
    };

    mem.record(&signal).map_err(ApiError::Internal)?;

    Ok(Json(json!({
        "recorded": true,
        "id": id,
        "tenant_id": auth.tenant_id,
    })))
}

/// `GET /learning/insights` — dashboard-level learning insights for a tenant.
pub async fn get_insights(
    State(app): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<InsightsQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let mem = LearningMemory::new(&app.data_dir);
    mem.init_tables().map_err(ApiError::Internal)?;

    let insights = mem.insights(&params.tenant_id);
    Ok(Json(
        serde_json::to_value(insights).unwrap_or_else(|_| json!({})),
    ))
}

/// `GET /learning/best-choice` — query the best choice for a decision area.
pub async fn get_best_choice(
    State(app): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<BestChoiceQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    if params.area.is_empty() {
        return Err(ApiError::BadRequest(
            "area query parameter is required".into(),
        ));
    }

    let mem = LearningMemory::new(&app.data_dir);
    mem.init_tables().map_err(ApiError::Internal)?;

    match mem.best_choice(&params.area, &params.app_type, &params.tenant_id) {
        Some(pref) => Ok(Json(
            serde_json::to_value(pref).unwrap_or_else(|_| json!({})),
        )),
        None => Ok(Json(json!({
            "area": params.area,
            "recommendation": null,
            "reason": "No learning data yet for this area",
        }))),
    }
}

/// `GET /learning/style` — get aggregated style preferences for a tenant.
pub async fn get_style(
    State(app): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<StyleQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let mem = LearningMemory::new(&app.data_dir);
    mem.init_tables().map_err(ApiError::Internal)?;

    let style = mem.style_preferences(&params.tenant_id);
    Ok(Json(
        serde_json::to_value(style).unwrap_or_else(|_| json!({})),
    ))
}

/// `GET /learning/quality-baseline` — get quality baseline statistics.
pub async fn get_quality_baseline(
    State(app): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<QualityBaselineQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let mem = LearningMemory::new(&app.data_dir);
    mem.init_tables().map_err(ApiError::Internal)?;

    let baseline = mem.quality_baseline(&params.app_type);
    Ok(Json(
        serde_json::to_value(baseline).unwrap_or_else(|_| json!({})),
    ))
}
