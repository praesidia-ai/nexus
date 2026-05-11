//! `ProjectAccess` — Axum extractor that combines auth + project ownership in one step.
//!
//! Handlers that operate on a specific project should declare `ProjectAccess` as a
//! parameter instead of the raw `Path<String>` + manual `validate_project_access` call.
//! This makes the ownership check mandatory and impossible to forget.
//!
//! # Usage
//!
//! ```rust,ignore
//! pub async fn my_handler(
//!     State(app): State<Arc<AppState>>,
//!     access: ProjectAccess,
//! ) -> ApiResult<Json<serde_json::Value>> {
//!     access.require_scope(&Scope::ProjectWrite)?;
//!     // access.project_id and access.tenant_id are now safe to use
//! }
//! ```

use std::sync::Arc;

use std::collections::HashMap;

use axum::{
    extract::Path,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};

use crate::{
    security::{
        auth::{AuthContext, Scope},
        tenant::validate_project_access,
    },
    state::AppState,
};

/// Verified project access context injected into handlers via Axum extraction.
///
/// Guarantees that:
/// 1. The request is authenticated (valid JWT or API key).
/// 2. The authenticated tenant owns the requested project.
#[derive(Debug, Clone)]
pub struct ProjectAccess {
    /// The project ID from the URL path parameter `:id`.
    pub project_id: String,
    /// Tenant ID from the authenticated context.
    pub tenant_id: String,
    /// User ID from the authenticated context.
    pub user_id: String,
    /// Permission scopes for this session.
    pub scopes: Vec<Scope>,
}

impl ProjectAccess {
    /// Check whether this context has a specific scope.
    pub fn has_scope(&self, scope: &Scope) -> bool {
        self.scopes.contains(&Scope::SystemAdmin) || self.scopes.contains(scope)
    }

    /// Require a specific scope, returning a 403 response if missing.
    #[allow(clippy::result_large_err)]
    pub fn require_scope(&self, scope: &Scope) -> Result<(), Response> {
        if self.has_scope(scope) {
            Ok(())
        } else {
            Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({"error": format!("Missing required scope: {scope}")})),
            )
                .into_response())
        }
    }
}

#[axum::async_trait]
impl axum::extract::FromRequestParts<Arc<AppState>> for ProjectAccess {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let auth = AuthContext::from_request_parts(parts, state).await?;

        let Path(params): Path<HashMap<String, String>> =
            Path::<HashMap<String, String>>::from_request_parts(parts, state)
                .await
                .map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({"error": "missing project id in path"})),
                    )
                        .into_response()
                })?;
        let project_id = params.get("id").cloned().ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "missing project id in path"})),
            )
                .into_response()
        })?;

        {
            // Validate ownership against a reader connection — every project
            // request hits this path, so funneling them through the writer
            // mutex coupled tenant validation latency to the slowest in-flight
            // write. The pool runs N readers in parallel.
            let db = state.acquire_reader().await;
            validate_project_access(&db, &project_id, &auth.tenant_id).map_err(|e| {
                (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({"error": e})),
                )
                    .into_response()
            })?;
        }

        Ok(ProjectAccess {
            project_id,
            tenant_id: auth.tenant_id,
            user_id: auth.user_id,
            scopes: auth.scopes,
        })
    }
}
