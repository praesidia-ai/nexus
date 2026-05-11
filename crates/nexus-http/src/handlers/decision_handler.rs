//! Decision Engine HTTP handlers — explainable decisions with coherence validation.

use std::sync::Arc;

use axum::{
    extract::State,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    decision_engine::DecisionEngine,
    error::{ApiError, ApiResult},
    security::auth::Scope,
    security::project_access::ProjectAccess,
    state::AppState,
};

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

/// Request body for applying a decision override.
#[derive(Debug, Deserialize)]
pub struct OverrideRequest {
    /// The decision area to override (e.g. "database", "hosting").
    pub area: String,
    /// The new choice value (e.g. "postgres", "docker").
    pub choice: String,
    /// Reason for the override.
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /projects/:id/decisions/explain
///
/// Analyse the project's intent and return a full decision explanation tree
/// including confidence scores, factors, alternatives, and coherence report.
pub async fn explain_decisions(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    // Load the project's stored intent.
    let intent = load_intent(&app, &project_id).await?;

    let engine = DecisionEngine::new();
    let decision_set = engine.decide(&intent);
    let legacy = engine.to_legacy(&decision_set);

    Ok(Json(json!({
        "project_id": project_id,
        "decisions": decision_set.decisions,
        "coherence": decision_set.coherence,
        "legacy": legacy,
    })))
}

/// POST /projects/:id/decisions/override
///
/// Apply a manual override to one decision area and re-validate coherence.
/// Returns the updated decision set.
pub async fn override_decision(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(body): Json<OverrideRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    let project_id = access.project_id.clone();
    let intent = load_intent(&app, &project_id).await?;

    let engine = DecisionEngine::new();
    let mut decision_set = engine.decide(&intent);

    // Validate the area is known.
    let valid_areas = ["frontend", "backend", "database", "auth", "hosting", "ui_library"];
    if !valid_areas.contains(&body.area.as_str()) {
        return Err(ApiError::BadRequest(format!(
            "Unknown decision area '{}'. Valid areas: {:?}",
            body.area, valid_areas
        )));
    }

    let coherence = engine.apply_override(&mut decision_set, &body.area, &body.choice, &body.reason);
    let legacy = engine.to_legacy(&decision_set);

    Ok(Json(json!({
        "project_id": project_id,
        "overridden_area": body.area,
        "overridden_choice": body.choice,
        "decisions": decision_set.decisions,
        "coherence": coherence,
        "legacy": legacy,
    })))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Load the Intent for a project. If no analysis exists yet, run the
/// deterministic analyser on the project's stored description.
async fn load_intent(
    app: &Arc<AppState>,
    project_id: &str,
) -> Result<crate::intent_engine::Intent, ApiError> {
    // Look up the project to get its description.
    let description = {
        let db = app.db.lock().await;
        let mut stmt = db
            .prepare("SELECT description FROM projects WHERE id = ?1")
            .map_err(|e| ApiError::Internal(format!("DB error: {}", e)))?;
        let desc: Result<String, _> = stmt.query_row(rusqlite::params![project_id], |row| row.get(0));
        match desc {
            Ok(d) => d,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return Err(ApiError::NotFound(format!("Project {} not found", project_id)));
            }
            Err(e) => {
                return Err(ApiError::Internal(format!("DB error: {}", e)));
            }
        }
    };

    // Run the deterministic analysis.
    let intent = crate::intent_engine::analyze(&description);
    Ok(intent)
}
