//! Intent -> Full App -- single prompt to deployed application.
//!
//! User says: "Airbnb for wine tastings"
//! System: creates project -> analyzes intent -> generates plan -> approves -> generates code -> starts app

use std::convert::Infallible;
use std::sync::Arc;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tracing::info;

use nexus_store::ProjectService;

use crate::{
    decision_engine,
    error::{ApiError, ApiResult},
    execution_pipeline::{ExecutionPipeline, PipelineEvent},
    intent_engine,
    plugin_hooks::{self, HookPoint},
    state::AppState,
};

#[derive(Deserialize)]
pub struct IntentReq {
    pub description: String,
    pub provider: Option<String>,
    pub model: Option<String>,
}

#[derive(Deserialize)]
pub struct AnalyzeReq {
    pub description: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type")]
pub enum IntentEvent {
    #[serde(rename = "step")]
    Step {
        step: String,
        status: String,
        detail: String,
    },
    #[serde(rename = "progress")]
    Progress { percent: u32, message: String },
    #[serde(rename = "intent")]
    Intent {
        app_type: String,
        needs_auth: bool,
        needs_database: bool,
        complexity: String,
        agents: Vec<serde_json::Value>,
        pages: Vec<String>,
        features: Vec<String>,
        confidence: f64,
    },
    #[serde(rename = "result")]
    Result {
        project_id: String,
        project_name: String,
        files_count: usize,
        app_url: Option<String>,
    },
    #[serde(rename = "error")]
    Error { step: String, message: String },
}

/// POST /intent/analyze -- analyze a description without generating anything
pub async fn analyze_intent_endpoint(
    Json(body): Json<AnalyzeReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let intent = intent_engine::analyze_flat(&body.description);
    Ok(Json(
        serde_json::to_value(intent).map_err(|e| ApiError::Internal(e.to_string()))?,
    ))
}

/// POST /intent-to-app -- create a full app from a description
pub async fn intent_to_app(
    State(app): State<Arc<AppState>>,
    auth: crate::security::auth::AuthContext,
    Json(body): Json<IntentReq>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let description = body.description.clone();
    let provider = body.provider;
    let model = body.model;

    // Per-tenant fair-share check before any work.
    if let Err(reason) = app.tenant_rate_limiter.check(&auth.tenant_id, "generation") {
        return Err(ApiError::TooManyRequests(reason));
    }

    // Reserve a global LLM concurrency slot. The guard is moved into the
    // spawned pipeline task and released when that task finishes — so the
    // slot covers the actual LLM work, not just the HTTP request lifetime.
    let llm_guard = app
        .rate_limiter
        .acquire_llm_slot()
        .await
        .map_err(|e| ApiError::TooManyRequests(format!("intent queue is full: {e}")))?;

    let app_bg = app.clone();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<IntentEvent>(50);

    tokio::spawn(async move {
        let _slot = llm_guard;
        run_intent_pipeline(app_bg, description, provider, model, tx).await;
    });

    let stream = async_stream::stream! {
        while let Some(event) = rx.recv().await {
            let json_str = serde_json::to_string(&event).unwrap_or_default();
            yield Ok::<_, Infallible>(Event::default().data(json_str));
        }
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    ))
}

