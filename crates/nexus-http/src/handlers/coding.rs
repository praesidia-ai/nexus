//! Coding agent handler — parallel feature development with smart agents.
//!
//! Each coding task runs through 4 phases:
//! 1. Planning   — Agent analyzes the request and produces a structured plan
//! 2. Coding     — Agent generates the actual code
//! 3. Validating — Agent reviews its own output for bugs, security, best practices
//! 4. Completed  — Output ready for the user
//!
//! Multiple tasks can run in parallel — each gets its own tokio task.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use nexus_store::{AgentSkillService, CodingTaskService, KnowledgeService, NewCodingTask};

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
pub struct CreateTaskReq {
    pub title: String,
    pub description: String,
    /// Agent to assign: "nova" (default), "orion" (security), "luna" (docs), etc.
    pub agent: Option<String>,
    pub priority: Option<i32>,
}

#[derive(Deserialize)]
pub struct BatchTasksReq {
    pub tasks: Vec<CreateTaskReq>,
}

#[derive(Serialize)]
pub struct TaskCreatedResponse {
    pub id: String,
    pub status: String,
}

#[derive(Serialize)]
pub struct BatchCreatedResponse {
    pub tasks: Vec<TaskCreatedResponse>,
}

// ---------------------------------------------------------------------------
// POST /projects/:id/coding/tasks — Create and start a coding task
// ---------------------------------------------------------------------------

pub async fn create_task(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
    Json(body): Json<CreateTaskReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let task = {
        let db = app.db.lock().await;
        validate_project_access(&db, &project_id, &auth.tenant_id)
            .map_err(ApiError::Forbidden)?;
        let svc = CodingTaskService::new(&db);
        svc.create_task(
            &project_id,
            &NewCodingTask {
                title: body.title.clone(),
                description: body.description.clone(),
                agent: body.agent.clone(),
                priority: body.priority,
            },
        )?
    };

    let task_id = task.id.clone();
    let title = body.title;
    let description = body.description;
    let agent = body.agent.unwrap_or_else(|| "nova".to_string());

    // Spawn the coding task in the background
    let app_bg = app.clone();
    let pid = project_id.clone();
    let tid = task_id.clone();
    tokio::spawn(async move {
        execute_coding_task(tid, pid, title, description, agent, app_bg).await;
    });

    Ok(Json(serde_json::json!({
        "id": task_id,
        "status": "pending"
    })))
}

// ---------------------------------------------------------------------------
// GET /projects/:id/coding/tasks — List all tasks
// ---------------------------------------------------------------------------

pub async fn list_tasks(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
) -> ApiResult<Json<Vec<nexus_store::CodingTask>>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let svc = CodingTaskService::new(&db);
    let tasks = svc.list_tasks(&project_id)?;
    Ok(Json(tasks))
}

// ---------------------------------------------------------------------------
// GET /projects/:id/coding/tasks/:task_id — Get single task
// ---------------------------------------------------------------------------

pub async fn get_task(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, task_id)): Path<(String, String)>,
) -> ApiResult<Json<nexus_store::CodingTask>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let svc = CodingTaskService::new(&db);
    let task = svc
        .get_task(&task_id)?
        .ok_or_else(|| ApiError::NotFound(format!("task {} not found", task_id)))?;

    if task.project_id != project_id {
        return Err(ApiError::NotFound(format!("task {} not found in project {}", task_id, project_id)));
    }

    Ok(Json(task))
}

// ---------------------------------------------------------------------------
// DELETE /projects/:id/coding/tasks/:task_id — Delete a task
// ---------------------------------------------------------------------------

pub async fn delete_task(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, task_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let svc = CodingTaskService::new(&db);
    svc.delete_task(&task_id)?;
    Ok(Json(serde_json::json!({"deleted": true})))
}

// ---------------------------------------------------------------------------
// POST /projects/:id/coding/tasks/batch — Create MULTIPLE tasks (parallel)
// ---------------------------------------------------------------------------

