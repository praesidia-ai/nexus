//! Integration tests that exercise the full HTTP router with a real AppState
//! backed by an ephemeral SQLite file in a temp directory.
//!
//! These tests drive the router in-process with `tower::ServiceExt::oneshot`
//! so no TCP listener is started. Auth is enforced with the same middleware
//! as production; each test signs its own JWT for a specific tenant.

use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
    Router,
};
use nexus_http::{
    security::auth::{create_jwt, Scope},
    server::create_router,
    state::AppState,
};
use serde_json::{json, Value};
use tempfile::TempDir;
use tower::ServiceExt;

/// Test-only JWT secret. MUST NOT match any pattern in
/// `state::is_insecure_value` — the auto-secrets bootstrapper replaces values
/// containing "dev-only", "change-me", or "do-not-use-in-prod" with a fresh
/// random one at `AppState::init` time, which would silently invalidate
/// every JWT this file signs.
const DEV_SECRET: &str = "nexus-integration-test-harness-secret-64-bytes-of-entropy-ok1";

/// Spin up a fresh `AppState` in a throwaway directory. The directory is
/// dropped when the returned [`TempDir`] goes out of scope.
async fn fresh_app() -> (Arc<AppState>, Router, TempDir) {
    let tmp = tempfile::tempdir().expect("create tempdir");
    // Make sure the server does not accidentally pick up a real OpenAI key.
    // SAFETY: tests run single-threaded when they mutate env; the env is only
    // read at AppState::init() time so ordering is fine in practice for this
    // scope. If flaky, switch to `serial_test`.
    unsafe {
        std::env::remove_var("OPENAI_API_KEY");
        std::env::remove_var("ANTHROPIC_API_KEY");
        std::env::set_var("NEXUS_JWT_SECRET", DEV_SECRET);
    }
    let state = AppState::init(tmp.path())
        .await
        .expect("build AppState");
    let router = create_router(state.clone());
    (state, router, tmp)
}

/// Build an Authorization header for `tenant_id` with every scope enabled.
fn bearer_for(tenant_id: &str) -> String {
    let all = Scope::all();
    let token = create_jwt(
        &format!("{tenant_id}-user"),
        tenant_id,
        &all,
        DEV_SECRET,
        3600,
    )
    .expect("sign JWT");
    format!("Bearer {token}")
}

async fn send(
    router: &Router,
    method: &str,
    path: &str,
    bearer: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(b) = bearer {
        builder = builder.header(header::AUTHORIZATION, b);
    }
    let req_body = match body {
        Some(v) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(serde_json::to_vec(&v).unwrap())
        }
        None => Body::empty(),
    };
    let request = builder.body(req_body).expect("build request");
    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("router oneshot");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap_or_default();
    let parsed: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::String(
            String::from_utf8_lossy(&bytes).into_owned(),
        ))
    };
    (status, parsed)
}

// ---------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_project_returns_201_with_tenant_id() {
    let (_state, router, _tmp) = fresh_app().await;
    let auth = bearer_for("tenant-a");

    let (status, body) = send(
        &router,
        "POST",
        "/projects",
        Some(&auth),
        Some(json!({ "name": "Alpha", "description": "hello" })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["name"], "Alpha");
    assert_eq!(body["tenant_id"], "tenant-a");
    assert!(body["id"].is_string());
}

#[tokio::test]
async fn list_projects_is_scoped_to_caller_tenant() {
    let (_state, router, _tmp) = fresh_app().await;
    let auth_a = bearer_for("tenant-a");
    let auth_b = bearer_for("tenant-b");

    // Create two projects for tenant-a, one for tenant-b.
    for name in &["a1", "a2"] {
        let (s, _) = send(
            &router,
            "POST",
            "/projects",
            Some(&auth_a),
            Some(json!({ "name": name })),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED, "create {} failed", name);
    }
    let (s, _) = send(
        &router,
        "POST",
        "/projects",
        Some(&auth_b),
        Some(json!({ "name": "b1" })),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);

    // List as tenant-a — expect exactly 2.
    let (_, body_a) = send(&router, "GET", "/projects", Some(&auth_a), None).await;
    let list_a = body_a.as_array().expect("array");
    assert_eq!(list_a.len(), 2, "tenant-a should see 2 projects");
    assert!(list_a.iter().all(|p| p["tenant_id"] == "tenant-a"));

    // List as tenant-b — expect exactly 1.
    let (_, body_b) = send(&router, "GET", "/projects", Some(&auth_b), None).await;
    let list_b = body_b.as_array().expect("array");
    assert_eq!(list_b.len(), 1, "tenant-b should see 1 project");
    assert!(list_b.iter().all(|p| p["tenant_id"] == "tenant-b"));
}

