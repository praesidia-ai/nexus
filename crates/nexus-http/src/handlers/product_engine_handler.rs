//! Product Engine HTTP handlers.

use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::json;

use crate::{
    error::{ApiError, ApiResult},
    intent_engine,
    product_engine,
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct ProductBriefRequest {
    pub description: String,
    /// Deprecated — full brief is always generated. Kept for API compat.
    #[serde(default)]
    pub v2: bool,
}

/// POST /product/brief — generate a full product brief from a description
pub async fn generate_brief(
    State(_app): State<Arc<AppState>>,
    Json(body): Json<ProductBriefRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if body.description.trim().is_empty() {
        return Err(ApiError::BadRequest("description is required".into()));
    }

    let intent = intent_engine::analyze_flat(&body.description);

    let brief = product_engine::generate_full_product_brief(&intent, &body.description);
    Ok(Json(serde_json::to_value(brief).unwrap_or(json!({}))))
}

/// POST /product/brief/prompt — generate brief and format as LLM prompt context
pub async fn generate_brief_prompt(
    State(_app): State<Arc<AppState>>,
    Json(body): Json<ProductBriefRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if body.description.trim().is_empty() {
        return Err(ApiError::BadRequest("description is required".into()));
    }

    let intent = intent_engine::analyze_flat(&body.description);
    let brief = product_engine::generate_full_product_brief(&intent, &body.description);
    let prompt_context = product_engine::format_full_brief_for_prompt(&brief);

    Ok(Json(json!({
        "domain": brief.base.domain,
        "prompt_context": prompt_context,
        "personas_count": brief.personas.len(),
        "onboarding_steps": brief.onboarding.steps.len(),
        "feature_priorities": brief.feature_priorities.len(),
        "monetization_model": brief.monetization.model,
    })))
}

/// POST /product/personas — generate target user personas for a description
pub async fn generate_personas(
    State(_app): State<Arc<AppState>>,
    Json(body): Json<ProductBriefRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if body.description.trim().is_empty() {
        return Err(ApiError::BadRequest("description is required".into()));
    }

    let intent = intent_engine::analyze_flat(&body.description);
    let brief = product_engine::generate_full_product_brief(&intent, &body.description);

    Ok(Json(serde_json::to_value(brief.personas).unwrap_or(json!([]))))
}

/// POST /product/monetization — generate monetization strategy for a description
pub async fn generate_monetization(
    State(_app): State<Arc<AppState>>,
    Json(body): Json<ProductBriefRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if body.description.trim().is_empty() {
        return Err(ApiError::BadRequest("description is required".into()));
    }

    let intent = intent_engine::analyze_flat(&body.description);
    let brief = product_engine::generate_full_product_brief(&intent, &body.description);

    Ok(Json(serde_json::to_value(brief.monetization).unwrap_or(json!({}))))
}

/// POST /product/onboarding — generate onboarding flow for a description
pub async fn generate_onboarding(
    State(_app): State<Arc<AppState>>,
    Json(body): Json<ProductBriefRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if body.description.trim().is_empty() {
        return Err(ApiError::BadRequest("description is required".into()));
    }

    let intent = intent_engine::analyze_flat(&body.description);
    let brief = product_engine::generate_full_product_brief(&intent, &body.description);

    Ok(Json(serde_json::to_value(brief.onboarding).unwrap_or(json!({}))))
}

/// POST /product/retention — generate retention loop for a description
pub async fn generate_retention(
    State(_app): State<Arc<AppState>>,
    Json(body): Json<ProductBriefRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if body.description.trim().is_empty() {
        return Err(ApiError::BadRequest("description is required".into()));
    }

    let intent = intent_engine::analyze_flat(&body.description);
    let brief = product_engine::generate_full_product_brief(&intent, &body.description);

    Ok(Json(serde_json::to_value(brief.retention).unwrap_or(json!({}))))
}

/// POST /product/features — generate prioritized feature list
pub async fn prioritize_features(
    State(_app): State<Arc<AppState>>,
    Json(body): Json<ProductBriefRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if body.description.trim().is_empty() {
        return Err(ApiError::BadRequest("description is required".into()));
    }

    let intent = intent_engine::analyze_flat(&body.description);
    let brief = product_engine::generate_full_product_brief(&intent, &body.description);

    Ok(Json(json!({
        "features": brief.feature_priorities,
        "total": brief.feature_priorities.len(),
    })))
}

/// POST /product/landing-copy — generate landing page copy
pub async fn generate_landing_copy(
    State(_app): State<Arc<AppState>>,
    Json(body): Json<ProductBriefRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if body.description.trim().is_empty() {
        return Err(ApiError::BadRequest("description is required".into()));
    }

    let intent = intent_engine::analyze_flat(&body.description);
    let brief = product_engine::generate_full_product_brief(&intent, &body.description);

    Ok(Json(serde_json::to_value(brief.landing_page_copy).unwrap_or(json!({}))))
}
