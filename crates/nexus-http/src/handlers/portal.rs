//! Portal handlers.
//!
//! - POST /projects/:id/portal/publish — scaffold + register portal
//! - GET  /portal/:slug — serve portal index.html
//! - GET  /portal/:slug/* — serve portal static assets

use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, Response, StatusCode},
    Json,
};
use serde::Deserialize;

use nexus_store::PortalBuilder;

use crate::{
    error::{ApiError, ApiResult},
    security::auth::AuthContext,
    security::tenant::validate_project_access,
    state::AppState,
};

// ---------------------------------------------------------------------------
// GET /projects/:id/portal
// ---------------------------------------------------------------------------

/// Portal config shape returned to the UI. Matches the `PortalConfig`
/// interface in `web/src/app/[projectId]/portal/page.tsx`.
fn portal_view(portal: Option<&nexus_store::Portal>) -> serde_json::Value {
    match portal {
        Some(p) => serde_json::json!({
            "slug": p.slug,
            "name": p.project_name,
            "published": p.published_at.is_some(),
            "published_url": p.published_at.as_ref().map(|_| format!("/portal/{}", p.slug)),
            "updated_at": p.published_at.clone().unwrap_or_else(|| p.created_at.clone()),
        }),
        None => serde_json::json!({
            "slug": "",
            "name": "",
            "published": false,
            "published_url": null,
            "updated_at": null,
        }),
    }
}

pub async fn get_portal(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let ps = nexus_store::PortalService::new(&db);
    // A project without a portal is a normal pre-publish state — return an
    // empty config (UI renders it as "Draft") instead of 404.
    let portal = ps.get_portal_for_project(&project_id)?;
    Ok(Json(portal_view(portal.as_ref())))
}

// ---------------------------------------------------------------------------
// POST /projects/:id/portal/publish
// ---------------------------------------------------------------------------

/// Request body for publish. All fields optional — if omitted, names are
/// derived from the project record and the slug from the project name. This
/// lets the UI invoke publish with no body.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct PublishPortalReq {
    pub project_name: Option<String>,
    pub agent_name: Option<String>,
    pub slug: Option<String>,
}

fn slugify(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

pub async fn publish_portal(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
    body: Option<Json<PublishPortalReq>>,
) -> ApiResult<Json<serde_json::Value>> {
    let req = body.map(|Json(b)| b).unwrap_or_default();

    // Resolve project_name from the request or the project record.
    let (project_name, agent_name) = {
        let db = app.db.lock().await;
        validate_project_access(&db, &project_id, &auth.tenant_id)
            .map_err(ApiError::Forbidden)?;

        let project_name = match req.project_name.clone() {
            Some(n) if !n.trim().is_empty() => n,
            _ => {
                let ps = nexus_store::ProjectService::new(&db);
                ps.get_project(&project_id)
                    .map_err(|e| ApiError::Internal(e.to_string()))?
                    .map(|p| p.name)
                    .unwrap_or_else(|| project_id.clone())
            }
        };
        let agent_name = req.agent_name.clone().unwrap_or_else(|| "Agent".to_string());
        (project_name, agent_name)
    };

    let portal_dir = app.project_portal_dir(&project_id);
    PortalBuilder::scaffold(&portal_dir, &project_name, &agent_name)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut slug = req.slug.unwrap_or_else(|| slugify(&project_name));
    if slug.is_empty() {
        slug = format!("project-{}", &project_id.chars().take(8).collect::<String>());
    }

    let portal = {
        let db = app.db.lock().await;
        let ps = nexus_store::PortalService::new(&db);
        let portal = ps
            .upsert_portal(&project_id, &slug, &project_name, &agent_name)
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        ps.publish_portal(&portal.id)
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        ps.get_portal(&portal.id)
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .ok_or_else(|| ApiError::Internal("Portal disappeared after publish".into()))?
    };

    Ok(Json(portal_view(Some(&portal))))
}

// ---------------------------------------------------------------------------
// GET /portal/:slug  — serve index.html
// ---------------------------------------------------------------------------

pub async fn serve_portal_index(
    State(app): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> ApiResult<Response<Body>> {
    let portal_dir = find_portal_dir(&app, &slug).await?;
    let index_path = portal_dir.join("index.html");

    if !index_path.exists() {
        return Err(ApiError::NotFound(format!("Portal '{}' not found", slug)));
    }

    let html = std::fs::read_to_string(&index_path)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Portal HTML comes out of codegen and may include user-influenced copy.
    // A strict CSP + anti-clickjacking header gives defense-in-depth against
    // a prompt-injected <script> making it into the rendered page. Inline
    // scripts and styles are allowed because the generator emits them; no
    // remote origins are permitted except self.
    let csp = "default-src 'self'; \
               script-src 'self' 'unsafe-inline'; \
               style-src 'self' 'unsafe-inline'; \
               img-src 'self' data:; \
               font-src 'self' data:; \
               connect-src 'self'; \
               frame-ancestors 'self'; \
               base-uri 'self'; \
               form-action 'self'";

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CONTENT_SECURITY_POLICY, csp)
        .header("X-Content-Type-Options", "nosniff")
        .header("X-Frame-Options", "SAMEORIGIN")
        .header("Referrer-Policy", "strict-origin-when-cross-origin")
        .body(Body::from(html))
        .map_err(|e| ApiError::Internal(e.to_string()))
}

// ---------------------------------------------------------------------------
// GET /portal/:slug/:file  — serve static assets (style.css, app.js)
// ---------------------------------------------------------------------------

pub async fn serve_portal_asset(
    State(app): State<Arc<AppState>>,
    Path((slug, file)): Path<(String, String)>,
) -> ApiResult<Response<Body>> {
    let portal_dir = find_portal_dir(&app, &slug).await?;

    // Only allow safe filenames (no path traversal)
    let safe_file: String = file
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
        .collect();

    let file_path = portal_dir.join(&safe_file);
    if !file_path.exists() || !file_path.is_file() {
        return Err(ApiError::NotFound(format!("Asset {} not found", safe_file)));
    }

    let content = std::fs::read(&file_path)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let content_type = mime_type(&safe_file);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(content))
        .map_err(|e| ApiError::Internal(e.to_string()))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn find_portal_dir(
    app: &Arc<AppState>,
    slug: &str,
) -> ApiResult<std::path::PathBuf> {
    let project_id = {
        let db = app.db.lock().await;
        let ps = nexus_store::PortalService::new(&db);
        let portal = ps
            .get_portal_by_slug(slug)?
            .ok_or_else(|| ApiError::NotFound(format!("Portal '{}' not found", slug)))?;
        portal.project_id
    };

    Ok(app.project_portal_dir(&project_id))
}

fn mime_type(filename: &str) -> &'static str {
    if filename.ends_with(".css") {
        "text/css"
    } else if filename.ends_with(".js") {
        "application/javascript"
    } else if filename.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if filename.ends_with(".json") {
        "application/json"
    } else if filename.ends_with(".png") {
        "image/png"
    } else if filename.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "application/octet-stream"
    }
}