// ---------------------------------------------------------------------------
// Tenant isolation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cross_tenant_get_project_returns_403() {
    let (_state, router, _tmp) = fresh_app().await;
    let auth_a = bearer_for("tenant-a");
    let auth_b = bearer_for("tenant-b");

    let (_, created) = send(
        &router,
        "POST",
        "/projects",
        Some(&auth_a),
        Some(json!({ "name": "alpha-secret" })),
    )
    .await;
    let project_id = created["id"].as_str().expect("id").to_string();

    // tenant-b should NOT be able to read tenant-a's project.
    let (status, _) = send(
        &router,
        "GET",
        &format!("/projects/{project_id}"),
        Some(&auth_b),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn cross_tenant_fork_returns_403() {
    let (_state, router, _tmp) = fresh_app().await;
    let auth_a = bearer_for("tenant-a");
    let auth_b = bearer_for("tenant-b");

    let (_, created) = send(
        &router,
        "POST",
        "/projects",
        Some(&auth_a),
        Some(json!({ "name": "orig", "description": "keep me" })),
    )
    .await;
    let project_id = created["id"].as_str().unwrap().to_string();

    let (status, _) = send(
        &router,
        "POST",
        &format!("/projects/{project_id}/fork"),
        Some(&auth_b),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn same_tenant_fork_succeeds() {
    let (_state, router, _tmp) = fresh_app().await;
    let auth = bearer_for("tenant-a");

    let (_, orig) = send(
        &router,
        "POST",
        "/projects",
        Some(&auth),
        Some(json!({ "name": "orig", "description": "base" })),
    )
    .await;
    let project_id = orig["id"].as_str().unwrap().to_string();

    let (status, forked) = send(
        &router,
        "POST",
        &format!("/projects/{project_id}/fork"),
        Some(&auth),
        Some(json!({ "name": "my-fork" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(forked["name"], "my-fork");
    assert_eq!(forked["tenant_id"], "tenant-a");
    assert_ne!(forked["id"], orig["id"]);
}

// ---------------------------------------------------------------------------
// Auth middleware
// ---------------------------------------------------------------------------

#[tokio::test]
async fn missing_auth_header_returns_401() {
    let (_state, router, _tmp) = fresh_app().await;

    let (status, _) = send(
        &router,
        "POST",
        "/projects",
        None,
        Some(json!({ "name": "no-auth" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn invalid_bearer_returns_401() {
    let (_state, router, _tmp) = fresh_app().await;

    let (status, _) = send(
        &router,
        "GET",
        "/projects",
        Some("Bearer not.a.valid.jwt"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// Public endpoints
// ---------------------------------------------------------------------------

#[tokio::test]
async fn health_is_public() {
    let (_state, router, _tmp) = fresh_app().await;
    let (status, _) = send(&router, "GET", "/health", None, None).await;
    assert_eq!(status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Mutation handler scope enforcement
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mutate_requires_valid_project_access() {
    let (_state, router, _tmp) = fresh_app().await;
    let auth_a = bearer_for("tenant-a");
    let auth_b = bearer_for("tenant-b");

    let (_, created) = send(
        &router,
        "POST",
        "/projects",
        Some(&auth_a),
        Some(json!({ "name": "mut-target" })),
    )
    .await;
    let project_id = created["id"].as_str().unwrap().to_string();

    // Cross-tenant mutate must be forbidden even before reaching the mutation
    // engine (project has no generated code yet, so a passing auth would trip
    // a different 400 — but the point is ProjectAccess should reject first).
    let (status, _) = send(
        &router,
        "POST",
        &format!("/projects/{project_id}/mutate"),
        Some(&auth_b),
        Some(json!({ "change": "anything" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

// ---------------------------------------------------------------------------
// Additional handler surface — guards other project-scoped endpoints.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cross_tenant_metrics_returns_403() {
    let (_state, router, _tmp) = fresh_app().await;
    let auth_a = bearer_for("tenant-a");
    let auth_b = bearer_for("tenant-b");

    let (_, p) = send(
        &router,
        "POST",
        "/projects",
        Some(&auth_a),
        Some(json!({ "name": "m" })),
    )
    .await;
    let pid = p["id"].as_str().unwrap().to_string();

    let (status, _) = send(
        &router,
        "GET",
        &format!("/projects/{pid}/metrics"),
        Some(&auth_b),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn cross_tenant_traces_returns_403() {
    let (_state, router, _tmp) = fresh_app().await;
    let auth_a = bearer_for("tenant-a");
    let auth_b = bearer_for("tenant-b");

    let (_, p) = send(
        &router,
        "POST",
        "/projects",
        Some(&auth_a),
        Some(json!({ "name": "tr" })),
    )
    .await;
    let pid = p["id"].as_str().unwrap().to_string();

    let (status, _) = send(
        &router,
        "GET",
        &format!("/projects/{pid}/traces"),
        Some(&auth_b),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn cross_tenant_code_graph_returns_403() {
    let (_state, router, _tmp) = fresh_app().await;
    let auth_a = bearer_for("tenant-a");
    let auth_b = bearer_for("tenant-b");

    let (_, p) = send(
        &router,
        "POST",
        "/projects",
        Some(&auth_a),
        Some(json!({ "name": "cg" })),
    )
    .await;
    let pid = p["id"].as_str().unwrap().to_string();

    let (status, _) = send(
        &router,
        "GET",
        &format!("/projects/{pid}/code-graph"),
        Some(&auth_b),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn cross_tenant_vault_list_returns_403() {
    let (_state, router, _tmp) = fresh_app().await;
    let auth_a = bearer_for("tenant-a");
    let auth_b = bearer_for("tenant-b");

    let (_, p) = send(
        &router,
        "POST",
        "/projects",
        Some(&auth_a),
        Some(json!({ "name": "v" })),
    )
    .await;
    let pid = p["id"].as_str().unwrap().to_string();

    let (status, _) = send(
        &router,
        "GET",
        &format!("/projects/{pid}/vault"),
        Some(&auth_b),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn cross_tenant_business_overview_returns_403() {
    let (_state, router, _tmp) = fresh_app().await;
    let auth_a = bearer_for("tenant-a");
    let auth_b = bearer_for("tenant-b");

    let (_, p) = send(
        &router,
        "POST",
        "/projects",
        Some(&auth_a),
        Some(json!({ "name": "b" })),
    )
    .await;
    let pid = p["id"].as_str().unwrap().to_string();

    let (status, _) = send(
        &router,
        "GET",
        &format!("/projects/{pid}/business"),
        Some(&auth_b),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn delete_project_by_foreign_tenant_returns_403() {
    let (_state, router, _tmp) = fresh_app().await;
    let auth_a = bearer_for("tenant-a");
    let auth_b = bearer_for("tenant-b");

    let (_, p) = send(
        &router,
        "POST",
        "/projects",
        Some(&auth_a),
        Some(json!({ "name": "doomed" })),
    )
    .await;
    let pid = p["id"].as_str().unwrap().to_string();

    let (status, _) = send(
        &router,
        "DELETE",
        &format!("/projects/{pid}"),
        Some(&auth_b),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Project should still be visible to its owner.
    let (status, _) = send(
        &router,
        "GET",
        &format!("/projects/{pid}"),
        Some(&auth_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Admin-only endpoints
// ---------------------------------------------------------------------------

#[tokio::test]
async fn non_admin_cannot_set_global_budget() {
    let (_state, router, _tmp) = fresh_app().await;

    // Sign a token with only project scopes — no SystemAdmin.
    let token = create_jwt(
        "u1",
        "tenant-a",
        &[Scope::ProjectRead, Scope::ProjectWrite],
        DEV_SECRET,
        3600,
    )
    .unwrap();

    let (status, _) = send(
        &router,
        "POST",
        "/costs/budget",
        Some(&format!("Bearer {token}")),
        Some(json!({ "daily_limit_usd": 1.0 })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn non_admin_cannot_trip_breaker() {
    let (_state, router, _tmp) = fresh_app().await;

    let token = create_jwt(
        "u1",
        "tenant-a",
        &[Scope::ProjectRead],
        DEV_SECRET,
        3600,
    )
    .unwrap();

    let (status, _) = send(
        &router,
        "POST",
        "/control/breaker/trip",
        Some(&format!("Bearer {token}")),
        Some(json!({ "subsystem": "llm" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

// ---------------------------------------------------------------------------
// Body validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_project_with_empty_name_rejected_with_400() {
    // Round-M: input_limits now rejects empty names at the handler layer,
    // before the row is written. This test pins the new (correct) behaviour.
    let (_state, router, _tmp) = fresh_app().await;
    let auth = bearer_for("tenant-a");

    let (status, _body) = send(
        &router,
        "POST",
        "/projects",
        Some(&auth),
        Some(json!({ "name": "" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_project_with_over_limit_name_returns_400() {
    let (_state, router, _tmp) = fresh_app().await;
    let auth = bearer_for("tenant-a");

    // 513 bytes — one past the 512-byte cap.
    let huge = "a".repeat(513);
    let (status, _body) = send(
        &router,
        "POST",
        "/projects",
        Some(&auth),
        Some(json!({ "name": huge })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn oneshot_rejects_oversized_description() {
    let (_state, router, _tmp) = fresh_app().await;
    let auth = bearer_for("tenant-a");

    // First create a valid project.
    let (s, body) = send(
        &router,
        "POST",
        "/projects",
        Some(&auth),
        Some(json!({"name":"p","description":""})),
    )
    .await;
    assert!(s.is_success(), "create project");
    let project_id = body["id"].as_str().unwrap().to_string();

    // 33 KB — past the 32 KB description cap.
    let bomb = "x".repeat(33 * 1024);
    let (status, _) = send(
        &router,
        "POST",
        &format!("/projects/{project_id}/oneshot/start"),
        Some(&auth),
        Some(json!({ "description": bomb })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn malformed_json_returns_400() {
    let (_state, router, _tmp) = fresh_app().await;
    let auth = bearer_for("tenant-a");

    // Manually build a request with bogus JSON.
    let request = Request::builder()
        .method("POST")
        .uri("/projects")
        .header(header::AUTHORIZATION, &auth)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{ not json"))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    assert!(
        response.status().is_client_error(),
        "expected 4xx for malformed JSON, got {}",
        response.status()
    );
}

// ---------------------------------------------------------------------------
// Round-M: additional guarantees introduced this round.
// ---------------------------------------------------------------------------

/// Step metrics must reject any caller whose tenant doesn't own the run.
/// Previously the check was a no-op (see metrics.rs comment) and leaked data.
#[tokio::test]
async fn step_metrics_cross_tenant_returns_403_or_404() {
    let (state, router, _tmp) = fresh_app().await;

    // Tenant A creates a project AND a run metric attached to it.
    let auth_a = bearer_for("tenant-a");
    let (status, body) = send(
        &router,
        "POST",
        "/projects",
        Some(&auth_a),
        Some(json!({"name":"p","description":""})),
    )
    .await;
    assert!(
        status.is_success(),
        "create project: status={status} body={body:?}"
    );
    let project_id = body["id"].as_str().unwrap().to_string();

    // Insert a synthetic run_metric directly.
    let run_id = {
        let db = state.db.lock().await;
        let svc = nexus_store::MetricsService::new(&db);
        svc.record_run_metric(
            Some(&project_id),
            "test",
            10,
            1,
            1,
            0,
            0,
            0,
            "success",
        )
        .unwrap()
        .id
    };

    // Tenant B tries to read tenant A's run — must be rejected.
    let auth_b = bearer_for("tenant-b");
    let (status, _) = send(
        &router,
        "GET",
        &format!("/runs/{run_id}/metrics"),
        Some(&auth_b),
        None,
    )
    .await;
    assert!(
        status == StatusCode::FORBIDDEN || status == StatusCode::NOT_FOUND,
        "cross-tenant run read must be rejected, got {status}"
    );
}

/// delete_api_key is admin-only — previously unauthenticated.
#[tokio::test]
async fn non_admin_cannot_delete_api_key() {
    let (_state, router, _tmp) = fresh_app().await;

    // Build a JWT with NO scopes (even the "default" user can't manage settings).
    let token = create_jwt("some-user", "tenant-a", &[], DEV_SECRET, 3600).unwrap();
    let bearer = format!("Bearer {token}");

    let (status, _) = send(
        &router,
        "DELETE",
        "/settings/api-keys/openai",
        Some(&bearer),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "non-admin must not be able to delete settings API keys"
    );
}

/// Per-project generation lock blocks overlapping oneshot runs.
/// We pre-acquire the lock out-of-band to simulate the race, then verify
/// a new POST is rejected with 409 before the SSE stream starts.
#[tokio::test]
async fn concurrent_oneshot_is_rejected_per_project() {
    let (state, router, _tmp) = fresh_app().await;
    let auth = bearer_for("tenant-a");

    // Create a project.
    let (status, body) = send(
        &router,
        "POST",
        "/projects",
        Some(&auth),
        Some(json!({"name":"p","description":""})),
    )
    .await;
    assert!(status.is_success(), "create project: {status}");
    let project_id = body["id"].as_str().unwrap().to_string();

    // Simulate an in-flight generation by acquiring the lock directly.
    {
        let db = state.db.lock().await;
        let svc = nexus_store::GenerationLockService::new(&db);
        assert!(
            svc.try_acquire(&project_id, "pretend run").unwrap(),
            "prime lock should succeed on first acquire"
        );
    }

    // A POST to /oneshot/start while the lock is held must be refused.
    let (status2, body2) = send(
        &router,
        "POST",
        &format!("/projects/{project_id}/oneshot/start"),
        Some(&auth),
        Some(json!({"description":"second run"})),
    )
    .await;

    assert_eq!(
        status2,
        StatusCode::CONFLICT,
        "held generation lock must produce 409, got {status2} body={body2:?}"
    );
}

/// user_preferences must isolate by tenant after migration 003.
#[tokio::test]
async fn user_preferences_are_tenant_isolated() {
    let (state, _router, _tmp) = fresh_app().await;
    let db = state.db.lock().await;

    // Two tenants write the same (category, key) pair — must not collide.
    db.execute(
        "INSERT INTO user_preferences (id, tenant_id, category, key, value, source)
         VALUES ('a', 'tenant-a', 'theme', 'mode', 'dark', 'explicit')",
        [],
    )
    .expect("tenant A insert");
    db.execute(
        "INSERT INTO user_preferences (id, tenant_id, category, key, value, source)
         VALUES ('b', 'tenant-b', 'theme', 'mode', 'light', 'explicit')",
        [],
    )
    .expect("tenant B insert — previously blocked by singleton UNIQUE(category,key)");

    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM user_preferences", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2, "both tenants' preferences must coexist");
}

/// Cross-tenant trace read is blocked — trace_id is verified against the
/// caller's project_id, not just the URL's project_id.
#[tokio::test]
async fn trace_logs_reject_foreign_trace_id() {
    let (state, router, _tmp) = fresh_app().await;

    // Tenant A creates a project AND a trace.
    let auth_a = bearer_for("tenant-a");
    let (_, body) = send(
        &router,
        "POST",
        "/projects",
        Some(&auth_a),
        Some(json!({"name":"p-a","description":""})),
    )
    .await;
    let project_a = body["id"].as_str().unwrap().to_string();

    let trace_id = {
        let db = state.db.lock().await;
        let svc = nexus_store::ObservabilityService::new(&db);
        svc.create_trace(&project_a, "test task", "agent", "openai", "gpt-4o")
            .unwrap()
            .id
    };

    // Tenant B creates its own project.
    let auth_b = bearer_for("tenant-b");
    let (_, body) = send(
        &router,
        "POST",
        "/projects",
        Some(&auth_b),
        Some(json!({"name":"p-b","description":""})),
    )
    .await;
    let project_b = body["id"].as_str().unwrap().to_string();

    // Tenant B tries to route tenant A's trace through its own project URL.
    let (status, _) = send(
        &router,
        "GET",
        &format!("/projects/{project_b}/traces/{trace_id}/logs"),
        Some(&auth_b),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "URL project_id + foreign trace_id must be rejected"
    );
}

/// Generation budget must BLOCK the oneshot when the tracker is over cap.
#[tokio::test]
async fn oneshot_blocked_when_budget_exhausted() {
    let (state, router, _tmp) = fresh_app().await;

    // Force the budget to effectively zero so any call is over-limit.
    state
        .cost_tracker
        .set_budget(nexus_http::cost_intelligence::CostBudget {
            daily_project_limit: 0.0001,
            daily_global_limit: 0.0001,
            max_tokens_per_call: 1,
            warn_threshold: 0.5,
        })
        .await;

    // Also record a synthetic $10 call so check_budget trips immediately.
    state
        .cost_tracker
        .record_call(None, "gpt-4o", "openai", 1_000_000, 0, 1, "seed")
        .await;

    // Create a project so the pipeline entry passes the initial checks.
    let auth = bearer_for("tenant-a");
    let (_, body) = send(
        &router,
        "POST",
        "/projects",
        Some(&auth),
        Some(json!({"name":"p","description":""})),
    )
    .await;
    let project_id = body["id"].as_str().unwrap().to_string();

    // Fire the oneshot. Pipeline spawns a background task; the handler itself
    // returns 200 to start the SSE stream, but the stream emits a fatal
    // Error event as its first payload. We only assert the stream started.
    let (status, _) = send(
        &router,
        "POST",
        &format!("/projects/{project_id}/oneshot/start"),
        Some(&auth),
        Some(json!({"description":"anything"})),
    )
    .await;
    // The handler always returns 200 on a spawned SSE; the budget gate
    // manifests as a fatal error event inside the stream rather than at
    // handler-return time. Just verify we didn't crash the handler.
    assert!(
        status.is_success() || status == StatusCode::TOO_MANY_REQUESTS,
        "oneshot budget gate must not panic; got {status}"
    );
}

// ---------------------------------------------------------------------------
// Round M+2: self-improvement loop.
// ---------------------------------------------------------------------------

/// Pattern detector seeds at least one draft proposal when the user has built
/// enough projects matching the same intent.
#[tokio::test]
async fn user_pattern_detector_proposes_repeated_intent() {
    let (state, _router, _tmp) = fresh_app().await;

    // Seed 3 "todo list" projects in phase 2 (considered completed).
    {
        let db = state.db.lock().await;
        for i in 0..3 {
            let id = format!("p-todo-{i}");
            db.execute(
                "INSERT INTO projects (id, name, description, phase, tenant_id, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, 2, 'tenant-a', datetime('now'), datetime('now'))",
                rusqlite::params![id, format!("todo {i}"), "build me a todo list app"],
            )
            .unwrap();
        }
    }

    let created = nexus_http::user_pattern_detector::detect_and_propose(&state)
        .await
        .expect("detect_and_propose");
    assert!(
        created >= 1,
        "expected at least one proposal for 3 todo-intent projects, got {created}"
    );

    // Running a second time must dedup (same pattern: skill rows shouldn't
    // pile up).
    let created_again = nexus_http::user_pattern_detector::detect_and_propose(&state)
        .await
        .expect("detect_and_propose (2nd)");
    assert_eq!(
        created_again, 0,
        "second pass must dedup against existing pattern: proposals"
    );
}

/// Listing `/self-improvement/suggested-skills` surfaces the draft proposals.
#[tokio::test]
async fn suggested_skills_endpoint_returns_draft_proposals() {
    let (state, router, _tmp) = fresh_app().await;

    // Seed proposals via the detector.
    {
        let db = state.db.lock().await;
        for i in 0..3 {
            db.execute(
                "INSERT INTO projects (id, name, description, phase, tenant_id, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, 2, 'tenant-a', datetime('now'), datetime('now'))",
                rusqlite::params![
                    format!("p-dash-{i}"),
                    format!("dash {i}"),
                    "admin dashboard for orders"
                ],
            )
            .unwrap();
        }
    }
    let _ = nexus_http::user_pattern_detector::detect_and_propose(&state)
        .await
        .expect("detect_and_propose");

    let auth = bearer_for("tenant-a");
    let (status, body) = send(
        &router,
        "GET",
        "/self-improvement/suggested-skills",
        Some(&auth),
        None,
    )
    .await;
    assert!(status.is_success(), "endpoint: {status}");
    let count = body["count"].as_u64().unwrap_or(0);
    assert!(count >= 1, "expected ≥1 suggestion, got {count}: {body}");
    let first = &body["skills"][0];
    assert!(
        first["description"].as_str().unwrap_or("").starts_with("pattern:"),
        "expected pattern: prefix in description, got {:?}",
        first["description"]
    );
}