pub async fn create_batch(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
    Json(body): Json<BatchTasksReq>,
) -> ApiResult<Json<BatchCreatedResponse>> {
    // Validate tenant access once upfront
    {
        let db = app.db.lock().await;
        validate_project_access(&db, &project_id, &auth.tenant_id)
            .map_err(ApiError::Forbidden)?;
    }

    let mut created = Vec::with_capacity(body.tasks.len());

    for task_req in body.tasks {
        let task = {
            let db = app.db.lock().await;
            let svc = CodingTaskService::new(&db);
            svc.create_task(
                &project_id,
                &NewCodingTask {
                    title: task_req.title.clone(),
                    description: task_req.description.clone(),
                    agent: task_req.agent.clone(),
                    priority: task_req.priority,
                },
            )?
        };

        let task_id = task.id.clone();
        let title = task_req.title;
        let description = task_req.description;
        let agent = task_req.agent.unwrap_or_else(|| "nova".to_string());

        // Spawn each task in parallel
        let app_bg = app.clone();
        let pid = project_id.clone();
        let tid = task_id.clone();
        tokio::spawn(async move {
            execute_coding_task(tid, pid, title, description, agent, app_bg).await;
        });

        created.push(TaskCreatedResponse {
            id: task_id,
            status: "pending".to_string(),
        });
    }

    Ok(Json(BatchCreatedResponse { tasks: created }))
}

// ---------------------------------------------------------------------------
// Public entry point for chat.rs to spawn coding tasks
// ---------------------------------------------------------------------------

pub async fn execute_coding_task_from_chat(
    task_id: String,
    project_id: String,
    title: String,
    description: String,
    agent: String,
    app: Arc<AppState>,
) {
    execute_coding_task(task_id, project_id, title, description, agent, app).await;
}

// ---------------------------------------------------------------------------
// Core coding task execution — runs in a spawned tokio task
// ---------------------------------------------------------------------------

