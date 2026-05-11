//! Observability endpoints — traces, audit log, evolution stats, healing events,
//! cost dashboards, Prometheus metrics, health status, and control plane.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use nexus_store::ObservabilityService;

use crate::{
    error::{ApiError, ApiResult},
    observability::{
        self, BudgetStatus, ControlMode, ControlPlane, CostTracker,
        HealthStatus, ProjectUsage, TenantUsage,
    },
    security::auth::{AuthContext, Scope},
    security::tenant::validate_project_access,
    state::AppState,
};

// ---------------------------------------------------------------------------
// Query / request types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ListParams {
    pub limit: Option<i64>,
}

/// Query parameters for listing tenant traces.
#[derive(Deserialize)]
pub struct ListTracesQuery {
    /// Tenant ID to filter traces by.
    pub tenant_id: Option<String>,
    /// Maximum number of traces to return (default: 50).
    pub limit: Option<usize>,
}

/// Query parameters for cost dashboard.
#[derive(Deserialize)]
pub struct CostDashboardQuery {
    /// Tenant ID to get costs for.
    pub tenant_id: Option<String>,
    /// Project ID to get costs for (optional, for project-level breakdown).
    pub project_id: Option<String>,
    /// Number of days to look back (default: 30).
    pub days: Option<u32>,
}

/// Query parameters for budget check.
#[derive(Deserialize)]
pub struct BudgetCheckQuery {
    /// Tenant ID to check budget for.
    pub tenant_id: String,
    /// Monthly budget cap in USD.
    pub budget_usd: f32,
}

/// Request body for setting control mode.
#[derive(Deserialize)]
pub struct SetModeRequest {
    /// The desired control mode.
    pub mode: ControlMode,
}

/// Request body for tripping or resetting a circuit breaker.
#[derive(Deserialize)]
pub struct BreakerRequest {
    /// Subsystem name (e.g. "llm", "database", "runtime").
    pub subsystem: String,
    /// Reason for tripping (only used for trip requests).
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Project-scoped observability handlers (from nexus-store)
// ---------------------------------------------------------------------------

/// GET /projects/:id/traces — list agent execution traces
pub async fn list_project_traces(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
    Query(params): Query<ListParams>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let svc = ObservabilityService::new(&db);
    // NOTE(audit): cap pagination to prevent unbounded queries.
    let traces = svc.list_traces(&project_id, params.limit.unwrap_or(50).min(1000))?;
    Ok(Json(json!(traces)))
}

/// GET /projects/:id/traces/:trace_id/logs — get tool call logs for a trace.
///
/// SECURITY: we verify (a) the URL's `project_id` belongs to the tenant AND
/// (b) the trace actually belongs to that project. Without (b), a tenant that
/// guesses any trace UUID could cross-read logs by routing them through its
/// own project_id in the URL.
pub async fn get_trace_logs(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, trace_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let svc = ObservabilityService::new(&db);
    let trace_owner = svc
        .get_trace_project_id(&trace_id)?
        .ok_or_else(|| ApiError::NotFound(format!("trace {trace_id} not found")))?;
    if trace_owner != project_id {
        return Err(ApiError::Forbidden(format!(
            "trace {trace_id} does not belong to project {project_id}"
        )));
    }
    let logs = svc.list_tool_logs(&trace_id)?;
    Ok(Json(json!(logs)))
}

/// GET /projects/:id/audit — get audit log
pub async fn list_audit(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
    Query(params): Query<ListParams>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let svc = ObservabilityService::new(&db);
    let entries = svc.list_audit(&project_id, params.limit.unwrap_or(100).min(1000))?;
    Ok(Json(json!(entries)))
}

/// GET /projects/:id/evolution/:agent — get agent evolution stats
pub async fn get_evolution(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, agent_name)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let svc = ObservabilityService::new(&db);
    let stats = svc.get_agent_stats(&project_id, &agent_name)?;
    Ok(Json(stats))
}

/// GET /projects/:id/healing — list healing events
pub async fn list_healing(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
    Query(params): Query<ListParams>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let svc = ObservabilityService::new(&db);
    let events = svc.list_healing_events(&project_id, params.limit.unwrap_or(20).min(1000))?;
    Ok(Json(json!(events)))
}

// ---------------------------------------------------------------------------
// Tenant-scoped trace handlers
// ---------------------------------------------------------------------------

/// `GET /observability/traces` — List generation traces for a tenant.
///
/// Returns an empty list since traces are held in-memory by `TraceCollector`
/// which would be wired into `AppState` in a full deployment.
pub async fn list_tenant_traces(
    State(_app): State<Arc<AppState>>,
    Query(params): Query<ListTracesQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let tenant_id = params.tenant_id.as_deref().unwrap_or("default");
    let limit = params.limit.unwrap_or(50).min(1000);

    // In a production deployment, the TraceCollector would live on AppState.
    // For now, return an empty list with the correct envelope.
    Ok(Json(json!({
        "tenant_id": tenant_id,
        "limit": limit,
        "traces": [],
    })))
}

