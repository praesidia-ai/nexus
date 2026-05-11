//! Handlers for the Perceived Speed Engine.

use std::sync::Arc;

use axum::{
    extract::State,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    error::ApiResult,
    perceived_speed::{self, SpeedEstimator},
    security::project_access::ProjectAccess,
    state::AppState,
};

/// GET /projects/:id/speed/estimate — estimate pipeline duration
pub async fn estimate_pipeline(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let project_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");

    let estimator = SpeedEstimator::load(&project_dir);
    let steps = &[
        "analyze_intent",
        "generate_spec",
        "validate_spec",
        "generate_schema",
        "generate_api",
        "generate_ui",
        "generate_agents",
        "validate_files",
        "write_to_disk",
        "install_deps",
        "start_health",
    ];
    let estimate = estimator.estimate_pipeline(steps);

    Ok(Json(json!(estimate)))
}

/// POST /projects/:id/speed/skeletons — generate optimistic skeletons
#[derive(Deserialize)]
pub struct SkeletonRequest {
    pub app_type: String,
    pub pages: Vec<String>,
}

pub async fn generate_skeletons(
    access: ProjectAccess,
    Json(request): Json<SkeletonRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let _project_id = access.project_id;
    let skeletons = perceived_speed::generate_skeletons(&request.app_type, &request.pages);
    Ok(Json(json!({ "skeletons": skeletons })))
}
