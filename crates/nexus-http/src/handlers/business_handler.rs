//! Business overview and CEO dashboard endpoints.
//!
//! These endpoints power the "one app for everything" experience,
//! giving the user a CEO-level view of their app and agent teams.

use std::sync::Arc;

use axum::{
    extract::State,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use nexus_store::{AppRunnerService, BusinessService};

use crate::{
    business_teams::{self, template_to_team},
    error::{ApiError, ApiResult},
    security::auth::Scope,
    security::project_access::ProjectAccess,
    state::AppState,
};

// ---------------------------------------------------------------------------
// GET /projects/:id/business — CEO-level business overview
// ---------------------------------------------------------------------------

pub async fn get_business_overview(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<Value>> {
    let project_id = access.project_id.clone();
    // --- App status ---
    let app_status = {
        let db = app.db.lock().await;
        let svc = AppRunnerService::new(&db);
        match svc.get_running_instance(&project_id).ok().flatten() {
            Some(inst) => json!({
                "status": inst.status,
                "port": inst.port,
                "url": format!("http://localhost:{}", inst.port),
                "uptime_estimate": "running since last start",
            }),
            None => json!({
                "status": "stopped",
                "port": null,
                "url": null,
                "uptime_estimate": null,
            }),
        }
    };

    // --- Active teams from DB ---
    let active_teams = {
        let db = app.db.lock().await;
        let biz = BusinessService::new(&db);
        biz.list_teams(&project_id)
            .unwrap_or_default()
            .into_iter()
            .map(|t| json!({
                "id": t.id,
                "name": t.display_name,
                "template": t.template_name,
                "autonomy_level": t.autonomy_level,
                "control_mode": t.control_mode,
                "status": t.status,
                "created_at": t.created_at,
            }))
            .collect::<Vec<Value>>()
    };

    // --- Recent business events ---
    let recent_events = {
        let db = app.db.lock().await;
        let biz = BusinessService::new(&db);
        biz.list_events(&project_id, 10)
            .unwrap_or_default()
            .into_iter()
            .map(|e| json!({
                "id": e.id,
                "event_type": e.event_type,
                "severity": e.severity,
                "created_at": e.created_at,
            }))
            .collect::<Vec<Value>>()
    };

    // --- Items needing attention (from high-severity events) ---
    let attention_items = {
        let db = app.db.lock().await;
        let biz = BusinessService::new(&db);
        biz.list_events(&project_id, 50)
            .unwrap_or_default()
            .into_iter()
            .filter(|e| e.severity == "warning" || e.severity == "critical")
            .take(5)
            .map(|e| json!({
                "type": e.severity,
                "event_type": e.event_type,
                "message": e.payload_json,
                "created_at": e.created_at,
            }))
            .collect::<Vec<Value>>()
    };

    // --- Cost summary from live tracker ---
    let cost_summary = {
        let summary = app.cost_tracker.daily_summary().await;
        let project_cost = summary.by_project.get(&project_id).copied().unwrap_or(0.0);
        json!({
            "today_total_usd": summary.total_cost_usd,
            "today_project_usd": project_cost,
            "total_calls": summary.total_calls,
            "total_tokens": summary.total_tokens,
            "budget_remaining_usd": summary.budget_remaining,
            "budget_used_pct": summary.budget_used_pct,
        })
    };

    // --- Kernel process stats ---
    let process_stats = {
        let running = app.scheduler.running_count().await;
        let total = app.scheduler.process_count().await;
        json!({ "running_processes": running, "total_processes": total })
    };

    Ok(Json(json!({
        "project_id": project_id,
        "app": app_status,
        "active_teams": active_teams,
        "recent_events": recent_events,
        "attention": attention_items,
        "costs": cost_summary,
        "processes": process_stats,
    })))
}

// ---------------------------------------------------------------------------
// GET /projects/:id/business/teams — list business team templates + instances
// ---------------------------------------------------------------------------

pub async fn list_business_teams(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<Value>> {
    let project_id = access.project_id.clone();
    // Available templates
    let templates = business_teams::all_team_templates();
    let templates_json: Vec<Value> = templates
        .iter()
        .map(|t| serde_json::to_value(t).unwrap_or_default())
        .collect();

    // Active team instances for this project
    let instances = {
        let db = app.db.lock().await;
        let biz = BusinessService::new(&db);
        biz.list_teams(&project_id)
            .unwrap_or_default()
            .into_iter()
            .map(|t| json!({
                "id": t.id,
                "template_name": t.template_name,
                "display_name": t.display_name,
                "autonomy_level": t.autonomy_level,
                "control_mode": t.control_mode,
                "status": t.status,
                "created_at": t.created_at,
            }))
            .collect::<Vec<Value>>()
    };

    Ok(Json(json!({
        "templates": templates_json,
        "instances": instances,
    })))
}

// ---------------------------------------------------------------------------
// POST /projects/:id/business/teams — create/activate a business team
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateBusinessTeamRequest {
    pub template_name: String,
    /// One of: "full_approval", "supervised", "autonomous", "progressive".
    pub autonomy_level: Option<String>,
    /// Override control mode: "safe", "assisted", "autonomous".
    pub control_mode: Option<String>,
}

pub async fn create_business_team(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(body): Json<CreateBusinessTeamRequest>,
) -> ApiResult<Json<Value>> {
    access
        .require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;
    let project_id = access.project_id.clone();
    // Find the template
    let templates = business_teams::all_team_templates();
    let template = templates
        .iter()
        .find(|t| t.name == body.template_name)
        .ok_or_else(|| {
            ApiError::NotFound(format!("Team template '{}' not found", body.template_name))
        })?;

    let autonomy = body.autonomy_level.as_deref().unwrap_or("supervised");
    let valid = ["full_approval", "supervised", "autonomous", "progressive"];
    if !valid.contains(&autonomy) {
        return Err(ApiError::BadRequest(format!(
            "Invalid autonomy_level '{}'. Must be one of: {}",
            autonomy,
            valid.join(", "),
        )));
    }

    let control_mode = body.control_mode.as_deref().unwrap_or("assisted");

    // Persist the team instance
    let team_instance = {
        let db = app.db.lock().await;
        let biz = BusinessService::new(&db);
        biz.create_team(
            &project_id,
            &body.template_name,
            &template.description,
            autonomy,
            control_mode,
            Some(&serde_json::to_string(template).unwrap_or_default()),
        )
        .map_err(|e| ApiError::Internal(e.to_string()))?
    };

    // Convert template to a runnable TeamDefinition
    let team_def = template_to_team(template, &team_instance.id, Some(control_mode));

    // Record business event
    {
        let db = app.db.lock().await;
        let biz = BusinessService::new(&db);
        biz.record_event(
            &project_id,
            Some(&team_instance.id),
            "team_created",
            &json!({
                "template": body.template_name,
                "autonomy_level": autonomy,
                "member_count": team_def.members.len(),
            })
            .to_string(),
            "info",
        )
        .ok();
    }

    Ok(Json(json!({
        "id": team_instance.id,
        "project_id": project_id,
        "template": body.template_name,
        "display_name": team_instance.display_name,
        "autonomy_level": autonomy,
        "control_mode": control_mode,
        "status": "active",
        "members": team_def.members.len(),
        "coordination": serde_json::to_value(&team_def.coordination).unwrap_or_default(),
        "created_at": team_instance.created_at,
    })))
}

// ---------------------------------------------------------------------------
// GET /projects/:id/business/events — recent business events
// ---------------------------------------------------------------------------

pub async fn list_business_events(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<Value>> {
    let project_id = access.project_id.clone();
    let events = {
        let db = app.db.lock().await;
        let biz = BusinessService::new(&db);
        biz.list_events(&project_id, 50)
            .unwrap_or_default()
            .into_iter()
            .map(|e| {
                let payload: Value = serde_json::from_str(&e.payload_json)
                    .unwrap_or(Value::String(e.payload_json.clone()));
                json!({
                    "id": e.id,
                    "team_id": e.team_id,
                    "event_type": e.event_type,
                    "payload": payload,
                    "severity": e.severity,
                    "created_at": e.created_at,
                })
            })
            .collect::<Vec<Value>>()
    };

    Ok(Json(json!({
        "project_id": project_id,
        "events": events,
        "count": events.len(),
    })))
}