async fn execute_coding_task(
    task_id: String,
    project_id: String,
    title: String,
    description: String,
    agent: String,
    app: Arc<AppState>,
) {
    // Phase 1: PLANNING
    update_task_status(&app, &task_id, "planning", "Analyzing requirements and creating plan...").await;

    // Gather project context (knowledge items, existing tables, existing agents)
    let context = gather_project_context(&app, &project_id).await;

    // Compose the agent's full prompt from assigned skills (if any)
    let agent_identity = {
        let db = app.db.lock().await;
        let skill_svc = AgentSkillService::new(&db);
        let ab = nexus_store::AgentBuilder::new(&db);

        let agents = ab.list_agents(&project_id).unwrap_or_default();
        let matching_agent = agents.iter().find(|a| {
            a.name.to_lowercase().contains(&agent)
        });

        if let Some(agent_def) = matching_agent {
            skill_svc
                .compose_agent_prompt(&agent_def.id, &agent_def.system_prompt)
                .unwrap_or_else(|_| agent_def.system_prompt.clone())
        } else {
            format!(
                "You are {}, a specialist coding agent.",
                agent_display_name(&agent)
            )
        }
    };

    let plan_prompt = format!(
        r#"{agent_identity}

You need to implement this feature:

Title: {title}
Description: {description}

Project context:
{context}

Create a detailed implementation plan. Respond with ONLY a JSON object:
{{
  "approach": "Brief description of the approach",
  "files": [
    {{"path": "src/app/api/feature/route.ts", "action": "create", "description": "API endpoint for..."}},
    {{"path": "src/app/feature/page.tsx", "action": "create", "description": "UI page for..."}}
  ],
  "dependencies": ["list of new npm packages if any"],
  "estimated_complexity": "low|medium|high",
  "risks": ["potential risks or edge cases"]
}}"#,
        agent_identity = agent_identity,
        title = title,
        description = description,
        context = context,
    );

    let plan_result = super::chat::call_llm_simple_for_project(&app, &plan_prompt, Some(&project_id)).await;
    match plan_result {
        Ok(plan_text) => {
            let plan_json = parse_json_from_llm(&plan_text);
            save_plan(&app, &task_id, &plan_json).await;
            save_files(
                &app,
                &task_id,
                plan_json
                    .get("files")
                    .cloned()
                    .unwrap_or(serde_json::json!([])),
            )
            .await;
        }
        Err(e) => {
            save_error(&app, &task_id, &format!("Planning failed: {}", e)).await;
            return;
        }
    }

    // Phase 2: CODING
    update_task_status(&app, &task_id, "coding", "Generating code...").await;

    let code_prompt = format!(
        r#"{agent_identity}

You are implementing this feature:

Title: {title}
Description: {description}

Based on your plan, generate the complete code for ALL files listed. For each file, output:

=== FILE: path/to/file.ts ===
(complete file contents here)
=== END FILE ===

Rules:
- Generate COMPLETE, RUNNABLE code — no placeholders, no "TODO" comments
- Use TypeScript with strict types
- Follow Next.js 15 App Router patterns
- Include proper error handling
- Use Tailwind CSS for styling
- If it's an API route, use NextRequest/NextResponse
- If it's a page, use 'use client' when needed
- Be thorough — every import, every type, every function"#,
        agent_identity = agent_identity,
        title = title,
        description = description,
    );

    let code_result = super::chat::call_llm_simple_for_project(&app, &code_prompt, Some(&project_id)).await;
    match code_result {
        Ok(code_output) => {
            save_output(&app, &task_id, &code_output).await;
        }
        Err(e) => {
            save_error(&app, &task_id, &format!("Coding failed: {}", e)).await;
            return;
        }
    }

    // Phase 3: VALIDATING
    update_task_status(&app, &task_id, "validating", "Reviewing code for quality and security...").await;

    let code_for_review = get_task_output(&app, &task_id).await.unwrap_or_default();

    let validate_prompt = format!(
        r#"You are Orion, a security and quality reviewer. Review the following code:

{code}

Check for:
1. Security issues (injection, XSS, auth bypass, secrets in code)
2. Logic bugs (off-by-one, null safety, race conditions)
3. Best practices (TypeScript strictness, error handling, accessibility)
4. Performance (N+1 queries, memory leaks, unnecessary re-renders)

Respond with ONLY a JSON object:
{{
  "score": 1-10,
  "issues": [
    {{"severity": "critical|high|medium|low", "file": "path", "line_hint": "near X", "description": "what's wrong", "fix": "how to fix"}}
  ],
  "summary": "Overall assessment in one sentence"
}}"#,
        code = code_for_review,
    );

    let review_result = super::chat::call_llm_simple_for_project(&app, &validate_prompt, Some(&project_id)).await;
    match review_result {
        Ok(review_text) => {
            let review_json = parse_json_from_llm(&review_text);
            save_review(&app, &task_id, &review_json).await;
        }
        Err(e) => {
            // Review failure is non-fatal — mark as completed anyway
            error!("Review failed for task {}: {}", task_id, e);
        }
    }

    // Phase 4: COMPLETED
    update_task_status(&app, &task_id, "completed", "Done").await;
    info!(task_id = %task_id, "Coding task completed");
}

// ---------------------------------------------------------------------------
// Helper: gather project context from DB
// ---------------------------------------------------------------------------

async fn gather_project_context(app: &Arc<AppState>, project_id: &str) -> String {
    let db = app.db.lock().await;

    let mut parts: Vec<String> = Vec::new();

    // Knowledge items
    if let Ok(svc) = std::result::Result::<_, ()>::Ok(KnowledgeService::new(&db)) {
        if let Ok(items) = svc.list_items(project_id) {
            if !items.is_empty() {
                let mut section = String::from("Knowledge items:\n");
                for item in &items {
                    section.push_str(&format!(
                        "- {} ({}): {}\n",
                        item.name,
                        item.item_type,
                        item.description.as_deref().unwrap_or("")
                    ));
                }
                parts.push(section);
            }
        }
    }

    // Existing tables
    {
        let sb = nexus_store::SchemaBuilder::new(&db);
        if let Ok(tables) = sb.list_tables(project_id) {
            if !tables.is_empty() {
                let mut section = String::from("Existing database tables:\n");
                for t in &tables {
                    section.push_str(&format!("- {}\n", t.table_name));
                }
                parts.push(section);
            }
        }
    }

    // Existing agents
    {
        let ab = nexus_store::AgentBuilder::new(&db);
        if let Ok(agents) = ab.list_agents(project_id) {
            if !agents.is_empty() {
                let mut section = String::from("Existing agents:\n");
                for a in &agents {
                    section.push_str(&format!("- {} ({})\n", a.name, a.role));
                }
                parts.push(section);
            }
        }
    }

    if parts.is_empty() {
        "No existing project context yet.".to_string()
    } else {
        parts.join("\n")
    }
}

