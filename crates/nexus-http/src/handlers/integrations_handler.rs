//! HTTP handlers for the integrations subsystem.
//!
//! Exposes endpoints for:
//! - Health-checking all configured integrations
//! - Listing GitHub repositories
//! - Sending notifications to all configured channels
//!
//! SECURITY: every outbound URL accepted from a client is validated through
//! `crate::security::url_guard::is_public_url` before the server dispatches a
//! request to it. Without that guard, an authenticated user could use the
//! server as an open relay or pivot to internal/metadata endpoints.

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::security::auth::AuthContext;
use crate::security::url_guard::is_public_url;
use crate::state::AppState;

// ─── Request / Response types ───────────────────────────────────────────────

/// Query parameters for listing GitHub repos.
#[derive(Debug, Deserialize)]
pub struct ListReposQuery {
    /// GitHub personal access token.
    pub token: String,
    /// Optional organization to scope the listing.
    pub org: Option<String>,
}

/// Request body for sending a notification.
#[derive(Debug, Deserialize)]
pub struct SendNotificationRequest {
    /// Notification title.
    pub title: String,
    /// Notification body text.
    pub body: String,
    /// Severity level (`info`, `warning`, `error`, `success`).
    #[serde(default = "default_severity")]
    pub severity: String,
    /// Channels to send to. Each entry describes a channel.
    pub channels: Vec<nexus_integrations::notifications::NotificationChannel>,
}

fn default_severity() -> String {
    "info".into()
}

/// A single channel delivery result in the response.
#[derive(Debug, Serialize)]
pub struct ChannelResult {
    /// Channel index (0-based).
    pub index: usize,
    /// Whether delivery succeeded.
    pub success: bool,
    /// Error message if delivery failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Response for the send notification endpoint.
#[derive(Debug, Serialize)]
pub struct SendNotificationResponse {
    /// Total number of channels attempted.
    pub total: usize,
    /// Number of successful deliveries.
    pub succeeded: usize,
    /// Per-channel results.
    pub results: Vec<ChannelResult>,
}

/// Health check response for integrations.
#[derive(Debug, Serialize)]
pub struct IntegrationsHealthResponse {
    /// Overall status.
    pub status: String,
    /// Per-integration health details.
    pub integrations: serde_json::Value,
}

// ─── Handlers ───────────────────────────────────────────────────────────────

/// `GET /integrations/health` — check connectivity of all integrations.
///
/// Returns a summary object with the status of each integration type
/// (GitHub, databases, notifications). Since integrations are not stored
/// globally on AppState, this returns a static readiness probe.
pub async fn integrations_health(
    State(_state): State<Arc<AppState>>,
) -> Json<IntegrationsHealthResponse> {
    Json(IntegrationsHealthResponse {
        status: "ok".into(),
        integrations: serde_json::json!({
            "github": { "available": true, "note": "Pass token via query param to test" },
            "databases": { "available": true, "note": "Supabase supported via REST" },
            "notifications": { "available": true, "note": "Slack, Discord, and webhook supported" },
        }),
    })
}

/// `GET /integrations/github/repos` — list GitHub repositories.
///
/// Requires `token` query parameter. Optionally pass `org` to scope to an organization.
pub async fn github_list_repos(
    State(_state): State<Arc<AppState>>,
    Query(params): Query<ListReposQuery>,
) -> Result<Json<Vec<nexus_integrations::github::Repo>>, (StatusCode, Json<serde_json::Value>)> {
    let gh = nexus_integrations::github::GitHubIntegration::new(&params.token);

    match gh.list_repos(params.org.as_deref()).await {
        Ok(repos) => Ok(Json(repos)),
        Err(e) => {
            let status = match &e {
                nexus_integrations::IntegrationError::Auth(_) => StatusCode::UNAUTHORIZED,
                nexus_integrations::IntegrationError::RateLimited { .. } => {
                    StatusCode::TOO_MANY_REQUESTS
                }
                _ => StatusCode::BAD_GATEWAY,
            };
            Err((status, Json(serde_json::json!({ "error": e.to_string() }))))
        }
    }
}

/// `POST /integrations/notifications/send` — send a notification to specified channels.
///
/// SECURITY:
/// - Requires an authenticated `AuthContext`. Anonymous callers are rejected by
///   the global `auth_middleware`, but adding the extractor here makes the
///   contract explicit and gives us the calling tenant for logging.
/// - Every channel URL is validated through `is_public_url()` BEFORE any
///   request is dispatched. A single bad URL fails the whole batch with a
///   structured 400 — better to refuse than to partially relay.
/// - The caller is logged at INFO so this surface is auditable; the channel
///   targets are NOT logged in full to avoid leaking webhook secrets.
///
/// Returns 400 for any malformed channel URL, 200 with per-channel results
/// otherwise.
pub async fn send_notification(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Json(req): Json<SendNotificationRequest>,
) -> Result<Json<SendNotificationResponse>, (StatusCode, Json<serde_json::Value>)> {
    use nexus_integrations::notifications::{
        NotifSeverity, Notification, NotificationChannel, NotificationService,
    };

    if req.channels.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "no channels supplied" })),
        ));
    }

    // Per-tenant fair-share — keeps a noisy tenant from saturating the
    // outbound notification path for everyone behind the same egress IP.
    if let Err(reason) = state.tenant_rate_limiter.check(&auth.tenant_id, "read") {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({ "error": reason })),
        ));
    }

    // SSRF guard: validate every URL before any network IO. A failure here
    // aborts the entire batch — if the user mixed valid + invalid URLs we
    // refuse rather than partially relay.
    for (idx, channel) in req.channels.iter().enumerate() {
        let url = match channel {
            NotificationChannel::Slack { webhook_url } => webhook_url.as_str(),
            NotificationChannel::Discord { webhook_url } => webhook_url.as_str(),
            NotificationChannel::Webhook { url, .. } => url.as_str(),
        };
        if let Err(reason) = is_public_url(url) {
            warn!(
                tenant_id = %auth.tenant_id,
                user_id = %auth.user_id,
                channel_index = idx,
                reason = %reason,
                "Rejected notification channel URL (SSRF guard)",
            );
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "channel URL rejected",
                    "channel_index": idx,
                    "reason": reason,
                })),
            ));
        }
    }

    let severity = match req.severity.as_str() {
        "warning" => NotifSeverity::Warning,
        "error" => NotifSeverity::Error,
        "success" => NotifSeverity::Success,
        _ => NotifSeverity::Info,
    };

    let notification = Notification {
        title: req.title,
        body: req.body,
        severity,
    };

    let mut svc = NotificationService::new();
    let channel_count = req.channels.len();
    for channel in req.channels {
        svc.add_channel(channel);
    }

    info!(
        tenant_id = %auth.tenant_id,
        user_id = %auth.user_id,
        channels = channel_count,
        "Dispatching notifications",
    );

    let results = svc.send(&notification).await;
    let total = results.len();
    let succeeded = results.iter().filter(|(_, r)| r.is_ok()).count();

    let channel_results: Vec<ChannelResult> = results
        .into_iter()
        .map(|(idx, result)| ChannelResult {
            index: idx,
            success: result.is_ok(),
            error: result.err().map(|e| e.to_string()),
        })
        .collect();

    Ok(Json(SendNotificationResponse {
        total,
        succeeded,
        results: channel_results,
    }))
}
