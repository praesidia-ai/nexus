//! Handlers for the Production Simulation Engine.

use std::sync::Arc;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::stream::Stream;
use tokio::sync::mpsc;

use crate::{
    production_sim::{self, SimConfig, SimEvent},
    security::project_access::ProjectAccess,
    state::AppState,
};

/// POST /projects/:id/simulate — run production simulation (SSE)
pub async fn simulate(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(config): Json<Option<SimConfig>>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let config = config.unwrap_or_default();
    let project_id = access.project_id.clone();
    let project_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");

    let (tx, rx) = mpsc::channel::<SimEvent>(50);

    tokio::spawn(async move {
        production_sim::simulate(app, project_dir, config, tx).await;
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
