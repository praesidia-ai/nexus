//! HTTP handlers for the governance framework.
//!
//! Exposes:
//!   GET    /governance/policies        — list all policies
//!   POST   /governance/policies        — create/update a policy
//!   DELETE /governance/policies/:id    — remove a policy
//!   POST   /governance/evaluate        — evaluate a proposed action
//!   POST   /governance/kill-switch     — activate/deactivate global halt
//!   GET    /governance/compliance      — grade an action for compliance
//!   POST   /governance/pii/redact      — redact PII from text

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::{
    error::ApiError,
    governance::{grade_action, redact_pii, AgentPolicy, PolicyDecision},
    security::auth::{AuthContext, Scope},
    state::AppState,
};

/// `GET /governance/policies` — list all policies.
pub async fn list_policies(
    State(app): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let policies = app.policy_engine.list_policies().await;
    Json(serde_json::json!({
        "policies": policies,
        "halted": app.policy_engine.is_halted(),
    }))
}

/// `POST /governance/policies` — create or update a policy.
///
/// Requires `system:admin` — policies span every tenant on this instance.
pub async fn upsert_policy(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(policy): Json<AgentPolicy>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    auth.require_scope(&Scope::SystemAdmin)
        .map_err(|_| ApiError::Forbidden("Missing scope: system:admin".into()))?;
    let id = policy.id.clone();
    let policy_for_audit = serde_json::to_value(&policy).ok();
    app.policy_engine.upsert_policy(policy).await;
    // CLAUDE.md invariant #6: governed actions go through the signed audit log.
    app.audit_log
        .append(
            "system",
            "policy_upserted",
            serde_json::json!({
                "policy_id": id,
                "actor": auth.user_id.clone(),
                "policy": policy_for_audit,
            }),
            &app.audit_keypair,
        )
        .await;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "id": id, "status": "upserted" })),
    ))
}

/// `DELETE /governance/policies/:id` — remove a policy.
///
/// Requires `system:admin`.
pub async fn delete_policy(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    auth.require_scope(&Scope::SystemAdmin)
        .map_err(|_| ApiError::Forbidden("Missing scope: system:admin".into()))?;
    if app.policy_engine.remove_policy(&id).await {
        // CLAUDE.md invariant #6: governed actions go through the signed audit log.
        app.audit_log
            .append(
                "system",
                "policy_removed",
                serde_json::json!({
                    "policy_id": id,
                    "actor": auth.user_id.clone(),
                }),
                &app.audit_keypair,
            )
            .await;
        Ok((StatusCode::OK, Json(serde_json::json!({ "id": id, "status": "removed" }))))
    } else {
        Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("Policy '{id}' not found") })),
        ))
    }
}

/// Request for evaluating a proposed action.
#[derive(Deserialize)]
pub struct EvaluateRequest {
    pub agent_id: String,
    pub tool_name: Option<String>,
    pub estimated_cost_usd: Option<f64>,
    pub action_tags: Option<Vec<String>>,
}

/// `POST /governance/evaluate` — evaluate a proposed action against policies.
pub async fn evaluate_action(
    State(app): State<Arc<AppState>>,
    Json(req): Json<EvaluateRequest>,
) -> Json<serde_json::Value> {
    let tags: Vec<&str> = req
        .action_tags
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|s| s.as_str())
        .collect();

    let decision = app
        .policy_engine
        .evaluate(
            &req.agent_id,
            req.tool_name.as_deref(),
            req.estimated_cost_usd,
            &tags,
        )
        .await;

    let (status, reason) = match &decision {
        PolicyDecision::Allow => ("allow", None),
        PolicyDecision::Deny { reason } => ("deny", Some(reason.as_str())),
        PolicyDecision::RequireApproval { reason } => ("require_approval", Some(reason.as_str())),
    };

    Json(serde_json::json!({
        "decision": status,
        "reason": reason,
    }))
}

/// `POST /governance/kill-switch` — activate or deactivate the global halt.
#[derive(Deserialize)]
pub struct KillSwitchRequest {
    pub active: bool,
    pub reason: Option<String>,
}

/// `POST /governance/kill-switch` — toggle the global halt.
///
/// Requires `system:admin`. The kill-switch halts every agent across every
/// tenant on this instance, so any non-admin caller would be a platform-wide
/// DoS vector.
pub async fn kill_switch(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(req): Json<KillSwitchRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    auth.require_scope(&Scope::SystemAdmin)
        .map_err(|_| ApiError::Forbidden("Missing scope: system:admin".into()))?;
    if req.active {
        app.policy_engine.activate_kill_switch();
        // Record in audit log
        let payload = serde_json::json!({
            "reason": req.reason.unwrap_or_else(|| "No reason provided".into()),
            "action": "kill_switch_activated",
            "actor": auth.user_id.clone(),
        });
        app.audit_log
            .append("system", "kill_switch_activated", payload, &app.audit_keypair)
            .await;
        Ok(Json(serde_json::json!({ "halted": true })))
    } else {
        app.policy_engine.deactivate_kill_switch();
        app.audit_log
            .append(
                "system",
                "kill_switch_deactivated",
                serde_json::json!({ "actor": auth.user_id.clone() }),
                &app.audit_keypair,
            )
            .await;
        Ok(Json(serde_json::json!({ "halted": false })))
    }
}

/// `POST /governance/compliance` — grade an action for compliance.
#[derive(Deserialize)]
pub struct ComplianceRequest {
    pub action: String,
    pub tool_name: Option<String>,
    pub data_contains_pii: Option<bool>,
    pub has_human_approval: Option<bool>,
}

pub async fn check_compliance(
    Json(req): Json<ComplianceRequest>,
) -> Json<serde_json::Value> {
    let report = grade_action(
        &req.action,
        req.tool_name.as_deref(),
        req.data_contains_pii.unwrap_or(false),
        req.has_human_approval.unwrap_or(false),
    );
    Json(serde_json::to_value(report).unwrap_or_default())
}

/// `POST /governance/pii/redact` — redact PII from text.
#[derive(Deserialize)]
pub struct RedactRequest {
    pub text: String,
}

pub async fn redact_pii_endpoint(
    Json(req): Json<RedactRequest>,
) -> Json<serde_json::Value> {
    let (redacted, detected_types) = redact_pii(&req.text);
    Json(serde_json::json!({
        "redacted": redacted,
        "detected_pii_types": detected_types,
        "pii_found": !detected_types.is_empty(),
    }))
}