async fn run_intent_pipeline(
    app: Arc<AppState>,
    description: String,
    _provider: Option<String>,
    _model: Option<String>,
    tx: tokio::sync::mpsc::Sender<IntentEvent>,
) {
    // Step 1: Create project
    let _ = tx
        .send(IntentEvent::Step {
            step: "create_project".into(),
            status: "running".into(),
            detail: "Creating project...".into(),
        })
        .await;
    let _ = tx
        .send(IntentEvent::Progress {
            percent: 5,
            message: "Setting up project...".into(),
        })
        .await;

    let project_name = extract_project_name(&description);
    let create_result = {
        let db = app.db.lock().await;
        let svc = ProjectService::new(&db);
        svc.create_project(&project_name, Some(&description), "default")
    };
    let project_id = match create_result {
        Ok(p) => {
            let _ = tx
                .send(IntentEvent::Step {
                    step: "create_project".into(),
                    status: "completed".into(),
                    detail: format!("Project '{}' created", project_name),
                })
                .await;
            p.id
        }
        Err(e) => {
            let _ = tx
                .send(IntentEvent::Error {
                    step: "create_project".into(),
                    message: e.to_string(),
                })
                .await;
            return;
        }
    };

    // Step 1.5: Analyze intent (deterministic, instant -- no LLM call)
    let intent = intent_engine::analyze_flat(&description);

    // Plugin hook: OnIntentParsed (CLAUDE.md invariant — fired in BOTH oneshot AND pipeline paths)
    let mut intent_hook_ctx = plugin_hooks::build_context_for_tenant(
        "intent_to_app",
        None,
        HookPoint::OnIntentParsed,
        Some(&description),
    );
    intent_hook_ctx.intent = serde_json::to_value(&intent).ok();
    let _ = plugin_hooks::fire_hook(&app, HookPoint::OnIntentParsed, &mut intent_hook_ctx).await;

    let _ = tx
        .send(IntentEvent::Intent {
            app_type: format!("{:?}", intent.app_type),
            needs_auth: intent.needs_auth,
            needs_database: intent.needs_database,
            complexity: format!("{:?}", intent.complexity),
            agents: intent
                .suggested_agents
                .iter()
                .map(|a| {
                    json!({
                        "name": a.name,
                        "type": a.agent_type,
                        "description": a.description,
                        "trigger": a.trigger
                    })
                })
                .collect(),
            pages: intent.suggested_pages.clone(),
            features: intent.inferred_features.clone(),
            confidence: intent.confidence,
        })
        .await;

    // Step 1.6: Decision Engine — make architecture decisions deterministically
    let decisions = decision_engine::decide(&intent);
    let decision_context = decision_engine::to_prompt_context(&decisions);

    // Persist decisions for observability
    let nexus_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated")
        .join(".nexus");
    let _ = std::fs::create_dir_all(&nexus_dir);
    if let Ok(json) = serde_json::to_string_pretty(&decisions) {
        let _ = std::fs::write(nexus_dir.join("decisions.json"), json);
    }

    let _ = tx
        .send(IntentEvent::Progress {
            percent: 10,
            message: "Intent analyzed, architecture decided, generating plan...".into(),
        })
        .await;

    // Step 2: Generate plan (use intent + decisions to build a better prompt)
    let _ = tx
        .send(IntentEvent::Step {
            step: "generate_plan".into(),
            status: "running".into(),
            detail: "AI is designing your application...".into(),
        })
        .await;

    let knowledge_context = {
        let db = app.db.lock().await;
        let ks = nexus_store::KnowledgeService::new(&db);
        ks.list_items(&project_id)
            .unwrap_or_default()
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

    // Build intent-aware plan prompt
    let intent_auth_hint = if intent.needs_auth {
        "The app NEEDS authentication (login/signup). Include auth in the architecture."
    } else {
        "The app does NOT need authentication."
    };
    let intent_db_hint = if intent.needs_database {
        "The app NEEDS a database. Include entity definitions."
    } else {
        "The app does NOT need a database."
    };
    let intent_payment_hint = if intent.needs_payments {
        "The app NEEDS payment processing (Stripe). Include billing/checkout flows."
    } else {
        ""
    };

    let agents_hint = if intent.suggested_agents.is_empty() {
        String::new()
    } else {
        let agent_list: Vec<String> = intent
            .suggested_agents
            .iter()
            .map(|a| format!("- {} ({}): {}", a.name, a.agent_type, a.description))
            .collect();
        format!(
            "\nSuggested AI agents to embed:\n{}\nIMPORTANT: Include these agents in the plan's 'agents' array.",
            agent_list.join("\n")
        )
    };

    let entities_hint = if intent.suggested_entities.is_empty() {
        String::new()
    } else {
        format!(
            "\nSuggested entities: {}",
            intent.suggested_entities.join(", ")
        )
    };

    let pages_hint = if intent.suggested_pages.is_empty() {
        String::new()
    } else {
        format!("\nSuggested pages: {}", intent.suggested_pages.join(", "))
    };

    let detected_stack_for_plan = nexus_store::detect_tech_stack(&description);
    let tech_hint = if detected_stack_for_plan.is_empty() {
        "Choose the best framework and language for this application. For web apps with UI, prefer Next.js 15. For pure APIs, choose the language that fits best (Python/FastAPI, Go, Rust, etc.).".to_string()
    } else {
        format!(
            "The user explicitly wants: **{}**. Plan around this technology.",
            detected_stack_for_plan
        )
    };

    let prompt = format!(
        r#"You are Leo, a senior technical product manager at a top-tier startup. Your job: understand EXACTLY what the user wants and produce a meticulous plan that a code generator will follow to build a PRODUCTION-QUALITY application that rivals what Lovable or Bolt.new would produce.

## User's Description
{description}

## Detected Intent
- App type: {app_type:?}
- {auth_hint}
- {db_hint}
- {payment_hint}
- UI style: {ui_style:?}
- Complexity: {complexity:?}
{agents_hint}
{entities_hint}
{pages_hint}

## Technology Decision
{tech_hint}

{decision_ctx}

{knowledge_block}

## Your Task

Think step by step about what the user is actually building. Then produce a JSON plan.

**CRITICAL for the summary**: Write a vivid, specific paragraph that reads like a product pitch. Not "A web application for managing tasks." Instead: "TaskFlow is a sleek project management platform where teams create branded workspaces, organize tasks into kanban boards with drag-and-drop, assign members with role-based permissions (Admin, Editor, Viewer), track time per task with a built-in timer, and visualize progress through interactive burndown charts and velocity metrics. The dashboard features a clean sidebar navigation, stat cards showing active tasks, overdue items, and team velocity, with a modern gradient hero section."

The summary is the PRIMARY input the code generator receives — vague summaries produce vague apps. Include: the app name, specific UI patterns (sidebar, cards, tables, charts), specific features with details, and the overall visual feel.

**CRITICAL for entities**: Model the REAL domain comprehensively. A recipe app needs `Recipe` (title, description, prep_time, cook_time, servings, difficulty, image_url, created_at), `Ingredient` (name, category), `RecipeIngredient` (recipe_id, ingredient_id, quantity, unit), `Category` (name, icon, color), `User` (name, email, avatar_url, bio), `Review` (recipe_id, user_id, rating, comment, created_at). Not just `Recipe` and `User`. Every list, card, chart, and detail page needs backing entities with the right fields. Include 6-8 specific fields per entity minimum.

**CRITICAL for pages**: List EVERY page the app needs with specific detail about what data is displayed and what actions are available. Each page description should be 2-3 sentences. Include: what components are on the page (tables, cards, forms, charts), what data they show, and what the user can do (create, edit, filter, sort, export).

Respond with ONLY a JSON object:
{{
  "summary": "A vivid, specific paragraph describing exactly what gets built — name the app, describe key workflows, mention specific UI patterns",
  "architecture": {{
    "framework": "The framework to use (e.g. Next.js 15, FastAPI, Go + Chi, Rails 7, etc.)",
    "database": "The database to use (e.g. SQLite, PostgreSQL, etc.)",
    "auth": {auth_bool},
    "auth_reason": "Why auth is or isn't needed",
    "description": "One-line architecture summary for hero tagline"
  }},
  "entities": [
    {{
      "name": "EntityName",
      "description": "What this entity represents and how users interact with it",
      "fields": [
        {{"name": "id", "field_type": "TEXT", "required": true, "description": "Primary key"}},
        {{"name": "field_name", "field_type": "TEXT|INTEGER|REAL|BLOB", "required": true, "description": "What this stores"}}
      ],
      "relationships": ["BelongsTo:OtherEntity", "HasMany:ChildEntity"]
    }}
  ],
  "agents": [
    {{
      "name": "AgentName",
      "type": "chatbot|analyzer|assistant",
      "description": "What the agent does for the user",
      "system_prompt": "Full system prompt the agent uses",
      "trigger": "floating_button|sidebar_panel|inline"
    }}
  ],
  "pages": [
    {{
      "name": "PageName",
      "route": "/route",
      "description": "What the user sees and does on this page — be specific about data displayed and actions available",
      "components": ["Component1", "Component2"]
    }}
  ],
  "integrations": []
}}"#,
        description = description,
        app_type = intent.app_type,
        auth_hint = intent_auth_hint,
        db_hint = intent_db_hint,
        payment_hint = intent_payment_hint,
        ui_style = intent.ui_style,
        complexity = intent.complexity,
        agents_hint = agents_hint,
        entities_hint = entities_hint,
        pages_hint = pages_hint,
        tech_hint = tech_hint,
        decision_ctx = decision_context,
        knowledge_block = if knowledge_context.is_empty() {
            String::new()
        } else {
            format!("## Existing Knowledge\n{}", knowledge_context)
        },
        auth_bool = intent.needs_auth,
    );

    let plan_result =
        super::chat::call_llm_simple_for_project(&app, &prompt, Some(&project_id)).await;

    let plan_data = match plan_result {
        Ok(text) => {
            let cleaned = text
                .trim()
                .trim_start_matches("```json")
                .trim_end_matches("```")
                .trim();
            serde_json::from_str::<serde_json::Value>(cleaned)
                .unwrap_or_else(|_| json!({"summary": text}))
        }
        Err(e) => {
            let _ = tx
                .send(IntentEvent::Error {
                    step: "generate_plan".into(),
                    message: e.to_string(),
                })
                .await;
            return;
        }
    };

    let _ = tx
        .send(IntentEvent::Step {
            step: "generate_plan".into(),
            status: "completed".into(),
            detail: plan_data["summary"]
                .as_str()
                .unwrap_or("Plan created")
                .to_string(),
        })
        .await;
    let _ = tx
        .send(IntentEvent::Progress {
            percent: 30,
            message: "Plan ready, generating code...".into(),
        })
        .await;

    // Step 3: Store plan in knowledge base
    {
        let db = app.db.lock().await;
        let ks = nexus_store::KnowledgeService::new(&db);
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

    // Step 4: Generate code (approve plan -> codegen)
    let _ = tx
        .send(IntentEvent::Step {
            step: "generate_code".into(),
            status: "running".into(),
            detail: "Generating application code...".into(),
        })
        .await;

    // Build IR from plan -- use intent's auth decision as authoritative
    let needs_auth = intent.needs_auth
        || plan_data
            .get("architecture")
            .and_then(|a| a.get("auth"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

    let entities: Vec<serde_json::Value> = plan_data["entities"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|e| {
            let fields: Vec<serde_json::Value> = e["fields"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .map(|f| {
                    json!({
                        "name": f["name"],
                        "type": f["field_type"].as_str().unwrap_or("TEXT"),
                        "primary_key": f["name"].as_str() == Some("id"),
                        "not_null": f["required"].as_bool().unwrap_or(false)
                    })
                })
                .collect();
            json!({
                "name": e["name"],
                "description": e["description"],
                "fields": fields,
                "materialize": true
            })
        })
        .collect();

    let planned_framework = plan_data["architecture"]["framework"]
        .as_str()
        .unwrap_or("Next.js 15");
    let planned_database = plan_data["architecture"]["database"]
        .as_str()
        .unwrap_or("SQLite");

    let ir = json!({
        "meta": {"version": "1.0", "project_id": project_id},
        "architecture": {"auth": needs_auth, "framework": planned_framework, "database": planned_database},
        "entities": entities,
        "agents": plan_data.get("agents").cloned().unwrap_or(json!([])),
    });

    let output_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");
    let data_db = app.project_data_db(&project_id);

    // Create plan (needs DB lock)
    let plan_or_err = {
        let db = app.db.lock().await;
        let materializer = nexus_store::CodeGenMaterializer::new(&db);
        materializer.plan(&project_id, &ir)
    };
    let codegen_plan = match plan_or_err {
        Ok(p) => p,
        Err(e) => {
            let _ = tx
                .send(IntentEvent::Error {
                    step: "generate_code".into(),
                    message: e.to_string(),
                })
                .await;
            return;
        }
    };

    let _ = tx
        .send(IntentEvent::Progress {
            percent: 50,
            message: "Building with AI...".into(),
        })
        .await;

    // Build agent templates from intent for the enhanced prompt
    let agent_templates: Vec<(String, String, String, String, String)> = intent
        .suggested_agents
        .iter()
        .map(|a| {
            (
                a.name.clone(),
                a.agent_type.clone(),
                a.api_route.clone(),
                a.system_prompt.clone(),
                a.trigger.clone(),
            )
        })
        .collect();

    // Serialize intent context for the prompt
    let intent_context = format!(
        "App type: {:?}, UI style: {:?}, Complexity: {:?}, Pages: {}, Entities: {}",
        intent.app_type,
        intent.ui_style,
        intent.complexity,
        intent.suggested_pages.join(", "),
        intent.suggested_entities.join(", "),
    );

    // Detect the tech stack from the user's raw description so the prompt stays language-agnostic.
    let detected_stack = nexus_store::detect_tech_stack(&description);

    // Only generate CSS/font tokens for web-based stacks — they're meaningless for Python/Go/Rust/etc.
    let (app_globals_css, app_font_link) = if nexus_store::is_web_stack(&detected_stack) {
        (
            crate::design_system::generate_globals_css(&intent.ui_style, &project_name),
            crate::design_system::font_imports(&intent.ui_style).to_string(),
        )
    } else {
        (String::new(), String::new())
    };

    let app_ctx = nexus_store::AppContext {
        app_name: project_name.clone(),
        ui_style: format!("{:?}", intent.ui_style),
        app_type: format!("{:?}", intent.app_type),
        tech_stack: detected_stack,
        globals_css: app_globals_css,
        font_link: app_font_link,
        suggested_pages: intent.suggested_pages.clone(),
        tagline: plan_data["architecture"]["description"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(120)
            .collect(),
    };

    // Build a rich summary that feeds the code generator maximum context.
    // The plan_data summary alone is often too brief — augment it with pages and architecture.
    let base_summary = plan_data["summary"]
        .as_str()
        .unwrap_or("A full-stack application");
    let pages_summary = plan_data["pages"]
        .as_array()
        .map(|pages| {
            pages
                .iter()
                .filter_map(|p| {
                    let name = p["name"].as_str()?;
                    let route = p["route"].as_str().unwrap_or("");
                    let desc = p["description"].as_str().unwrap_or("");
                    Some(format!("- **{}** (`{}`): {}", name, route, desc))
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    let arch_summary = format!(
        "Framework: {}. Database: {}.",
        plan_data["architecture"]["framework"]
            .as_str()
            .unwrap_or("auto"),
        plan_data["architecture"]["database"]
            .as_str()
            .unwrap_or("auto"),
    );
    let summary = if pages_summary.is_empty() {
        format!("{}\n\n{}", base_summary, arch_summary)
    } else {
        format!(
            "{}\n\n{}\n\n### Pages to generate:\n{}",
            base_summary, arch_summary, pages_summary
        )
    };
    let gen_prompt = if agent_templates.is_empty() {
        nexus_store::build_generation_prompt(&codegen_plan, &summary, Some(&app_ctx))
    } else {
        nexus_store::build_enhanced_generation_prompt(
            &codegen_plan,
            &summary,
            &agent_templates,
            &intent_context,
            Some(&app_ctx),
        )
    };

    let generated_files =
        super::llm_codegen::generate_app_files(&app, &gen_prompt, Some(&project_id)).await;

    let codegen_result = match generated_files {
        Ok(files) => {
            let db = app.db.lock().await;
            let m = nexus_store::CodeGenMaterializer::new(&db);
            m.generate_from_llm_output(&project_id, &output_dir, &codegen_plan, &data_db, &files)
        }
        Err(e) => {
            let _ = tx
                .send(IntentEvent::Error {
                    step: "generate_code".into(),
                    message: e.clone(),
                })
                .await;
            return;
        }
    };

    let files_count = match &codegen_result {
        Ok(r) => {
            let _ = tx
                .send(IntentEvent::Step {
                    step: "generate_code".into(),
                    status: "completed".into(),
                    detail: format!("{} files generated", r.files_written.len()),
                })
                .await;
            r.files_written.len()
        }
        Err(e) => {
            let _ = tx
                .send(IntentEvent::Error {
                    step: "generate_code".into(),
                    message: e.to_string(),
                })
                .await;
            return;
        }
    };

    // Plugin hook: OnAfterGeneration (CLAUDE.md invariant — fired in BOTH oneshot AND pipeline paths)
    let mut gen_hook_ctx = plugin_hooks::build_context_for_tenant(
        "intent_to_app",
        None,
        HookPoint::OnAfterGeneration,
        Some(&description),
    );
    gen_hook_ctx.intent = serde_json::to_value(&intent).ok();
    let _ = plugin_hooks::fire_hook(&app, HookPoint::OnAfterGeneration, &mut gen_hook_ctx).await;

    let _ = tx
        .send(IntentEvent::Progress {
            percent: 80,
            message: "Code generated, starting app...".into(),
        })
        .await;

    // Step 5: Update project phase
    {
        let db = app.db.lock().await;
        let svc = ProjectService::new(&db);
        let _ = svc.update_project_phase(&project_id, 3);
    }

    // Step 6: Start the app
    let _ = tx
        .send(IntentEvent::Step {
            step: "start_app".into(),
            status: "running".into(),
            detail: "Installing dependencies and starting...".into(),
        })
        .await;

    // Create app instance
    let app_port = {
        let db = app.db.lock().await;
        let svc = nexus_store::AppRunnerService::new(&db);
        let port = svc.next_available_port().unwrap_or(4100);
        let _ = svc.create_instance(
            &project_id,
            port,
            output_dir.to_str().unwrap_or_default(),
            false,
        );
        port
    };

    // Run npm install + start (simplified -- just trigger, don't wait)
    let output_dir_clone = output_dir.clone();
    let port = app_port;
    let app_clone = app.clone();
    let project_id_clone = project_id.clone();
    tokio::spawn(async move {
        // npm install
        let install_result = tokio::task::spawn_blocking({
            let dir = output_dir_clone.clone();
            move || nexus_store::app_runner::npm_install(&dir)
        })
        .await;

        if let Ok(Ok(_)) = install_result {
            // Start dev server
            let spawn_result = tokio::task::spawn_blocking({
                let dir = output_dir_clone;
                move || nexus_store::app_runner::spawn_dev_server(&dir, port)
            })
            .await;

            if let Ok(Ok(pid)) = spawn_result {
                let db = app_clone.db.lock().await;
                let svc = nexus_store::AppRunnerService::new(&db);
                let instances = svc.list_instances(&project_id_clone).unwrap_or_default();
                if let Some(inst) = instances.first() {
                    let _ = svc.update_status(&inst.id, "running", Some(pid), None);
                }
            }
        }
    });

    let _ = tx
        .send(IntentEvent::Step {
            step: "start_app".into(),
            status: "completed".into(),
            detail: format!("App starting on port {}", app_port),
        })
        .await;
    let _ = tx
        .send(IntentEvent::Progress {
            percent: 100,
            message: "Done!".into(),
        })
        .await;

    // Final result
    let _ = tx
        .send(IntentEvent::Result {
            project_id: project_id.clone(),
            project_name,
            files_count,
            app_url: Some(format!("http://localhost:{}", app_port)),
        })
        .await;

    info!(
        project_id = %project_id,
        files = files_count,
        port = app_port,
        agents = intent.suggested_agents.len(),
        "Intent->App pipeline completed"
    );
}

/// POST /pipeline/run -- the deterministic execution pipeline.
///
/// **Deprecated**: Use `POST /oneshot` instead. This endpoint is maintained
/// for backwards compatibility and will be removed in a future release.
pub async fn run_pipeline(
    State(app): State<Arc<AppState>>,
    Json(body): Json<IntentReq>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    tracing::warn!("POST /pipeline/run is deprecated — use POST /oneshot instead");
    let description = body.description.clone();

    // Step 1: Analyze intent (deterministic, instant)
    let intent = intent_engine::analyze_flat(&description);

    // Plugin hook: OnIntentParsed — CLAUDE.md invariant requires plugin hooks in BOTH
    // the oneshot AND pipeline paths so plugins can intercept on either entry point.
    let mut intent_hook_ctx = plugin_hooks::build_context_for_tenant(
        "pipeline_run",
        None,
        HookPoint::OnIntentParsed,
        Some(&description),
    );
    intent_hook_ctx.intent = serde_json::to_value(&intent).ok();
    let _ = plugin_hooks::fire_hook(&app, HookPoint::OnIntentParsed, &mut intent_hook_ctx).await;

    // Step 2: Create project
    let project_name = extract_project_name(&description);
    let project_id = {
        let db = app.db.lock().await;
        let svc = ProjectService::new(&db);
        svc.create_project(&project_name, Some(&description), "default")
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .id
    };

    let project_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");

    let (tx, mut rx) = tokio::sync::mpsc::channel::<PipelineEvent>(100);

    // Spawn pipeline; fire OnAfterGeneration once the pipeline returns so plugins
    // see both endpoints' completion lifecycle.
    let app_bg = app.clone();
    let intent_for_hook = intent.clone();
    let description_for_hook = description.clone();
    tokio::spawn(async move {
        let mut pipeline = ExecutionPipeline::new(
            app_bg.clone(),
            tx,
            project_id,
            project_dir,
            intent,
            &description,
        );
        #[allow(deprecated)]
        let _ = pipeline.execute(&description).await;

        let mut gen_hook_ctx = plugin_hooks::build_context_for_tenant(
            "pipeline_run",
            None,
            HookPoint::OnAfterGeneration,
            Some(&description_for_hook),
        );
        gen_hook_ctx.intent = serde_json::to_value(&intent_for_hook).ok();
        let _ =
            plugin_hooks::fire_hook(&app_bg, HookPoint::OnAfterGeneration, &mut gen_hook_ctx).await;
    });

    let stream = async_stream::stream! {
        while let Some(event) = rx.recv().await {
            let json_str = serde_json::to_string(&event).unwrap_or_default();
            yield Ok::<_, Infallible>(Event::default().data(json_str));
        }
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    ))
}

fn extract_project_name(description: &str) -> String {
    // Take first 4 meaningful words
    let words: Vec<&str> = description
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .take(4)
        .collect();
    if words.is_empty() {
        "New App".to_string()
    } else {
        words.join(" ")
    }
}
