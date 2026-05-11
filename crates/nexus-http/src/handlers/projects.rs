//! Project CRUD handlers.
//!
//! Every project-scoped handler extracts [`AuthContext`] from request extensions
//! (set by `auth_middleware`) and calls [`validate_project_access`] to enforce
//! tenant isolation before any database operation.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use nexus_store::ProjectService;

use crate::{
    error::{ApiError, ApiResult},
    security::auth::AuthContext,
    security::tenant::validate_project_access,
    state::AppState,
};


// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateProjectReq {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Serialize)]
pub struct ProjectResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub phase: i64,
    pub llm_provider: Option<String>,
    pub llm_model: Option<String>,
    pub tenant_id: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<nexus_store::Project> for ProjectResponse {
    fn from(p: nexus_store::Project) -> Self {
        Self {
            id: p.id,
            name: p.name,
            description: p.description,
            phase: p.phase,
            llm_provider: p.llm_provider,
            llm_model: p.llm_model,
            tenant_id: p.tenant_id,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /projects — list projects visible to the authenticated tenant.
pub async fn list_projects(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
) -> ApiResult<Json<Vec<ProjectResponse>>> {
    let db = app.db.lock().await;
    let svc = ProjectService::new(&db);
    let projects = svc.list_projects_for_tenant(&auth.tenant_id)?;
    Ok(Json(projects.into_iter().map(Into::into).collect()))
}

/// POST /projects
///
/// Creates the project row, an initial conversation, and the first user
/// message atomically inside one SQLite transaction so a partial failure
/// cannot leave orphaned conversations or messages behind.
pub async fn create_project(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<CreateProjectReq>,
) -> ApiResult<(StatusCode, Json<ProjectResponse>)> {
    // Cap user-supplied strings before they touch the DB or prompt pipeline.
    crate::input_limits::require_bounded(
        "name",
        &body.name,
        crate::input_limits::MAX_PROJECT_NAME_BYTES,
    )?;
    crate::input_limits::require_bounded_opt(
        "description",
        body.description.as_deref(),
        crate::input_limits::MAX_DESCRIPTION_BYTES,
    )?;
    let mut db = app.db.lock().await;
    let tx = db
        .transaction()
        .map_err(|e| ApiError::Internal(format!("begin tx: {e}")))?;

    let project = {
        let svc = ProjectService::new(&tx);
        let project = svc.create_project(
            &body.name,
            body.description.as_deref(),
            &auth.tenant_id,
        )?;
        if let Some(desc) = body.description.as_deref().filter(|s| !s.trim().is_empty()) {
            let conv = svc.create_conversation(&project.id)?;
            svc.append_nexus_message(&conv.id, "user", desc, None)?;
        }
        project
    };

    tx.commit()
        .map_err(|e| ApiError::Internal(format!("commit tx: {e}")))?;

    Ok((StatusCode::CREATED, Json(project.into())))
}

/// GET /projects/:id
pub async fn get_project(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(id): Path<String>,
) -> ApiResult<Json<ProjectResponse>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let svc = ProjectService::new(&db);
    let project = svc
        .get_project(&id)?
        .ok_or_else(|| ApiError::NotFound(format!("project {} not found", id)))?;
    Ok(Json(project.into()))
}

/// DELETE /projects/:id
///
/// Deletes the project row (cascades to conversations/messages/etc via FK)
/// and — crucially — removes the project's on-disk directory so we do not
/// leak generated files + agent state after deletion.
pub async fn delete_project(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    // Stop any running app instance BEFORE removing the project row, so the
    // process/container does not outlive its parent record. Best-effort —
    // individual stop failures are logged but do not block deletion.
    let running = {
        let db = app.db.lock().await;
        validate_project_access(&db, &id, &auth.tenant_id)
            .map_err(ApiError::Forbidden)?;
        let svc = nexus_store::AppRunnerService::new(&db);
        svc.get_running_instance(&id).ok().flatten()
    };
    if let Some(instance) = running {
        if instance.sandbox {
            if let Some(cid) = instance.container_id.as_deref() {
                if let Err(e) = nexus_store::app_runner::docker_stop(cid) {
                    tracing::warn!(
                        project_id = %id,
                        container_id = %cid,
                        error = %e,
                        "failed to stop container during project delete",
                    );
                }
            }
        } else if let Some(pid) = instance.pid {
            if let Err(e) = nexus_store::app_runner::stop_process(pid) {
                tracing::warn!(
                    project_id = %id,
                    pid = pid,
                    error = %e,
                    "failed to stop process during project delete",
                );
            }
        }
        let db = app.db.lock().await;
        let svc = nexus_store::AppRunnerService::new(&db);
        let _ = svc.update_status(&instance.id, "stopped", None, None);
    }

    {
        let db = app.db.lock().await;
        validate_project_access(&db, &id, &auth.tenant_id)
            .map_err(ApiError::Forbidden)?;
        let svc = ProjectService::new(&db);
        svc.get_project(&id)?
            .ok_or_else(|| ApiError::NotFound(format!("project {} not found", id)))?;
        db.execute("DELETE FROM projects WHERE id = ?1", rusqlite::params![id])?;
    }

    // Off-lock filesystem cleanup. Best-effort: if the directory is missing
    // or locked by a running process, we log and move on — the DB row is
    // already gone, which is the source of truth the UI renders from.
    let project_dir = app.data_dir.join("projects").join(&id);
    if project_dir.exists() {
        if let Err(e) = std::fs::remove_dir_all(&project_dir) {
            tracing::warn!(
                project_id = %id,
                dir = %project_dir.display(),
                error = %e,
                "Failed to remove project directory after DB delete; orphaned files left on disk",
            );
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

/// PATCH /projects/:id/phase  — body: {"phase": 2}
#[derive(Deserialize)]
pub struct UpdatePhaseReq {
    pub phase: i64,
}

pub async fn update_phase(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(id): Path<String>,
    Json(body): Json<UpdatePhaseReq>,
) -> ApiResult<Json<ProjectResponse>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let svc = ProjectService::new(&db);
    svc.update_project_phase(&id, body.phase)?;
    let project = svc
        .get_project(&id)?
        .ok_or_else(|| ApiError::NotFound(format!("project {} not found", id)))?;
    Ok(Json(project.into()))
}

/// GET /projects/:id/conversations
pub async fn list_conversations(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let svc = ProjectService::new(&db);
    let convs = svc.list_conversations(&project_id)?;
    Ok(Json(serde_json::to_value(convs)?))
}

/// POST /projects/:id/fork — duplicate a project (row + generated files + first user message).
#[derive(Deserialize, Default)]
pub struct ForkProjectReq {
    pub name: Option<String>,
}

pub async fn fork_project(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(id): Path<String>,
    Json(body): Json<ForkProjectReq>,
) -> ApiResult<(StatusCode, Json<ProjectResponse>)> {
    // Scope the lock so we release it before doing the filesystem copy.
    let (new_project, src_prompt): (nexus_store::Project, Option<String>) = {
        let mut db = app.db.lock().await;
        validate_project_access(&db, &id, &auth.tenant_id)
            .map_err(ApiError::Forbidden)?;

        // Read source state before opening a transaction.
        let (src_name, src_description, prompt) = {
            let svc = ProjectService::new(&db);
            let src = svc
                .get_project(&id)?
                .ok_or_else(|| ApiError::NotFound(format!("project {} not found", id)))?;
            let prompt = svc
                .list_conversations(&id)
                .ok()
                .and_then(|convs| convs.into_iter().next())
                .and_then(|conv| svc.list_messages(&conv.id).ok())
                .and_then(|msgs| {
                    msgs.into_iter()
                        .find(|m| m.role == "user")
                        .map(|m| m.content)
                });
            (src.name, src.description, prompt)
        };

        let name = body
            .name
            .as_deref()
            .map(str::to_string)
            .unwrap_or_else(|| format!("{} (fork)", src_name));

        // Wrap the new-project + conversation + message writes in a transaction
        // so a mid-sequence failure cannot leave orphaned rows behind.
        let tx = db
            .transaction()
            .map_err(|e| ApiError::Internal(format!("begin tx: {e}")))?;
        let forked = {
            let svc = ProjectService::new(&tx);
            let forked = svc.create_project(&name, src_description.as_deref(), &auth.tenant_id)?;
            if let Some(ref p) = prompt {
                if !p.trim().is_empty() {
                    let conv = svc.create_conversation(&forked.id)?;
                    svc.append_nexus_message(&conv.id, "user", p, None)?;
                }
            }
            forked
        };
        tx.commit()
            .map_err(|e| ApiError::Internal(format!("commit tx: {e}")))?;

        (forked, prompt)
    };

    // Copy generated files off-lock.
    let src_dir = app
        .data_dir
        .join("projects")
        .join(&id)
        .join("generated");
    let dst_dir = app
        .data_dir
        .join("projects")
        .join(&new_project.id)
        .join("generated");
    if src_dir.exists() {
        if let Err(e) = fork_copy_dir(&src_dir, &dst_dir) {
            // Filesystem copy failed — roll back the DB row so we don't leave orphaned metadata.
            let db = app.db.lock().await;
            let _ = db.execute(
                "DELETE FROM projects WHERE id = ?1",
                rusqlite::params![new_project.id],
            );
            return Err(ApiError::Internal(format!(
                "Failed to copy project files: {e}"
            )));
        }
    }
    let _ = src_prompt;

    Ok((StatusCode::CREATED, Json(new_project.into())))
}

fn fork_copy_dir(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let name = entry.file_name();
        // Skip heavy / regenerable directories.
        if name == "node_modules" || name == ".next" || name == "target" || name == ".git" {
            continue;
        }
        let dest = dst.join(&name);
        if ft.is_dir() {
            fork_copy_dir(&entry.path(), &dest)?;
        } else if ft.is_file() {
            std::fs::copy(entry.path(), &dest)?;
        }
        // Symlinks deliberately not followed to avoid escaping the project root.
    }
    Ok(())
}

/// POST /projects/:id/conversations
pub async fn create_conversation(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let svc = ProjectService::new(&db);
    svc.get_project(&project_id)?
        .ok_or_else(|| ApiError::NotFound(format!("project {} not found", project_id)))?;
    let conv = svc.create_conversation(&project_id)?;
    Ok((StatusCode::CREATED, Json(serde_json::to_value(conv)?)))
}

/// GET /projects/:id/conversations/:conv_id/messages
pub async fn list_messages(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, conv_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let svc = ProjectService::new(&db);
    let messages = svc.list_messages(&conv_id)?;
    Ok(Json(serde_json::to_value(messages)?))
}

// ---------------------------------------------------------------------------
// Project-level LLM model
// ---------------------------------------------------------------------------

/// GET /projects/:id/model — get the project's LLM provider/model
pub async fn get_project_model(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let svc = ProjectService::new(&db);
    let project = svc
        .get_project(&id)?
        .ok_or_else(|| ApiError::NotFound(format!("project {} not found", id)))?;
    Ok(Json(serde_json::json!({
        "provider": project.llm_provider,
        "model": project.llm_model,
    })))
}

#[derive(Deserialize)]
pub struct SetProjectModelReq {
    pub provider: Option<String>,
    pub model: Option<String>,
}

/// POST /projects/:id/model — set the project's LLM provider/model (null to clear)
pub async fn set_project_model(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(id): Path<String>,
    Json(body): Json<SetProjectModelReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let svc = ProjectService::new(&db);
    svc.get_project(&id)?
        .ok_or_else(|| ApiError::NotFound(format!("project {} not found", id)))?;
    svc.update_project_model(&id, body.provider.as_deref(), body.model.as_deref())?;
    Ok(Json(serde_json::json!({
        "status": "ok",
        "provider": body.provider,
        "model": body.model,
    })))
}
