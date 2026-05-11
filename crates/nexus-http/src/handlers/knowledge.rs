//! Knowledge items handlers.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use nexus_store::{KnowledgeService, NewKnowledgeItem};

#[derive(Debug, Deserialize, Default)]
pub struct ListKnowledgeQuery {
    /// Max items to return (server clamps to [1, 1000]).
    #[serde(default)]
    pub limit: Option<usize>,
    /// Skip this many items before returning results.
    #[serde(default)]
    pub offset: Option<usize>,
}

use crate::{
    error::{ApiError, ApiResult},
    security::auth::AuthContext,
    security::tenant::validate_project_access,
    state::AppState,
};

// ---------------------------------------------------------------------------
// GET /projects/:id/knowledge
// ---------------------------------------------------------------------------

pub async fn list_knowledge(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
    Query(q): Query<ListKnowledgeQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let ks = KnowledgeService::new(&db);
    // Clamp pagination so a hostile client cannot ask for limit=99_999_999
    // and OOM the server materializing a giant Vec.
    let limit = q.limit.unwrap_or(200).clamp(1, 1000);
    let offset = q.offset.unwrap_or(0);
    let items = ks.list_items_paged(&project_id, limit, offset)?;
    let returned = items.len();
    Ok(Json(serde_json::json!({
        "items": items,
        "limit": limit,
        "offset": offset,
        "returned": returned,
    })))
}

// ---------------------------------------------------------------------------
// POST /projects/:id/knowledge
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct AddKnowledgeReq {
    pub item_type: String,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

pub async fn add_knowledge(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
    Json(body): Json<AddKnowledgeReq>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let ks = KnowledgeService::new(&db);
    let item = ks.add_item(
        &project_id,
        &NewKnowledgeItem {
            item_type: body.item_type,
            name: body.name,
            description: body.description,
            icon: body.icon,
            metadata: body.metadata,
        },
    )?;
    Ok((StatusCode::CREATED, Json(serde_json::to_value(item)?)))
}

// ---------------------------------------------------------------------------
// DELETE /projects/:id/knowledge/:item_id
// ---------------------------------------------------------------------------

pub async fn delete_knowledge(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, item_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let ks = KnowledgeService::new(&db);
    ks.delete_item(&item_id)?;
    Ok(StatusCode::NO_CONTENT)
}
