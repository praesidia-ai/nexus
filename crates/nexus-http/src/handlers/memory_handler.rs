//! Persistent agent memory endpoints — episodic + semantic + vector.
//!
//! These endpoints expose the `nexus-memory` crate's capabilities via HTTP:
//! recording episodes, recalling similar episodes, learning knowledge facts,
//! querying the knowledge base, consolidation, and health reporting.

use std::sync::Arc;

use axum::{
    extract::State,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    error::ApiResult,
    security::project_access::ProjectAccess,
    state::AppState,
};
use nexus_memory::{
    episodic::{ConsolidationReport, Episode, RecallFilters},
    semantic::{KnowledgeCategory, KnowledgeFact},
    MemoryHealth,
};

// ---------------------------------------------------------------------------
// Request / Response DTOs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RecordEpisodeReq {
    pub episode: Episode,
}

#[derive(Deserialize)]
pub struct RecallReq {
    pub query_embedding: Vec<f32>,
    #[serde(default)]
    pub filters: RecallFiltersDto,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    10
}

#[derive(Deserialize, Default)]
pub struct RecallFiltersDto {
    pub tenant_id: Option<String>,
    pub project_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub min_importance: Option<f32>,
    pub since: Option<String>,
}

impl From<RecallFiltersDto> for RecallFilters {
    fn from(dto: RecallFiltersDto) -> Self {
        RecallFilters {
            tenant_id: dto.tenant_id,
            project_id: dto.project_id,
            tags: dto.tags,
            min_importance: dto.min_importance,
            since: dto.since,
        }
    }
}

#[derive(Deserialize)]
pub struct LearnFactReq {
    pub fact: KnowledgeFact,
}

#[derive(Deserialize)]
pub struct QueryKnowledgeReq {
    pub query_embedding: Vec<f32>,
    pub categories: Option<Vec<KnowledgeCategory>>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Deserialize)]
pub struct ConsolidateReq {
    #[serde(default = "default_retention_days")]
    pub retention_days: u64,
}

fn default_retention_days() -> u64 {
    90
}

#[derive(Serialize)]
pub struct RecordResponse {
    pub ok: bool,
    pub id: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `POST /memory/episodes` — Record a new episode.
pub async fn record_episode(
    State(app): State<Arc<AppState>>,
    Json(body): Json<RecordEpisodeReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let mem = app.memory_system()?;
    mem.episodic.record(&body.episode)?;
    Ok(Json(json!({ "ok": true, "id": body.episode.id })))
}

/// `POST /memory/recall` — Recall episodes similar to a query embedding.
pub async fn recall_episodes(
    State(app): State<Arc<AppState>>,
    Json(body): Json<RecallReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let mem = app.memory_system()?;
    let filters: RecallFilters = body.filters.into();
    let episodes = mem.episodic.recall(&body.query_embedding, &filters, body.limit.min(1000))?;
    Ok(Json(json!({ "episodes": episodes })))
}

/// `GET /memory/episodes/project/:id` — Get project episode history.
pub async fn project_episode_history(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let mem = app.memory_system()?;
    let episodes = mem.episodic.project_history(&project_id, 50)?;
    Ok(Json(json!({ "episodes": episodes })))
}

/// `GET /memory/knowledge` — List top knowledge facts (by confidence).
///
/// Used by the knowledge UI to show what the system has learned. Does not
/// require an embedding; returns a flat list keyed by `subject`.
pub async fn list_knowledge(
    State(app): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let mem = app.memory_system()?;
    let facts = mem
        .semantic
        .top_facts_by_tenant("default", 100)
        .unwrap_or_default();
    let items: Vec<serde_json::Value> = facts
        .into_iter()
        .map(|f| {
            json!({
                "key": f.subject,
                "value": f.content,
                "category": format!("{:?}", f.category).to_lowercase(),
                "confidence": f.confidence,
            })
        })
        .collect();
    Ok(Json(json!({ "items": items })))
}

/// `POST /memory/knowledge` — Learn or reinforce a knowledge fact.
pub async fn learn_knowledge(
    State(app): State<Arc<AppState>>,
    Json(body): Json<LearnFactReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let mem = app.memory_system()?;
    mem.semantic.learn(&body.fact)?;
    Ok(Json(json!({ "ok": true, "id": body.fact.id })))
}

/// `POST /memory/knowledge/query` — Query knowledge base by embedding similarity.
pub async fn query_knowledge(
    State(app): State<Arc<AppState>>,
    Json(body): Json<QueryKnowledgeReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let mem = app.memory_system()?;
    let cats = body.categories.as_deref();
    let facts = mem.semantic.query(&body.query_embedding, cats, body.limit.min(1000))?;
    Ok(Json(json!({ "facts": facts })))
}

/// `POST /memory/consolidate` — Trigger episode consolidation.
pub async fn consolidate(
    State(app): State<Arc<AppState>>,
    Json(body): Json<ConsolidateReq>,
) -> ApiResult<Json<ConsolidationReport>> {
    let mem = app.memory_system()?;
    let report = mem.episodic.consolidate(body.retention_days)?;
    Ok(Json(report))
}

/// `GET /memory/health` — Memory system health summary.
pub async fn memory_health(
    State(app): State<Arc<AppState>>,
) -> ApiResult<Json<MemoryHealth>> {
    let mem = app.memory_system()?;
    // Use a default tenant for global health; callers can specify tenant via query param
    let health = mem.health("default");
    Ok(Json(health))
}
