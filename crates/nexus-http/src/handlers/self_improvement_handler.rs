//! HTTP handlers for the self-improvement engine.
//!
//! Exposes endpoints to trigger post-generation learning, retrieve
//! self-evaluation reports, and manage the eval-gated skill promotion loop.
//!
//! SECURITY: `learn` accepts a `project_id` in the body. Without auth + tenant
//! validation any caller could inject learning signals targeting another
//! tenant's project (cross-tenant signal poisoning, same shape as the
//! `decision_learning_handler` bug). The handler now requires
//! `AuthContext` and validates the body's `project_id` against the caller's
//! tenant before any state mutation.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;

use crate::security::auth::AuthContext;
use crate::security::tenant::validate_project_access;
use crate::self_improvement_engine::SelfImprovementEngine;
use crate::skill_dna;
use crate::state::AppState;

/// Request body for triggering post-generation learning.
#[derive(Debug, Deserialize)]
pub struct LearnRequest {
    /// ID of the project that was generated.
    pub project_id: String,
    /// The original description provided by the user.
    pub description: String,
    /// Taste/quality score achieved (0.0 to 1.0).
    pub taste_score: f32,
    /// Whether the generated project built successfully.
    pub build_success: bool,
    /// Total generation duration in milliseconds.
    pub duration_ms: u64,
}

/// `POST /self-improvement/learn` — trigger post-generation learning.
///
/// Requires authentication. The body's `project_id` is verified to belong to
/// the caller's tenant before learning runs — otherwise a tenant could poison
/// another tenant's learning history with arbitrary outcomes.
pub async fn learn(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<LearnRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    // Tenant guard against the body-supplied project_id.
    {
        let db = state.db.lock().await;
        if let Err(reason) = validate_project_access(&db, &body.project_id, &auth.tenant_id) {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": reason })),
            );
        }
    }

    // Bound the description so a tenant cannot make every learning call cost
    // megabytes of memory + storage. The cap mirrors what oneshot/intent use.
    if let Err(api_err) = crate::input_limits::require_bounded(
        "description",
        &body.description,
        crate::input_limits::MAX_CHAT_MESSAGE_BYTES,
    ) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": api_err.to_string() })),
        );
    }

    let engine = SelfImprovementEngine::new();
    match engine
        .post_generation_learning(
            &state,
            &body.project_id,
            &body.description,
            body.taste_score,
            body.build_success,
            body.duration_ms,
        )
        .await
    {
        Ok(outcome) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "episodes_recorded": outcome.episodes_recorded,
                "signals_recorded": outcome.signals_recorded,
                "patterns_learned": outcome.patterns_learned,
            })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

/// `GET /self-improvement/report` — get the latest self-evaluation report.
pub async fn report(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let engine = SelfImprovementEngine::new();
    let report = engine.run_self_evaluation(&state).await;

    Json(serde_json::json!({
        "total_generations": report.total_generations,
        "avg_taste_score": report.avg_taste_score,
        "build_success_rate": report.build_success_rate,
        "trend": report.trend,
        "weak_areas": report.weak_areas,
    }))
}

// ---------------------------------------------------------------------------
// Skill Library — pattern extraction + eval-gated promotion
// ---------------------------------------------------------------------------

/// `GET /self-improvement/skills` — list all learned skills in the library.
pub async fn list_skills(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let skills = skill_dna::get_skills_for_intent(&state, "").await;
    Json(serde_json::json!({
        "skills": skills,
        "count": skills.len(),
    }))
}

