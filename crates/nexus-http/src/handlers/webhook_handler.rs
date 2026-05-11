//! HTTP handlers for webhook management.
//!
//! Provides CRUD endpoints for registering, listing, removing, and testing
//! webhook configurations.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::error::{ApiError, ApiResult};
use crate::security::url_guard::is_public_url;
use crate::state::AppState;
use crate::webhooks::{WebhookConfig, WebhookEvent};

/// Request body for registering a new webhook.
#[derive(Debug, Deserialize)]
pub struct RegisterWebhookRequest {
    /// URL to POST event payloads to.
    pub url: String,
    /// Events to subscribe to.
    pub events: Vec<WebhookEvent>,
    /// Shared secret for HMAC-SHA256 signing.
    pub secret: String,
}

/// `POST /webhooks` — register a new webhook endpoint.
pub async fn register_webhook(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RegisterWebhookRequest>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    if body.url.is_empty() {
        return Err(ApiError::BadRequest("Webhook URL is required".into()));
    }
    // SSRF guard: reject loopback, RFC1918, link-local, non-http(s) schemes.
    is_public_url(&body.url).map_err(|e| {
        ApiError::BadRequest(format!("Webhook URL rejected by SSRF guard: {e}"))
    })?;

    let id = uuid::Uuid::new_v4().to_string();
    let config = WebhookConfig {
        id: id.clone(),
        url: body.url,
        events: body.events,
        secret: body.secret,
        active: true,
    };

    let mut registry = state.webhook_service.lock().await;
    registry.register(config);

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": id, "status": "registered" })),
    ))
}

/// `GET /webhooks` — list all registered webhooks.
pub async fn list_webhooks(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let registry = state.webhook_service.lock().await;
    let webhooks: Vec<_> = registry
        .list()
        .iter()
        .map(|w| {
            serde_json::json!({
                "id": w.id,
                "url": w.url,
                "events": w.events,
                "active": w.active,
            })
        })
        .collect();

    Json(serde_json::json!({ "webhooks": webhooks }))
}

/// `DELETE /webhooks/:id` — remove a webhook by ID.
pub async fn delete_webhook(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let mut registry = state.webhook_service.lock().await;
    if registry.unregister(&id) {
        Ok(Json(serde_json::json!({ "status": "removed", "id": id })))
    } else {
        Err(ApiError::NotFound(format!("Webhook {id} not found")))
    }
}

/// `POST /webhooks/:id/test` — send a test event to a specific webhook.
pub async fn test_webhook(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let registry = state.webhook_service.lock().await;
    let webhook = registry.list().iter().find(|w| w.id == id).cloned();
    drop(registry);

    match webhook {
        Some(wh) => {
            // Re-validate the stored URL in case policy changed since registration.
            is_public_url(&wh.url).map_err(|e| {
                ApiError::BadRequest(format!("Webhook URL rejected by SSRF guard: {e}"))
            })?;
            let payload = serde_json::json!({
                "test": true,
                "webhook_id": wh.id,
                "message": "This is a test event from Nexus",
            });
            let mut temp = crate::webhooks::WebhookService::new();
            temp.register(WebhookConfig {
                events: vec![WebhookEvent::GenerationCompleted],
                ..wh
            });
            temp.fire(WebhookEvent::GenerationCompleted, payload).await;

            Ok(Json(serde_json::json!({ "status": "test_sent", "id": id })))
        }
        None => Err(ApiError::NotFound(format!("Webhook {id} not found"))),
    }
}
