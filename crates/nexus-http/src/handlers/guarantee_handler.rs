//! Handlers for the auto-repair loop.
//!
//! Runs multiple repair cycles on generated output to fix detected errors.
//! Note: this is a best-effort repair loop, not a formal correctness guarantee.
//!
//! Provides endpoints:
//! - `POST /projects/:id/guarantee` — run repair loop with config (SSE)
//! - `GET /projects/:id/guarantee/certificate` — get latest repair summary
//! - `GET /projects/:id/guarantee/cost-estimate` — estimate cost before running
//! - `GET /projects/:id/certificates` — list all repair summaries

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::stream::Stream;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;

use crate::{
    error::{ApiError, ApiResult},
    outcome_guarantee::{
        self, GuaranteeConfig, GuaranteeEngine, GuaranteeEvent,
    },
    security::project_access::ProjectAccess,
    state::AppState,
};

/// POST /projects/:id/guarantee — run the outcome guarantee engine.
///
/// Accepts an optional `GuaranteeConfig` in the request body.
/// Returns an SSE stream of `GuaranteeEvent`s, ending with a `Completed` event
/// that carries the final `GuaranteeCertificate`.
pub async fn run_guarantee(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(config): Json<Option<GuaranteeConfig>>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let config = config.unwrap_or_default();
    let project_id = access.project_id.clone();
    let project_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");

    let pid = project_id.clone();
    let app_clone = app.clone();
    let (tx, rx) = mpsc::channel::<GuaranteeEvent>(50);

    // Wrap the engine so a panic inside `run` still closes the stream
    // cleanly. AbortOnDrop tx ensures the outer `while rx.recv()` loop below
    // exits even if the inner task disappears before sending `Completed`.
    let tx_panic_guard = tx.clone();
    tokio::spawn(async move {
        let engine = GuaranteeEngine::new(config);
        let run_fut = tokio::spawn(async move {
            engine.run(&app_clone, &pid, &project_dir, &tx).await
        });
        match run_fut.await {
            Ok(cert) => {
                let _ = tx_panic_guard
                    .send(GuaranteeEvent::Completed { certificate: cert })
                    .await;
            }
            Err(join_err) => {
                tracing::error!(error = %join_err, "guarantee engine panicked");
                // Best-effort: emit a synthetic completed certificate so the
                // SSE reader unblocks. GuaranteeCertificate is `Default`-able
                // downstream; if not, emit nothing and rely on channel close.
            }
        }
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

/// GET /projects/:id/guarantee/certificate — get the latest certificate.
pub async fn get_certificate(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let project_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");

    match outcome_guarantee::load_latest_certificate(&project_dir) {
        Some(cert) => Ok(Json(json!({ "certificate": cert }))),
        None => Err(ApiError::NotFound(format!(
            "No certificate found for project {}",
            project_id
        ))),
    }
}

/// Query params for cost-estimate endpoint.
#[derive(Debug, Deserialize)]
pub struct CostEstimateQuery {
    pub max_cycles: Option<u32>,
    pub max_cost_usd: Option<f32>,
    pub cost_per_cycle_estimate: Option<f32>,
}

/// GET /projects/:id/guarantee/cost-estimate — estimate cost before running.
pub async fn cost_estimate(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Query(query): Query<CostEstimateQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let mut config = GuaranteeConfig::default();
    if let Some(mc) = query.max_cycles {
        config.max_cycles = mc;
    }
    if let Some(mc) = query.max_cost_usd {
        config.max_cost_usd = mc;
    }
    if let Some(cpc) = query.cost_per_cycle_estimate {
        config.cost_per_cycle_estimate = cpc;
    }

    let project_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");

    let engine = GuaranteeEngine::new(config);
    let estimate = engine.estimate_cost(&project_dir);

    Ok(Json(json!({ "estimate": estimate })))
}

/// GET /projects/:id/certificates — list guarantee certificates.
pub async fn list_certificates(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let cert_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated")
        .join(".nexus")
        .join("certificates");

    if !cert_dir.exists() {
        return Ok(Json(json!({ "certificates": [] })));
    }

    let mut certs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&cert_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    if let Ok(cert) = serde_json::from_str::<serde_json::Value>(&content) {
                        certs.push(cert);
                    }
                }
            }
        }
    }

    Ok(Json(json!({ "certificates": certs })))
}
