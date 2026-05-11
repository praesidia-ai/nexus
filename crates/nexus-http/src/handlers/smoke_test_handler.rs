//! Handler for end-to-end smoke testing of running projects.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::{smoke_test::SmokeTestRunner, state::AppState};

#[derive(Deserialize)]
pub struct SmokeTestRequest {
    /// Base URL of the running app (e.g. "http://localhost:3000").
    pub app_url: String,
    /// Optional list of page names to test.
    #[serde(default)]
    pub pages: Vec<String>,
}

/// `POST /projects/:id/smoke-test`
///
/// Run smoke tests against a running project instance.
pub async fn run_smoke_test(
    State(_state): State<Arc<AppState>>,
    Path(_project_id): Path<String>,
    Json(body): Json<SmokeTestRequest>,
) -> Result<Json<crate::smoke_test::SmokeTestReport>, (StatusCode, String)> {
    let runner = SmokeTestRunner::new();
    let report = runner.run(&body.app_url, &body.pages).await;

    Ok(Json(report))
}
