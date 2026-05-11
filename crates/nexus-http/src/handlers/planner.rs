//! Plan-before-code handler.
//!
//! Before code generation, the Planner agent (Leo) creates a structured
//! project plan that the user reviews and approves.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use nexus_store::{KnowledgeService, ProjectService};

use crate::{
    error::{ApiError, ApiResult},
    security::auth::AuthContext,
    security::tenant::validate_project_access,
    state::AppState,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectPlan {
    pub id: String,
    pub project_id: String,
    pub status: String, // "draft" | "approved" | "rejected" | "executing"
    pub summary: String,
    pub architecture: ArchitecturePlan,
    pub entities: Vec<EntityPlan>,
    pub agents: Vec<AgentPlan>,
    pub pages: Vec<PagePlan>,
    pub integrations: Vec<String>,
    pub estimated_files: usize,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitecturePlan {
    pub framework: String,
    pub database: String,
    pub auth: bool,
    /// Why auth was included or omitted (shown to the user in the plan review UI).
    #[serde(default)]
    pub auth_reason: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityPlan {
    pub name: String,
    pub description: String,
    pub fields: Vec<FieldPlan>,
    pub relationships: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldPlan {
    pub name: String,
    pub field_type: String,
    pub required: bool,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPlan {
    pub name: String,
    pub role: String,
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PagePlan {
    pub name: String,
    pub route: String,
    pub description: String,
    pub components: Vec<String>,
}

// POST /projects/:id/plan/generate -- ask the Planner agent to create a plan
#[derive(Deserialize)]
pub struct GeneratePlanReq {
    pub description: String,
}

pub async fn generate_plan(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
    Json(body): Json<GeneratePlanReq>,
) -> ApiResult<Json<serde_json::Value>> {
    info!(project = %project_id, "Generating project plan");

    // Gather existing knowledge for context
    let knowledge_context = {
        let db = app.db.lock().await;
        validate_project_access(&db, &project_id, &auth.tenant_id).map_err(ApiError::Forbidden)?;
        let ks = KnowledgeService::new(&db);
        let items = ks.list_items(&project_id).unwrap_or_default();
        items
            .iter()
            .map(|i| {
                format!(
                    "- {} ({}): {}",
                    i.name,
                    i.item_type,
                    i.description.as_deref().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    // Per-tenant + global rate-limit before any LLM IO. Without these a
    // single tenant can drive uncapped planning calls against the platform key.
    if let Err(reason) = app.tenant_rate_limiter.check(&auth.tenant_id, "generation") {
        return Err(ApiError::TooManyRequests(reason));
    }
    let _llm_slot = app
        .rate_limiter
        .acquire_llm_slot()
        .await
        .map_err(|e| ApiError::TooManyRequests(format!("planner queue is full: {e}")))?;

    let prompt = format!(
        r#"You are Leo, a senior Product Manager agent. Create a structured project plan for this application.

User's description: {}

Existing knowledge base:
{}

## Authentication decision (auto-detect from description)

Infer whether the app needs auth from the description alone:

`auth: true` → multi-user portals, customer-facing platforms, SaaS apps, admin panels with roles, any app where distinct users see their own private data.
`auth: false` → landing pages, single-operator tools, public dashboards, calculators, read-only apps, anything a single person runs for themselves.

When `auth: true`: plan a simple NextAuth.js credentials provider (email + password). No OAuth, no email verification.
Set `auth_reason` to one sentence explaining why you chose true or false (displayed to the user during plan review).

Respond with ONLY a JSON object (no markdown, no explanation) in this exact format:
{{
  "summary": "One paragraph describing what will be built",
  "architecture": {{
    "framework": "Next.js 15",
    "database": "SQLite",
    "auth": false,
    "auth_reason": "Single-operator tool — no login needed.",
    "description": "Brief architecture description"
  }},
  "entities": [
    {{
      "name": "EntityName",
      "description": "What this entity represents",
      "fields": [
        {{"name": "id", "field_type": "TEXT", "required": true, "description": "Primary key"}},
        {{"name": "field_name", "field_type": "TEXT|INTEGER|REAL", "required": true, "description": "Field purpose"}}
      ],
      "relationships": ["Related to OtherEntity via foreign_key"]
    }}
  ],
  "agents": [
    {{"name": "AgentName", "role": "What this agent does", "tools": ["tool1", "tool2"]}}
  ],
  "pages": [
    {{"name": "PageName", "route": "/route", "description": "Page purpose", "components": ["DataTable", "Form"]}}
  ],
  "integrations": ["Stripe payments", "Email notifications"]
}}"#,
        body.description, knowledge_context
    );

    // Call LLM using resolved provider config (respects Settings API keys)
    let content_str = super::chat::call_llm_simple_for_project(&app, &prompt, Some(&project_id))
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let content = content_str.as_str();

    // Parse the JSON plan from LLM response
    let plan_data: serde_json::Value = serde_json::from_str(
        content
            .trim()
            .trim_start_matches("```json")
            .trim_end_matches("```")
            .trim(),
    )
    .unwrap_or_else(|_| {
        serde_json::json!({
            "summary": content,
            "architecture": {"framework": "Next.js 15", "database": "SQLite", "auth": false, "auth_reason": "Defaulting to no authentication — edit the plan if login is required.", "description": "Standard web app"},
            "entities": [],
            "agents": [],
            "pages": [],
            "integrations": []
        })
    });

    let plan_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    // Count estimated files
    let entity_count = plan_data["entities"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    let estimated_files = entity_count * 2 + 10; // 2 per entity (page+api) + base files

    // Store plan in knowledge base
    {
        let db = app.db.lock().await;
        let ks = KnowledgeService::new(&db);
        let _ = ks.add_item(
            &project_id,
            &nexus_store::NewKnowledgeItem {
                item_type: "plan".to_string(),
                name: "Project Plan".to_string(),
                description: Some(plan_data["summary"].as_str().unwrap_or("").to_string()),
                icon: Some("\u{1f4cb}".to_string()),
                metadata: Some(plan_data.clone()),
            },
        );
    }

    let response = serde_json::json!({
        "id": plan_id,
        "project_id": project_id,
        "status": "draft",
        "plan": plan_data,
        "estimated_files": estimated_files,
        "created_at": now
    });

    Ok(Json(response))
}

// POST /projects/:id/plan/approve -- approve the plan and trigger codegen
pub async fn approve_plan(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<Json<serde_json::Value>> {
    // Validate tenant access before any work
    {
        let db = app.db.lock().await;
        validate_project_access(&db, &project_id, &auth.tenant_id).map_err(ApiError::Forbidden)?;
    }

    info!(project = %project_id, "Plan approved -- triggering codegen");

    let plan = body.get("plan").cloned().unwrap_or(serde_json::json!({}));

    // Convert plan to IR format for codegen
    let entities: Vec<serde_json::Value> = plan["entities"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|e| {
            let fields: Vec<serde_json::Value> = e["fields"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "name": f["name"],
                        "type": f["field_type"].as_str().unwrap_or("TEXT"),
                        "primary_key": f["name"].as_str() == Some("id"),
                        "not_null": f["required"].as_bool().unwrap_or(false)
                    })
                })
                .collect();
            serde_json::json!({
                "name": e["name"],
                "description": e["description"],
                "fields": fields,
                "materialize": true
            })
        })
        .collect();

    let agents: Vec<serde_json::Value> = plan["agents"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|a| {
            serde_json::json!({
                "name": a["name"],
                "role": a["role"],
                "tools": a["tools"]
            })
        })
        .collect();

    // Extract auth flag from the plan's architecture section
    let needs_auth = plan
        .get("architecture")
        .and_then(|a| a.get("auth"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let ir = serde_json::json!({
        "meta": {"version": "1.0", "project_id": project_id},
        "architecture": {"auth": needs_auth},
        "entities": entities,
        "agents": agents,
        "ui": {"portal": false},
        "code": {"language": "TypeScript", "framework": "Next.js"}
    });

    // Run codegen — LLM-powered with static fallback
    let output_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");
    let data_db = app.project_data_db(&project_id);

    // Step 1: Create plan (needs DB lock)
    let plan_result = {
        let db = app.db.lock().await;
        let materializer = nexus_store::CodeGenMaterializer::new(&db);
        materializer.plan(&project_id, &ir)?
    };
    // DB lock released

    // Step 2: LLM-powered generation — no static fallbacks.
    let summary = plan
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("A full-stack web application");
    let prompt = nexus_store::build_generation_prompt(&plan_result, summary, None);

    let generated_files = super::llm_codegen::generate_app_files(&app, &prompt, Some(&project_id))
        .await
        .map_err(crate::error::ApiError::Internal)?;

    info!(project = %project_id, file_count = generated_files.len(), "LLM generated files from planner approval");

    let result = {
        let db = app.db.lock().await;
        let materializer = nexus_store::CodeGenMaterializer::new(&db);
        materializer.generate_from_llm_output(
            &project_id,
            &output_dir,
            &plan_result,
            &data_db,
            &generated_files,
        )?
    };

    // Update project phase to 3 (Materialization)
    {
        let db = app.db.lock().await;
        let svc = ProjectService::new(&db);
        let _ = svc.update_project_phase(&project_id, 3);
    }

    Ok(Json(serde_json::json!({
        "status": "approved",
        "codegen_result": {
            "files_written": result.files_written.len(),
            "tables_created": result.tables_created.len(),
            "agents_configured": result.agents_configured.len(),
            "output_dir": result.output_dir,
            "valid": result.validation.valid,
            "errors": result.validation.errors
        }
    })))
}

// GET /projects/:id/plan -- get the current plan
pub async fn get_plan(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id).map_err(ApiError::Forbidden)?;
    let ks = KnowledgeService::new(&db);
    let items = ks.list_items(&project_id)?;
    let plan_item = items.iter().find(|i| i.item_type == "plan");

    match plan_item {
        Some(item) => Ok(Json(serde_json::json!({
            "exists": true,
            "plan": item.metadata,
            "name": item.name,
            "description": item.description
        }))),
        None => Ok(Json(serde_json::json!({"exists": false}))),
    }
}
