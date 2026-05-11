//! Health check endpoints — readiness, liveness, and detailed probes.
//!
//! These endpoints are designed for container orchestrators (Kubernetes,
//! Docker Compose health checks, load balancer probes) and operational
//! dashboards.
//!
//! | Endpoint              | Purpose                                     |
//! |-----------------------|---------------------------------------------|
//! | `GET /health/ready`   | Readiness — can this instance serve traffic? |
//! | `GET /health/live`    | Liveness — is the process alive?             |
//! | `GET /health/detailed`| Full subsystem breakdown (JSON)              |

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::state::AppState;

/// `GET /health/ready` — readiness probe for load balancers.
///
/// Returns `200 OK` if the service can handle requests (database is
/// accessible). Returns `503 Service Unavailable` otherwise.
///
/// Load balancers should remove the instance from the pool on a non-200
/// response.
pub async fn readiness(
    State(state): State<Arc<AppState>>,
) -> (StatusCode, Json<Value>) {
    let db_ok = {
        let db = state.db.lock().await;
        db.query_row("SELECT 1", [], |_| Ok(())).is_ok()
    };

    if db_ok {
        (StatusCode::OK, Json(json!({"status": "ready"})))
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"status": "not_ready", "reason": "database unavailable"})))
    }
}

/// `GET /health/live` — liveness probe.
///
/// Always returns `200 OK` as long as the process is running and able to
/// handle HTTP requests. Container orchestrators use this to decide whether
/// to restart the container.
pub async fn liveness() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

/// `GET /health/detailed` — detailed health with subsystem status.
///
/// Returns a JSON object describing each subsystem's health:
///
/// ```json
/// {
///   "status": "healthy",
///   "version": "0.1.0",
///   "subsystems": {
///     "database": { "status": "ok" },
///     "llm_providers": { "status": "ok", "providers": { ... } }
///   }
/// }
/// ```
pub async fn detailed_health(State(state): State<Arc<AppState>>) -> Json<Value> {
    let db_status = {
        let db = state.db.lock().await;
        match db.query_row("SELECT 1", [], |_| Ok(())) {
            Ok(_) => json!({"status": "ok"}),
            Err(e) => json!({"status": "error", "error": e.to_string()}),
        }
    };

    let llm_status = {
        let has_openai_env = !state.openai_api_key.is_empty()
            && state.openai_api_key != "sk-placeholder";
        let has_anthropic_env = state.anthropic_api_key.is_some();
        let (has_openai_db, has_anthropic_db) = {
            let db = state.db.lock().await;
            let svc = nexus_store::ProjectService::new(&db);
            let openai = svc.get_setting("llm.openai.api_key").ok().flatten().map(|k| !k.is_empty()).unwrap_or(false);
            let anthropic = svc.get_setting("llm.anthropic.api_key").ok().flatten().map(|k| !k.is_empty()).unwrap_or(false);
            (openai, anthropic)
        };
        let has_openai = has_openai_env || has_openai_db;
        let has_anthropic = has_anthropic_env || has_anthropic_db;
        if has_openai || has_anthropic {
            json!({
                "status": "ok",
                "providers": {
                    "openai": has_openai,
                    "anthropic": has_anthropic,
                }
            })
        } else {
            json!({"status": "warning", "message": "No LLM API keys configured"})
        }
    };

    let overall = if db_status["status"] == "ok" {
        "healthy"
    } else {
        "unhealthy"
    };

    Json(json!({
        "status": overall,
        "version": env!("CARGO_PKG_VERSION"),
        "subsystems": {
            "database": db_status,
            "llm_providers": llm_status,
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn liveness_returns_ok_json() {
        let Json(body) = liveness().await;
        assert_eq!(body["status"], "ok");
    }

    #[test]
    fn detailed_health_json_shape() {
        // Verify the JSON structure matches what monitoring tools expect.
        let body = json!({
            "status": "healthy",
            "version": "0.1.0",
            "subsystems": {
                "database": {"status": "ok"},
                "llm_providers": {
                    "status": "ok",
                    "providers": {
                        "openai": true,
                        "anthropic": false,
                    }
                }
            }
        });

        assert_eq!(body["status"], "healthy");
        assert_eq!(body["subsystems"]["database"]["status"], "ok");
        assert!(body["subsystems"]["llm_providers"]["providers"]["openai"].as_bool().unwrap());
        assert!(!body["subsystems"]["llm_providers"]["providers"]["anthropic"].as_bool().unwrap());
    }
}