/// `GET /observability/traces/:trace_id` — Get a single trace by ID.
pub async fn get_trace(
    State(_app): State<Arc<AppState>>,
    Path(trace_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    // Would look up from TraceCollector on AppState.
    Err(ApiError::NotFound(format!("Trace '{trace_id}' not found")))
}

// ---------------------------------------------------------------------------
// Cost handlers
// ---------------------------------------------------------------------------

/// `GET /observability/costs` — Cost dashboard with tenant or project breakdown.
pub async fn cost_dashboard(
    State(app): State<Arc<AppState>>,
    Query(params): Query<CostDashboardQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;

    if let Some(ref project_id) = params.project_id {
        let usage: ProjectUsage = CostTracker::project_usage(&db, project_id);
        return Ok(Json(json!({
            "scope": "project",
            "project_id": project_id,
            "usage": usage,
        })));
    }

    let tenant_id = params.tenant_id.as_deref().unwrap_or("default");
    let days = params.days.unwrap_or(30);
    let usage: TenantUsage = CostTracker::tenant_usage(&db, tenant_id, days);

    Ok(Json(json!({
        "scope": "tenant",
        "tenant_id": tenant_id,
        "days": days,
        "usage": usage,
    })))
}

/// `GET /observability/costs/budget` — Check spend against a budget cap.
pub async fn budget_check(
    State(app): State<Arc<AppState>>,
    Query(params): Query<BudgetCheckQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    let status: BudgetStatus =
        CostTracker::budget_check(&db, &params.tenant_id, params.budget_usd);

    Ok(Json(json!({
        "tenant_id": params.tenant_id,
        "budget_usd": params.budget_usd,
        "status": status,
    })))
}

// ---------------------------------------------------------------------------
// Metrics handler
// ---------------------------------------------------------------------------

/// `GET /metrics` — Prometheus-compatible metrics endpoint.
///
/// Exposes aggregate system metrics (request counts, latency histograms,
/// per-provider LLM stats) which are low-sensitivity but still useful
/// reconnaissance for an attacker sizing the deployment. Restricted to
/// `SystemAdmin` so ordinary tenant tokens cannot scrape it; monitoring
/// scrapers should be issued an admin-scoped service token.
pub async fn prometheus_metrics(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
) -> Result<(StatusCode, [(axum::http::header::HeaderName, &'static str); 1], String), axum::response::Response> {
    auth.require_scope(&crate::security::auth::Scope::SystemAdmin)?;
    let body = observability::prometheus_metrics(&app).await;
    Ok((
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    ))
}

// ---------------------------------------------------------------------------
// Health handler
// ---------------------------------------------------------------------------

/// `GET /health/detailed` — Unified health dashboard.
pub async fn health_detailed(
    State(app): State<Arc<AppState>>,
) -> ApiResult<Json<HealthStatus>> {
    let status = observability::health_dashboard(&app).await;
    Ok(Json(status))
}

// ---------------------------------------------------------------------------
// Control Plane handlers
// ---------------------------------------------------------------------------

// A static control plane instance for demonstration. In production, this would
// live on AppState.
fn global_control_plane() -> &'static ControlPlane {
    use std::sync::OnceLock;
    static CP: OnceLock<ControlPlane> = OnceLock::new();
    CP.get_or_init(ControlPlane::new)
}

/// `POST /control/mode` — Set the control plane operating mode (admin only).
pub async fn set_control_mode(
    State(_app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(req): Json<SetModeRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::SystemAdmin)
        .map_err(|_| ApiError::Forbidden("system:admin scope required".into()))?;
    let cp = global_control_plane();
    cp.set_mode(req.mode.clone());
    Ok(Json(json!({
        "mode": cp.mode(),
        "applied": true,
    })))
}

/// `GET /control/status` — Get current control plane status.
pub async fn control_status(
    State(_app): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let cp = global_control_plane();
    Json(json!({
        "mode": cp.mode(),
        "circuit_breakers": cp.breakers(),
    }))
}

/// `POST /control/breaker/trip` — Trip a circuit breaker (admin only).
pub async fn trip_breaker(
    State(_app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(req): Json<BreakerRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::SystemAdmin)
        .map_err(|_| ApiError::Forbidden("system:admin scope required".into()))?;
    let cp = global_control_plane();
    let reason = req.reason.as_deref().unwrap_or("Manual trip");
    cp.trip_breaker(&req.subsystem, reason);
    Ok(Json(json!({
        "subsystem": req.subsystem,
        "tripped": true,
        "breakers": cp.breakers(),
    })))
}

/// `POST /control/breaker/reset` — Reset a circuit breaker (admin only).
pub async fn reset_breaker(
    State(_app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(req): Json<BreakerRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::SystemAdmin)
        .map_err(|_| ApiError::Forbidden("system:admin scope required".into()))?;
    let cp = global_control_plane();
    cp.reset_breaker(&req.subsystem);
    Ok(Json(json!({
        "subsystem": req.subsystem,
        "tripped": false,
        "breakers": cp.breakers(),
    })))
}
