//! Code graph endpoints.

use std::sync::Arc;
use axum::{extract::{Query, State}, Json};
use serde::Deserialize;
use serde_json::json;

use crate::{
    code_graph::CodeGraph,
    error::{ApiError, ApiResult},
    security::auth::Scope,
    security::project_access::ProjectAccess,
    state::AppState,
};

#[derive(Deserialize)]
pub struct ImpactQuery { pub file: Option<String> }

#[derive(Deserialize)]
pub struct SymbolQuery { pub name: Option<String> }

/// GET /projects/:id/code-graph — get the full code graph
pub async fn get_code_graph(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let dir = app.data_dir.join("projects").join(&project_id).join("generated");
    if !dir.exists() { return Err(ApiError::BadRequest("No generated code".into())); }
    let graph = CodeGraph::load_or_build(&dir);
    Ok(Json(json!(graph)))
}

/// POST /projects/:id/code-graph/rebuild — force rebuild
pub async fn rebuild_code_graph(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    let project_id = access.project_id.clone();
    let dir = app.data_dir.join("projects").join(&project_id).join("generated");
    if !dir.exists() { return Err(ApiError::BadRequest("No generated code".into())); }
    let graph = CodeGraph::build(&dir);
    graph.save(&dir);
    Ok(Json(json!({"rebuilt": true, "stats": graph.stats})))
}

/// GET /projects/:id/code-graph/impact?file=path — impact analysis for a file
pub async fn impact_analysis(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Query(q): Query<ImpactQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let dir = app.data_dir.join("projects").join(&project_id).join("generated");
    if !dir.exists() { return Err(ApiError::BadRequest("No generated code".into())); }
    let file = q.file.ok_or_else(|| ApiError::BadRequest("file parameter required".into()))?;
    let graph = CodeGraph::load_or_build(&dir);
    let impact = graph.impact_analysis(&file);
    Ok(Json(json!(impact)))
}

/// GET /projects/:id/code-graph/symbol?name=X — find symbol usages
pub async fn find_symbol(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Query(q): Query<SymbolQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let dir = app.data_dir.join("projects").join(&project_id).join("generated");
    if !dir.exists() { return Err(ApiError::BadRequest("No generated code".into())); }
    let name = q.name.ok_or_else(|| ApiError::BadRequest("name parameter required".into()))?;
    let graph = CodeGraph::load_or_build(&dir);
    let usages = graph.find_usages(&name);
    Ok(Json(json!({"symbol": name, "usages": usages})))
}