/// `GET /self-improvement/suggested-skills` — list draft patterns Nexus has
/// proposed based on the user's history. These haven't been promoted yet,
/// so they're advisory; the UI can show them as "Nexus noticed you often
/// build X — create a shortcut?" cards.
///
/// Unlike `list_skills` (active only), this returns `draft` rows with the
/// `pattern:` prefix that `user_pattern_detector` emits.
pub async fn list_suggested_skills(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let db = state.db.lock().await;
    let mut stmt = match db.prepare(
        "SELECT id, name, description, intent, prompt_fragment, confidence, total_uses,
                patterns, examples, created_at
         FROM skill_dna
         WHERE status = 'draft' AND source_type = 'auto' AND description LIKE 'pattern:%'
         ORDER BY confidence DESC, created_at DESC
         LIMIT 20",
    ) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "list_suggested_skills query failed");
            return Json(serde_json::json!({ "skills": [], "count": 0 }));
        }
    };
    let rows = stmt
        .query_map([], |row| {
            let patterns: String = row.get(7)?;
            let examples: String = row.get(8)?;
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "description": row.get::<_, String>(2)?,
                "intent": row.get::<_, String>(3)?,
                "prompt_fragment": row.get::<_, String>(4)?,
                "confidence": row.get::<_, f64>(5)?,
                "total_uses": row.get::<_, i64>(6)?,
                "patterns": serde_json::from_str::<serde_json::Value>(&patterns).unwrap_or(serde_json::Value::Null),
                "examples": serde_json::from_str::<serde_json::Value>(&examples).unwrap_or(serde_json::Value::Null),
                "created_at": row.get::<_, String>(9)?,
            }))
        });
    let skills: Vec<serde_json::Value> = match rows {
        Ok(r) => r.filter_map(|x| x.ok()).collect(),
        Err(_) => Vec::new(),
    };
    Json(serde_json::json!({
        "skills": skills.clone(),
        "count": skills.len(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct ExtractPatternsRequest {
    /// The project ID whose execution trace to mine.
    pub project_id: String,
    /// The original task description for tagging patterns.
    pub description: String,
    /// Outcome quality (0.0–1.0); patterns from high-quality outcomes are preferred.
    pub quality_score: f32,
}

/// `POST /self-improvement/extract` — extract reusable patterns from a completed execution.
///
/// This runs the pattern-extraction pipeline on an execution trace and stores
/// any discovered skills in the library with a pending promotion status.
pub async fn extract_patterns(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ExtractPatternsRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let success = body.quality_score >= 0.6;
    let skill = skill_dna::extract_from_execution(
        &body.project_id,
        &body.description,
        &[],
        &[],
        &[],
        success,
    );

    let Some(skill) = skill else {
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "extracted": false,
                "reason": "no reusable patterns found or execution was unsuccessful",
            })),
        );
    };

    match skill_dna::store_skill(&state, &skill).await {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "extracted": true,
                "skill_id": skill.id,
                "name": skill.name,
                "patterns": skill.patterns.len(),
                "status": "pending_eval",
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        ),
    }
}

/// `POST /self-improvement/promote` — run eval gates and promote ready skills.
///
/// Checks all pending skills against the configured quality thresholds.
/// Skills that pass are promoted to "active" status and become available
/// to the agent's runtime context.
pub async fn promote_skills(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let promoted = skill_dna::promote_if_ready(&state).await;
    Json(serde_json::json!({
        "promoted": promoted,
        "message": format!("{promoted} skill(s) promoted after eval gating"),
    }))
}

#[derive(Debug, Deserialize)]
pub struct RecordUsageRequest {
    pub skill_id: String,
    pub success: bool,
}

/// `POST /self-improvement/skills/usage` — record the outcome of using a skill.
///
/// Feeds back runtime success/failure signals so the library can decay
/// low-performing skills and amplify high-performing ones.
pub async fn record_skill_usage(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RecordUsageRequest>,
) -> Json<serde_json::Value> {
    skill_dna::record_skill_usage(&state, &body.skill_id, body.success).await;
    Json(serde_json::json!({ "recorded": true, "skill_id": body.skill_id }))
}

#[derive(Debug, Deserialize)]
pub struct ComposePromptRequest {
    pub intent: String,
    pub base_prompt: String,
    pub max_skills: Option<usize>,
}

/// `POST /self-improvement/compose-prompt` — compose an agent prompt enriched with learned skills.
///
/// Retrieves the top matching skills for the intent and injects them into
/// the base system prompt, producing a self-improving agent context.
pub async fn compose_prompt(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ComposePromptRequest>,
) -> Json<serde_json::Value> {
    let enriched = skill_dna::compose_prompt(
        &state,
        &body.base_prompt,
        &body.intent,
    )
    .await;
    Json(serde_json::json!({ "prompt": enriched }))
}
