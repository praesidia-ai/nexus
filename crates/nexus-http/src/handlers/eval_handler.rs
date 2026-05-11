//! Evaluation endpoints — run suites, list built-in suites, view results.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use nexus_eval::{
    runner::EvalRunner,
    suite::{EvalSuite, SuiteResult},
};

use crate::state::AppState;

// ── In-memory result store ──────────────────────────────────────────────────

/// Thread-safe store for past evaluation results.
///
/// In a production setup this would be persisted to SQLite; for now we keep
/// results in memory so the API is functional without schema changes.
pub type EvalResultStore = Arc<Mutex<Vec<SuiteResult>>>;

/// Create a new empty result store.
pub fn new_result_store() -> EvalResultStore {
    Arc::new(Mutex::new(Vec::new()))
}

// ── Request / Response DTOs ─────────────────────────────────────────────────

/// Request body for `POST /eval/run`.
#[derive(Debug, Deserialize)]
pub struct RunEvalRequest {
    /// Either an inline suite definition **or** the ID of a built-in suite.
    #[serde(default)]
    pub suite: Option<EvalSuite>,
    /// If `suite` is not provided, look up a built-in suite by this ID.
    #[serde(default)]
    pub suite_id: Option<String>,
}

/// Summary returned from `GET /eval/suites`.
#[derive(Debug, Serialize)]
pub struct SuiteSummary {
    /// Suite identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Number of cases in the suite.
    pub case_count: usize,
}

// ── Built-in suites ─────────────────────────────────────────────────────────

/// Return the list of built-in evaluation suites shipped with Nexus.
fn builtin_suites() -> Vec<EvalSuite> {
    let mut suites = Vec::new();

    // Attempt to load from the examples directory embedded at compile-time.
    // If the JSON files are not available (e.g. stripped binary), fall back to
    // an empty list.
    if let Ok(suite) = serde_json::from_str::<EvalSuite>(include_str!(
        "../../../nexus-eval/examples/evals/gen-quality-v1.json"
    )) {
        suites.push(suite);
    }
    if let Ok(suite) = serde_json::from_str::<EvalSuite>(include_str!(
        "../../../nexus-eval/examples/evals/intent-accuracy-v1.json"
    )) {
        suites.push(suite);
    }

    suites
}

/// Find a built-in suite by ID.
fn find_builtin(id: &str) -> Option<EvalSuite> {
    builtin_suites().into_iter().find(|s| s.id == id)
}

// ── Handlers ────────────────────────────────────────────────────────────────

/// `POST /eval/run` — Run an evaluation suite.
///
/// Accepts either an inline `suite` JSON body or a `suite_id` referencing
/// a built-in suite. Returns the full [`SuiteResult`].
pub async fn run_eval(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RunEvalRequest>,
) -> Result<Json<SuiteResult>, (StatusCode, Json<serde_json::Value>)> {
    let suite = if let Some(s) = body.suite {
        s
    } else if let Some(id) = body.suite_id {
        find_builtin(&id).ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": format!("suite not found: {id}")
                })),
            )
        })?
    } else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "provide either `suite` or `suite_id`"
            })),
        ));
    };

    let runner = EvalRunner::new();
    let result = runner.run_suite(&suite).await;

    // Persist in-memory.
    let store = state.eval_results.lock().await;
    // We need to drop the lock before returning, so clone the result first.
    drop(store);
    {
        let mut store = state.eval_results.lock().await;
        store.push(result.clone());
    }

    Ok(Json(result))
}

/// `GET /eval/suites` — List built-in evaluation suites.
pub async fn list_suites() -> Json<Vec<SuiteSummary>> {
    let summaries = builtin_suites()
        .into_iter()
        .map(|s| SuiteSummary {
            id: s.id,
            name: s.name,
            description: s.description,
            case_count: s.cases.len(),
        })
        .collect();
    Json(summaries)
}

/// `GET /eval/results` — List past evaluation results (most recent first).
pub async fn list_results(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<SuiteResult>> {
    let store = state.eval_results.lock().await;
    let mut results = store.clone();
    results.reverse();
    Json(results)
}
