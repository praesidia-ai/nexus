//! Starter templates HTTP handlers — browse and use pre-built app templates.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};

use nexus_store::TemplateService;

use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
};

/// GET /templates — list all starter templates
pub async fn list_templates(
    State(app): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    let svc = TemplateService::new(&db);
    let templates = svc.list_templates()?;

    Ok(Json(serde_json::json!({ "templates": templates })))
}

/// GET /templates/:id — get a specific template
pub async fn get_template(
    State(app): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    let svc = TemplateService::new(&db);
    let template = svc
        .get_template(&id)?
        .ok_or_else(|| ApiError::NotFound(format!("Template '{}' not found", id)))?;

    Ok(Json(serde_json::to_value(template)?))
}

/// POST /templates/:id/use — use a template (increments popularity, returns prompt)
pub async fn use_template(
    State(app): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    let svc = TemplateService::new(&db);
    let template = svc
        .get_template(&id)?
        .ok_or_else(|| ApiError::NotFound(format!("Template '{}' not found", id)))?;

    svc.increment_popularity(&id)?;

    Ok(Json(serde_json::json!({
        "template": template,
        "prompt": template.preview_prompt,
    })))
}
