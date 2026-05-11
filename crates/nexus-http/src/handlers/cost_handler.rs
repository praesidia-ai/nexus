//! Handlers for the Cost Intelligence Layer.

use std::sync::Arc;

use axum::{
    extract::State,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    cost_intelligence::CostBudget,
    error::{ApiError, ApiResult},
    security::auth::{AuthContext, Scope},
    state::AppState,
};

// Note: In production, the CostTracker would be part of AppState.
// For now, we provide read-only endpoints that work with the audit_entries table.

/// GET /costs/summary — get cost summary for today
pub async fn daily_summary(
    State(app): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    // Read from audit_entries where action = 'llm_call'
    let mut stmt = db
        .prepare(
            "SELECT detail FROM audit_entries WHERE action = 'llm_call' AND created_at LIKE ?1 ORDER BY created_at DESC LIMIT 100",
        )
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let records: Vec<serde_json::Value> = stmt
        .query_map(rusqlite::params![format!("{}%", today)], |row| {
            let detail: String = row.get(0)?;
            Ok(detail)
        })
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .filter_map(|r| r.ok())
        .filter_map(|s| serde_json::from_str(&s).ok())
        .collect();

    let total_cost: f64 = records
        .iter()
        .filter_map(|r| r.get("cost_usd").and_then(|v| v.as_f64()))
        .sum();

    let total_tokens: u64 = records
        .iter()
        .filter_map(|r| r.get("total_tokens").and_then(|v| v.as_u64()))
        .sum();

    Ok(Json(json!({
        "date": today,
        "total_calls": records.len(),
        "total_cost_usd": total_cost,
        "total_tokens": total_tokens,
        "records": records,
    })))
}

/// GET /costs/budget — get current budget status
pub async fn budget_status() -> ApiResult<Json<serde_json::Value>> {
    let budget = CostBudget::default();
    Ok(Json(json!({
        "daily_project_limit": budget.daily_project_limit,
        "daily_global_limit": budget.daily_global_limit,
        "max_tokens_per_call": budget.max_tokens_per_call,
    })))
}

/// POST /costs/budget — update budget limits
#[derive(Deserialize)]
pub struct SetBudgetReq {
    pub daily_limit_usd: Option<f64>,
    pub daily_global_limit: Option<f64>,
    pub max_tokens_per_call: Option<u64>,
}

pub async fn set_budget(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<SetBudgetReq>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::SystemAdmin)
        .map_err(|_| ApiError::Forbidden("system:admin scope required".into()))?;

    // Start from the current live budget so partial updates preserve fields
    // we aren't setting. Previously this built a fresh `CostBudget::default()`
    // and discarded the result — POST /costs/budget was a silent no-op.
    let mut budget = app.cost_tracker.budget().await;
    if let Some(v) = body.daily_limit_usd {
        budget.daily_project_limit = v;
    }
    if let Some(v) = body.daily_global_limit {
        budget.daily_global_limit = v;
    }
    if let Some(v) = body.max_tokens_per_call {
        budget.max_tokens_per_call = v;
    }
    app.cost_tracker.set_budget(budget.clone()).await;

    // Persist the new budget in the settings table so it survives restart.
    // Stored as individual rows so migrations and reads stay simple.
    {
        let db = app.db.lock().await;
        let svc = nexus_store::ProjectService::new(&db);
        let _ = svc.set_setting("budget.daily_project_limit", &budget.daily_project_limit.to_string());
        let _ = svc.set_setting("budget.daily_global_limit", &budget.daily_global_limit.to_string());
        let _ = svc.set_setting("budget.max_tokens_per_call", &budget.max_tokens_per_call.to_string());
    }

    tracing::info!(
        actor = %auth.user_id,
        project_limit = budget.daily_project_limit,
        global_limit = budget.daily_global_limit,
        max_tokens = budget.max_tokens_per_call,
        "Cost budget updated"
    );

    Ok(Json(json!({
        "daily_project_limit": budget.daily_project_limit,
        "daily_global_limit": budget.daily_global_limit,
        "max_tokens_per_call": budget.max_tokens_per_call,
        "updated": true,
    })))
}

/// GET /costs/recommendations — get cost optimization recommendations
pub async fn recommendations(
    State(app): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    // Use the live tracker from AppState so recommendations reflect real usage
    let recs = app.cost_tracker.recommendations().await;
    Ok(Json(json!({ "recommendations": recs })))
}

/// GET /costs/model-route — suggest optimal model for a task
#[derive(Deserialize)]
pub struct ModelRouteQuery {
    pub purpose: String,
    pub complexity: Option<String>,
}

pub async fn model_route(
    axum::extract::Query(query): axum::extract::Query<ModelRouteQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let complexity = query.complexity.as_deref().unwrap_or("medium");
    let model = crate::cost_intelligence::route_model(&query.purpose, complexity);
    Ok(Json(json!({
        "recommended_model": model,
        "purpose": query.purpose,
        "complexity": complexity,
    })))
}
