//! HTTP handlers for Intent Engine endpoints.
//!
//! `analyze` and `clarify` are global (not project-scoped) but still
//! require an authenticated `AuthContext` so the call is attributable to
//! a tenant for cost tracking, rate limiting, and forensic logging. The
//! optional semantic-fallback path inside `analyze` drives an LLM call;
//! that path acquires a global LLM concurrency slot before reaching the
//! provider so a single tenant cannot drain the platform key by hammering
//! `/intent/analyze` with low-confidence inputs.

use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;

use crate::{
    error::{ApiError, ApiResult},
    intent_engine,
    security::auth::AuthContext,
    state::AppState,
};

/// Request body for the analyze endpoint.
#[derive(Deserialize)]
pub struct AnalyzeReq {
    /// The user's natural-language app description.
    pub description: String,
    /// If true, run the LLM semantic fallback for low-confidence fields.
    #[serde(default)]
    pub use_semantic_fallback: bool,
}

/// Request body for the clarify endpoint.
#[derive(Deserialize)]
pub struct ClarifyReq {
    /// The user's natural-language app description.
    pub description: String,
}

/// POST /intent/analyze -- analyze a description with the layered intent engine.
///
/// Returns the full Intent with confidence scores, domain pack matching,
/// and optionally LLM-enhanced semantic analysis.
pub async fn analyze(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<AnalyzeReq>,
) -> ApiResult<Json<serde_json::Value>> {
    if body.description.trim().is_empty() {
        return Err(ApiError::BadRequest("description cannot be empty".into()));
    }
    crate::input_limits::require_bounded(
        "description",
        &body.description,
        crate::input_limits::MAX_CHAT_MESSAGE_BYTES,
    )?;

    // Step 1: Deterministic analysis (always runs, instant)
    let mut intent = intent_engine::analyze(&body.description);

    // Step 2: Optional semantic fallback for low-confidence results.
    // Lives in a sibling module to keep `intent_engine` LLM-free per the
    // CLAUDE.md determinism invariant. The LLM slot is acquired ONLY on
    // this branch — unconditional acquisition would penalise the common
    // case where the deterministic path returns a high-confidence answer.
    if body.use_semantic_fallback && intent.meta.overall_confidence < 0.7 {
        if let Err(reason) = app.tenant_rate_limiter.check(&auth.tenant_id, "generation") {
            return Err(ApiError::TooManyRequests(reason));
        }
        let _llm_slot = app.rate_limiter.acquire_llm_slot().await.map_err(|e| {
            ApiError::TooManyRequests(format!("intent semantic queue is full: {e}"))
        })?;

        intent = crate::intent_engine_semantic::semantic_fallback(&app, &body.description, &intent)
            .await;
    }

    let json_value =
        serde_json::to_value(&intent).map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json_value))
}

/// POST /intent/clarify -- get clarification questions for an ambiguous description.
///
/// Returns a list of questions sorted by urgency (lowest confidence first).
pub async fn clarify(
    _auth: AuthContext,
    Json(body): Json<ClarifyReq>,
) -> ApiResult<Json<serde_json::Value>> {
    if body.description.trim().is_empty() {
        return Err(ApiError::BadRequest("description cannot be empty".into()));
    }
    crate::input_limits::require_bounded(
        "description",
        &body.description,
        crate::input_limits::MAX_CHAT_MESSAGE_BYTES,
    )?;

    // Pure deterministic — no LLM, no slot.
    let intent = intent_engine::analyze(&body.description);
    let questions = intent_engine::generate_clarifications(&intent);

    let response = serde_json::json!({
        "questions": questions,
        "overall_confidence": intent.meta.overall_confidence,
        "ambiguity_flags": intent.meta.ambiguity_flags,
    });

    Ok(Json(response))
}