// ---------------------------------------------------------------------------
// Helper: agent display name
// ---------------------------------------------------------------------------

fn agent_display_name(agent: &str) -> &str {
    match agent {
        "nova" => "Nova (Coder)",
        "atlas" => "Atlas (Cloud)",
        "kai" => "Kai (Researcher)",
        "luna" => "Luna (Writer)",
        "orion" => "Orion (Security)",
        "sage" => "Sage (Data)",
        "ivy" => "Ivy (Marketer)",
        "rex" => "Rex (DevOps)",
        "leo" => "Leo (PM)",
        "mia" => "Mia (Support)",
        other => other,
    }
}

// ---------------------------------------------------------------------------
// Helper: parse JSON from LLM output (strips markdown fences)
// ---------------------------------------------------------------------------

fn parse_json_from_llm(text: &str) -> serde_json::Value {
    let trimmed = text.trim();

    // Strip markdown code fences if present
    let json_str = if trimmed.starts_with("```") {
        let after_first_fence = if let Some(nl) = trimmed.find('\n') {
            &trimmed[nl + 1..]
        } else {
            trimmed
        };
        if let Some(end) = after_first_fence.rfind("```") {
            &after_first_fence[..end]
        } else {
            after_first_fence
        }
    } else {
        trimmed
    };

    serde_json::from_str(json_str.trim()).unwrap_or_else(|_| {
        // Try to find JSON object within the text
        if let Some(start) = json_str.find('{') {
            if let Some(end) = json_str.rfind('}') {
                if let Ok(val) = serde_json::from_str(&json_str[start..=end]) {
                    return val;
                }
            }
        }
        serde_json::json!({"raw": text})
    })
}

// ---------------------------------------------------------------------------
// DB helper functions (lock app.db for each operation)
// ---------------------------------------------------------------------------

async fn update_task_status(app: &Arc<AppState>, task_id: &str, status: &str, detail: &str) {
    let db = app.db.lock().await;
    let svc = CodingTaskService::new(&db);
    if let Err(e) = svc.update_status(task_id, status, Some(detail)) {
        error!("Failed to update task {} status to {}: {}", task_id, status, e);
    }
}

async fn save_plan(app: &Arc<AppState>, task_id: &str, plan: &serde_json::Value) {
    let db = app.db.lock().await;
    let svc = CodingTaskService::new(&db);
    if let Err(e) = svc.set_plan(task_id, plan) {
        error!("Failed to save plan for task {}: {}", task_id, e);
    }
}

async fn save_files(app: &Arc<AppState>, task_id: &str, files: serde_json::Value) {
    let db = app.db.lock().await;
    let svc = CodingTaskService::new(&db);
    if let Err(e) = svc.set_files(task_id, &files) {
        error!("Failed to save files for task {}: {}", task_id, e);
    }
}

async fn save_output(app: &Arc<AppState>, task_id: &str, output: &str) {
    let db = app.db.lock().await;
    let svc = CodingTaskService::new(&db);
    if let Err(e) = svc.set_output(task_id, output) {
        error!("Failed to save output for task {}: {}", task_id, e);
    }
}

async fn save_review(app: &Arc<AppState>, task_id: &str, review: &serde_json::Value) {
    let db = app.db.lock().await;
    let svc = CodingTaskService::new(&db);
    if let Err(e) = svc.set_review(task_id, review) {
        error!("Failed to save review for task {}: {}", task_id, e);
    }
}

async fn save_error(app: &Arc<AppState>, task_id: &str, error_msg: &str) {
    let db = app.db.lock().await;
    let svc = CodingTaskService::new(&db);
    if let Err(e) = svc.set_error(task_id, error_msg) {
        error!("Failed to save error for task {}: {}", task_id, e);
    }
}

async fn get_task_output(app: &Arc<AppState>, task_id: &str) -> Option<String> {
    let db = app.db.lock().await;
    let svc = CodingTaskService::new(&db);
    svc.get_task(task_id).ok().flatten().and_then(|t| t.output)
}
