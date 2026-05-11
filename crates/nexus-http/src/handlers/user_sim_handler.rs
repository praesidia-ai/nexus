//! Handlers for End-to-End User Simulation Testing.

use std::sync::Arc;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::stream::Stream;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;

use crate::{
    error::{ApiError, ApiResult},
    security::project_access::ProjectAccess,
    state::AppState,
    user_sim,
};

/// GET /projects/:id/sim-test/journeys — generate test journeys from project
pub async fn list_journeys(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let project_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");
    if !project_dir.exists() {
        return Err(ApiError::BadRequest("No generated code".into()));
    }

    let journeys = user_sim::generate_journeys(&project_dir);
    Ok(Json(json!({ "journeys": journeys })))
}

#[derive(Deserialize)]
pub struct RunQuery {
    pub base_url: Option<String>,
}

/// POST /projects/:id/sim-test/run — run all simulated user journeys (SSE)
pub async fn run_tests(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    axum::extract::Query(query): axum::extract::Query<RunQuery>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let project_id = access.project_id.clone();
    let project_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");

    let base_url = query
        .base_url
        .unwrap_or_else(|| "http://localhost:3000".to_string());

    let (tx, rx) = mpsc::channel(50);

    tokio::spawn(async move {
        let journeys = user_sim::generate_journeys(&project_dir);
        user_sim::run_journeys(&base_url, &journeys, tx).await;
    });

    let stream = async_stream::stream! {
        let mut rx = rx;
        while let Some(event) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&event) {
                yield Ok(Event::default().data(json));
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    )
}
