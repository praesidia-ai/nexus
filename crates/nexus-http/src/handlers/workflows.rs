//! Workflow run handlers.
//!
//! Triggers named pipelines (app-build, code-review, security-audit, feature, deploy, agent-deploy).
//! Runs are persisted to workflow_runs table. Each pipeline step calls the OpenAI API
//! directly (no nexus-workflow dependency) with context interpolation.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use nexus_store::{AgentBuilder, AgentDefinition, MetricsService, WorkflowRunService};
use nexus_workflow::definition::WorkflowDefinition;

use crate::{
    error::{ApiError, ApiResult},
    security::auth::AuthContext,
    security::tenant::validate_project_access,
    state::AppState,
};

const KNOWN_PIPELINES: &[&str] = &[
    "app-build",
    "code-review",
    "security-audit",
    "feature",
    "deploy",
    "agent-deploy",
    "custom",
];

/// A custom workflow step sent by the frontend canvas.
#[derive(Deserialize)]
struct CustomStep {
    agent: String,
    _role: String,
    prompt: String,
}

// ---------------------------------------------------------------------------
// Pipeline step definitions
// ---------------------------------------------------------------------------

/// Returns (step_name, prompt_template) pairs for a known pipeline.
fn get_pipeline_steps(pipeline: &str) -> Vec<(&'static str, &'static str)> {
    match pipeline {
        "app-build" => vec![
            (
                "research",
                "Research the following requirements thoroughly. \
                 Look for similar solutions, best practices, and potential pitfalls.\n\n\
                 Requirements:\n{requirements}\nStack hint: {stack}\n\n\
                 Produce a concise research summary with key findings and recommendations.",
            ),
            (
                "product_spec",
                "Based on the requirements and research below, write a clear \
                 product specification with user stories, acceptance criteria, and a \
                 prioritised feature list.\n\n\
                 Requirements:\n{requirements}\n\nResearch:\n{research}",
            ),
            (
                "architecture",
                "Design a production-ready system architecture for this project. \
                 Include: component diagram, data model, API surface, deployment topology, \
                 and technology choices with justification.\n\n\
                 Product spec:\n{product_spec}\nStack hint: {stack}",
            ),
            (
                "implement",
                "Implement the application according to the spec and architecture. \
                 Write production-quality code with full project structure, core features, \
                 unit and integration tests, and a README.\n\n\
                 Authentication guidance: include auth ONLY if the architecture explicitly \
                 specifies auth:true (multi-user portal, SaaS, admin panel). \
                 For all other app types (landing pages, tools, single-operator apps) do NOT \
                 add login/register pages, sessions, or auth middleware — the app must work \
                 immediately without accounts.\n\n\
                 Product spec:\n{product_spec}\n\nArchitecture:\n{architecture}",
            ),
            (
                "security_audit",
                "Perform a thorough security audit of the implementation below. \
                 Check for OWASP Top 10, injection vulnerabilities, auth issues, \
                 secrets in code, and insecure defaults. \
                 Provide a prioritised list of findings with remediation steps.\n\n\
                 Implementation:\n{implement}",
            ),
            (
                "documentation",
                "Write comprehensive documentation for this project: \
                 Developer README, API reference, and user guide.\n\n\
                 Implementation:\n{implement}\n\nArchitecture:\n{architecture}",
            ),
            (
                "deploy",
                "Create a production-ready deployment configuration: \
                 Dockerfile, docker-compose.yml, GitHub Actions CI/CD, \
                 environment variable docs, and health-check configuration.\n\n\
                 Implementation:\n{implement}\n\nArchitecture:\n{architecture}\n\n\
                 Security findings:\n{security_audit}",
            ),
        ],
        "code-review" => vec![
            (
                "security_review",
                "Review the following code for security issues. \
                 Check: injection, auth, crypto, secrets, input validation, \
                 dependency vulnerabilities, and OWASP Top 10.\n\n\
                 Language: {language}\nCode:\n{code}",
            ),
            (
                "quality_review",
                "Review the following code for quality, correctness, and maintainability. \
                 Check: naming, complexity, test coverage, error handling, \
                 performance, idiomatic patterns, and documentation.\n\n\
                 Language: {language}\nCode:\n{code}",
            ),
            (
                "final_report",
                "Consolidate the following two reviews into a clear, actionable code-review report. \
                 Structure: Executive Summary, Critical Issues, Recommendations, Positive Highlights.\n\n\
                 Security review:\n{security_review}\n\nQuality review:\n{quality_review}",
            ),
        ],
        "security-audit" => vec![
            (
                "threat_model",
                "Perform STRIDE threat modelling for the following target. \
                 Identify assets, trust boundaries, data flows, and threats.\n\n\
                 Target:\n{target}",
            ),
            (
                "vulnerability_scan",
                "Analyse the target for concrete vulnerabilities based on the threat model. \
                 Focus on exploitable weaknesses, CVEs in dependencies, and misconfigurations.\n\n\
                 Target:\n{target}\n\nThreat model:\n{threat_model}",
            ),
            (
                "remediation_plan",
                "Create a prioritised remediation roadmap based on the vulnerability analysis. \
                 For each finding: severity, impact, effort, and concrete fix steps.\n\n\
                 Findings:\n{vulnerability_scan}",
            ),
            (
                "audit_report",
                "Write a professional security audit report. \
                 Format: Executive Summary, Scope, Threat Model, Findings (by severity), \
                 Remediation Plan, Conclusion.\n\n\
                 Threat model:\n{threat_model}\n\nVulnerabilities:\n{vulnerability_scan}\n\n\
                 Remediation:\n{remediation_plan}",
            ),
        ],
        "feature" => vec![
            (
                "design",
                "Design the following feature for the existing codebase. \
                 Produce: acceptance criteria, API changes, data model changes, \
                 and a short implementation plan.\n\n\
                 Feature:\n{feature}\n\nCodebase:\n{codebase}",
            ),
            (
                "implementation",
                "Implement the feature according to the design. \
                 Write the code, tests, and any migrations needed.\n\n\
                 Design:\n{design}\n\nCodebase:\n{codebase}",
            ),
            (
                "review",
                "Review the implementation for security and quality issues.\n\n\
                 Implementation:\n{implementation}",
            ),
            (
                "docs_update",
                "Update the documentation to cover the new feature.\n\n\
                 Feature design:\n{design}\n\nImplementation:\n{implementation}",
            ),
        ],
        "deploy" => vec![
            (
                "infra",
                "Design the cloud infrastructure for deploying this application. \
                 Include: compute, networking, database, storage, and cost estimate.\n\n\
                 Application:\n{implementation}\nTarget: {target_env}",
            ),
            (
                "cicd",
                "Write a complete CI/CD pipeline configuration. \
                 Include: build, test, security scan, Docker build, and deploy stages.\n\n\
                 Application:\n{implementation}\nInfrastructure:\n{infra}",
            ),
            (
                "monitoring",
                "Configure monitoring, logging, and alerting for the deployment. \
                 Include: health checks, log aggregation, metrics, and on-call runbooks.\n\n\
                 Infrastructure:\n{infra}",
            ),
        ],
        "agent-deploy" => vec![
            (
                "validate",
                "Review the following agent definition and check for issues: {agent_config}",
            ),
            (
                "generate_card",
                "Generate a production agent card JSON for deploying this agent: {validate}",
            ),
            (
                "deploy_config",
                "Generate deployment configuration (Docker, health checks, monitoring) \
                 for this agent: {generate_card}",
            ),
        ],
        _ => vec![],
    }
}

// ---------------------------------------------------------------------------
// Pipeline executor (runs in a spawned task)
// ---------------------------------------------------------------------------

async fn execute_pipeline(
    pipeline: String,
    context: Option<serde_json::Value>,
    run_id: String,
    project_id: String,
    app: Arc<AppState>,
) {
    // Load customer agents for this project (name → definition, case-insensitive).
    let project_agents: HashMap<String, AgentDefinition> = {
        let db = app.db.lock().await;
        let ab = AgentBuilder::new(&db);
        ab.list_agents(&project_id)
            .unwrap_or_default()
            .into_iter()
            .map(|a| (a.name.to_lowercase(), a))
            .collect()
    };

    // Build the step list.
    // Priority: custom steps from context.steps (pipeline=="custom" or present)
    //           → built-in pipeline definition.
    let builtin_steps = get_pipeline_steps(&pipeline);

    // (step_id, prompt_template, resolved_agent)
    let steps: Vec<(String, String, Option<AgentDefinition>)> =
        if let Some(custom_steps) = context
            .as_ref()
            .and_then(|c| c.get("steps"))
            .and_then(|s| serde_json::from_value::<Vec<CustomStep>>(s.clone()).ok())
        {
            custom_steps
                .into_iter()
                .map(|s| {
                    let agent = project_agents.get(&s.agent.to_lowercase()).cloned();
                    (s.agent.clone(), s.prompt, agent)
                })
                .collect()
        } else {
            builtin_steps
                .iter()
                .map(|(name, tmpl)| {
                    // Try to find a customer agent whose name matches the step name.
                    let agent = project_agents.get(&name.to_lowercase()).cloned();
                    (name.to_string(), tmpl.to_string(), agent)
                })
                .collect()
        };

    let mut vars: HashMap<String, String> = HashMap::new();

    // Load context variables (skip the "steps" key — that's structural, not a template var).
    if let Some(ref ctx) = context {
        if let Some(obj) = ctx.as_object() {
            for (k, v) in obj {
                if k != "steps" {
                    vars.insert(
                        k.clone(),
                        v.as_str().unwrap_or(&v.to_string()).to_string(),
                    );
                }
            }
        }
    }

    let mut step_outputs: Vec<serde_json::Value> = Vec::new();
    let start = std::time::Instant::now();
    let total_steps = steps.len();

    for (step_name, prompt_template, agent) in &steps {
        let step_start = std::time::Instant::now();

        // Interpolate accumulated vars into the prompt template.
        let mut prompt = prompt_template.clone();
        for (k, v) in &vars {
            prompt = prompt.replace(&format!("{{{}}}", k), v);
        }

        let agent_label = agent
            .as_ref()
            .map(|a| a.name.as_str())
            .unwrap_or(step_name.as_str());

        info!(
            run_id = %run_id,
            step = %step_name,
            agent = %agent_label,
            has_system_prompt = agent.as_ref().map(|a| !a.system_prompt.is_empty()).unwrap_or(false),
            "Running pipeline step"
        );

        match super::chat::call_llm_for_agent(&app, agent.as_ref(), &prompt).await {
            Ok(output) => {
                let step_duration_ms = step_start.elapsed().as_millis() as i64;

                // Store output so subsequent steps can reference it
                vars.insert(step_name.to_string(), output.clone());
                step_outputs.push(serde_json::json!({
                    "step": step_name,
                    "status": "completed",
                    "duration_ms": step_duration_ms,
                    "output_length": output.len()
                }));

                // Record step metric
                let db = app.db.lock().await;
                let ms = MetricsService::new(&db);
                let _ = ms.record_step_metric(
                    &run_id,
                    step_name,
                    agent_label,
                    step_duration_ms,
                    0,     // retries
                    false, // not skipped
                    output.len() as i64,
                    "success",
                    None,
                );

                info!(
                    run_id = %run_id,
                    step = %step_name,
                    duration_ms = step_duration_ms,
                    output_len = output.len(),
                    "Pipeline step completed"
                );
            }
            Err(e) => {
                let step_duration_ms = step_start.elapsed().as_millis() as i64;
                warn!(
                    run_id = %run_id,
                    step = %step_name,
                    error = %e,
                    "Pipeline step failed"
                );

                // Record failed step metric
                {
                    let db = app.db.lock().await;
                    let ms = MetricsService::new(&db);
                    let _ = ms.record_step_metric(
                        &run_id,
                        step_name,
                        agent_label,
                        step_duration_ms,
                        0,
                        false,
                        0,
                        "failed",
                        Some(&e.to_string()),
                    );
                }

                // Fail the run
                let db = app.db.lock().await;
                let svc = WorkflowRunService::new(&db);
                let _ = svc.fail_run(&run_id, &e.to_string());

                // Record partial run metric
                let ms = MetricsService::new(&db);
                let steps_succeeded = step_outputs.len() as i32;
                let _ = ms.record_run_metric(
                    Some(&project_id),
                    &pipeline,
                    start.elapsed().as_millis() as i64,
                    steps.len() as i32,
                    steps_succeeded,
                    1, // 1 failed
                    0,
                    0,
                    "failed",
                );
                return;
            }
        }
    }

    // All steps succeeded — complete the run
    let total_duration_ms = start.elapsed().as_millis();
    let output = serde_json::json!({
        "pipeline": pipeline,
        "steps": step_outputs,
        "final_output": vars.get(
                steps.last().map(|(name, _, _)| name.as_str()).unwrap_or("")
            )
            .cloned()
            .unwrap_or_default(),
        "duration_ms": total_duration_ms
    });

    let db = app.db.lock().await;
    let svc = WorkflowRunService::new(&db);
    let _ = svc.complete_run(&run_id, &output.to_string());

    // Record run metric
    let ms = MetricsService::new(&db);
    let _ = ms.record_run_metric(
        Some(&project_id),
        &pipeline,
        total_duration_ms as i64,
        total_steps as i32,
        total_steps as i32, // all succeeded
        0,
        0,
        0,
        "success",
    );

    info!(
        run_id = %run_id,
        pipeline = %pipeline,
        duration_ms = total_duration_ms,
        steps = total_steps,
        "Pipeline completed successfully"
    );
}

// ---------------------------------------------------------------------------
// GET /projects/:id/workflows
// ---------------------------------------------------------------------------

pub async fn list_runs(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let svc = WorkflowRunService::new(&db);
    let runs = svc.list_runs(&project_id)?;
    Ok(Json(serde_json::to_value(runs)?))
}

// ---------------------------------------------------------------------------
// GET /projects/:id/workflows/:run_id
// ---------------------------------------------------------------------------

pub async fn get_run(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, run_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let run_svc = WorkflowRunService::new(&db);
    let run = run_svc
        .get_run(&run_id)?
        .ok_or_else(|| ApiError::NotFound(format!("workflow run {} not found", run_id)))?;
    let metrics_svc = MetricsService::new(&db);
    let steps = metrics_svc.list_step_metrics(&run_id).unwrap_or_default();
    Ok(Json(serde_json::json!({
        "run": run,
        "steps": steps,
    })))
}

// ---------------------------------------------------------------------------
// GET /projects/:id/workflows/:run_id/steps
// ---------------------------------------------------------------------------

pub async fn list_run_steps(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, run_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let run_svc = WorkflowRunService::new(&db);
    let _ = run_svc
        .get_run(&run_id)?
        .ok_or_else(|| ApiError::NotFound(format!("workflow run {} not found", run_id)))?;
    let metrics_svc = MetricsService::new(&db);
    let steps = metrics_svc.list_step_metrics(&run_id).unwrap_or_default();
    Ok(Json(serde_json::to_value(steps)?))
}

// ---------------------------------------------------------------------------
// POST /projects/:id/workflows/:run_id/cancel
// ---------------------------------------------------------------------------

pub async fn cancel_run(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, run_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let svc = WorkflowRunService::new(&db);
    let run = svc
        .get_run(&run_id)?
        .ok_or_else(|| ApiError::NotFound(format!("workflow run {} not found", run_id)))?;
    if run.status != "running" {
        return Err(ApiError::BadRequest(format!(
            "Cannot cancel run in status '{}'",
            run.status
        )));
    }
    svc.fail_run(&run_id, "cancelled by user")?;
    Ok(Json(serde_json::json!({ "status": "cancelled", "run_id": run_id })))
}

// ---------------------------------------------------------------------------
// POST /projects/:id/workflows/run
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RunWorkflowReq {
    /// One of the known pipeline names (e.g. "app-build").
    pub pipeline: String,
    /// Arbitrary context JSON passed to the pipeline.
    pub context: Option<serde_json::Value>,
}

pub async fn run_workflow(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
    Json(body): Json<RunWorkflowReq>,
) -> ApiResult<Json<serde_json::Value>> {
    // Validate tenant access before any work
    {
        let db = app.db.lock().await;
        validate_project_access(&db, &project_id, &auth.tenant_id)
            .map_err(ApiError::Forbidden)?;
    }

    if !KNOWN_PIPELINES.contains(&body.pipeline.as_str()) {
        return Err(ApiError::BadRequest(format!(
            "Unknown pipeline '{}'. Available: {}",
            body.pipeline,
            KNOWN_PIPELINES.join(", ")
        )));
    }

    // Validate that at least one LLM provider has an API key configured
    {
        let (_, _, resolved_key) = super::chat::resolve_llm_config(&app, None, None).await;
        if resolved_key.is_empty() || resolved_key == "sk-placeholder" {
            return Err(ApiError::BadRequest(
                "No LLM API key configured. Go to Settings (/settings) to add an API key for any provider."
                    .to_string(),
            ));
        }
    }

    let input_json = body
        .context
        .as_ref()
        .map(|v| v.to_string());

    let run = {
        let db = app.db.lock().await;
        let svc = WorkflowRunService::new(&db);
        svc.create_run(Some(&project_id), &body.pipeline, input_json.as_deref())?
    };

    info!(
        project = %project_id,
        pipeline = %body.pipeline,
        run_id = %run.id,
        "Workflow run started"
    );

    // Spawn async pipeline execution
    let run_id = run.id.clone();
    let pipeline = body.pipeline.clone();
    let context = body.context.clone();
    let app_clone = app.clone();
    let pid = project_id.clone();

    tokio::spawn(async move {
        execute_pipeline(pipeline, context, run_id, pid, app_clone).await;
    });

    Ok(Json(serde_json::json!({
        "run_id": run.id,
        "pipeline": run.pipeline,
        "status": run.status,
        "started_at": run.started_at,
        "available_pipelines": KNOWN_PIPELINES
    })))
}

// ===========================================================================
// Persistent workflow engine handlers (nexus-workflow)
// ===========================================================================

/// Request body for `POST /workflows/start`.
#[derive(Debug, Deserialize)]
pub struct StartWorkflowRequest {
    /// The workflow definition to execute.
    pub definition: WorkflowDefinition,
    /// Initial context values passed to the workflow.
    #[serde(default)]
    pub context: serde_json::Value,
}

/// Response for persistent workflow status endpoints.
#[derive(Debug, Serialize)]
pub struct WorkflowResponse {
    pub id: String,
    pub definition_id: String,
    pub status: String,
    pub step_states: serde_json::Value,
    pub context: serde_json::Value,
    pub started_at: String,
    pub updated_at: String,
}

impl From<nexus_workflow::engine::WorkflowInstance> for WorkflowResponse {
    fn from(inst: nexus_workflow::engine::WorkflowInstance) -> Self {
        Self {
            id: inst.id,
            definition_id: inst.definition_id,
            status: inst.status.as_str().to_string(),
            step_states: serde_json::to_value(&inst.step_states).unwrap_or_default(),
            context: inst.context,
            started_at: inst.started_at,
            updated_at: inst.updated_at,
        }
    }
}

/// Convert `WorkflowError` to `ApiError`.
fn map_wf_err(e: nexus_workflow::WorkflowError) -> ApiError {
    match e {
        nexus_workflow::WorkflowError::NotFound(m) => ApiError::NotFound(m),
        nexus_workflow::WorkflowError::AlreadyRunning(m) => ApiError::BadRequest(m),
        nexus_workflow::WorkflowError::Cancelled(m) => ApiError::BadRequest(m),
        nexus_workflow::WorkflowError::StepFailed(m) => ApiError::BadRequest(m),
        nexus_workflow::WorkflowError::Timeout(m) => ApiError::Internal(m),
        nexus_workflow::WorkflowError::Storage(m) => ApiError::Internal(m),
        nexus_workflow::WorkflowError::CycleDetected(m) => ApiError::BadRequest(m),
        nexus_workflow::WorkflowError::MaxIterationsExceeded(m) => ApiError::BadRequest(m),
        nexus_workflow::WorkflowError::ValidationError(m) => ApiError::BadRequest(m),
    }
}

/// Start a new persistent workflow instance.
///
/// `POST /workflows/start`
pub async fn start_persistent_workflow(
    State(state): State<Arc<AppState>>,
    Json(req): Json<StartWorkflowRequest>,
) -> ApiResult<(StatusCode, Json<WorkflowResponse>)> {
    let engine = get_workflow_engine(&state)?;
    let instance = engine
        .start(&req.definition, req.context)
        .await
        .map_err(map_wf_err)?;

    info!(instance_id = %instance.id, "Persistent workflow started via API");
    Ok((StatusCode::CREATED, Json(WorkflowResponse::from(instance))))
}

/// Get the status of a persistent workflow instance.
///
/// `GET /workflows/:id`
pub async fn get_persistent_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<WorkflowResponse>> {
    let engine = get_workflow_engine(&state)?;
    let instance = engine.status(&id).map_err(map_wf_err)?;
    Ok(Json(WorkflowResponse::from(instance)))
}

/// Resume a paused or crashed persistent workflow.
///
/// `POST /workflows/:id/resume`
pub async fn resume_persistent_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<WorkflowResponse>> {
    let engine = get_workflow_engine(&state)?;
    let instance = engine.resume(&id).await.map_err(map_wf_err)?;
    info!(instance_id = %id, "Persistent workflow resumed via API");
    Ok(Json(WorkflowResponse::from(instance)))
}

/// Path parameters for the approve endpoint.
#[derive(Debug, Deserialize)]
pub struct ApproveParams {
    pub id: String,
    pub step: String,
}

/// Approve a human-approval step in a persistent workflow.
///
/// `POST /workflows/:id/approve/:step`
pub async fn approve_persistent_step(
    State(state): State<Arc<AppState>>,
    Path(params): Path<ApproveParams>,
) -> ApiResult<Json<serde_json::Value>> {
    let engine = get_workflow_engine(&state)?;
    engine
        .approve(&params.id, &params.step)
        .await
        .map_err(map_wf_err)?;
    info!(
        instance_id = %params.id,
        step_id = %params.step,
        "Persistent workflow step approved via API"
    );
    Ok(Json(serde_json::json!({"approved": true})))
}

/// Cancel a running persistent workflow.
///
/// `POST /workflows/:id/cancel`
pub async fn cancel_persistent_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let engine = get_workflow_engine(&state)?;
    engine.cancel(&id).await.map_err(map_wf_err)?;
    info!(instance_id = %id, "Persistent workflow cancelled via API");
    Ok(Json(serde_json::json!({"cancelled": true})))
}

/// List all persistent workflow instances.
///
/// `GET /workflows`
pub async fn list_persistent_workflows(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<Vec<WorkflowResponse>>> {
    let engine = get_workflow_engine(&state)?;
    let instances = engine.list().map_err(map_wf_err)?;
    let responses: Vec<WorkflowResponse> = instances.into_iter().map(WorkflowResponse::from).collect();
    Ok(Json(responses))
}

/// Helper: get or create the persistent workflow engine from app state.
fn get_workflow_engine(state: &AppState) -> ApiResult<nexus_workflow::WorkflowEngine> {
    let wf_dir = state.data_dir.join("workflows");
    nexus_workflow::WorkflowEngine::new(&wf_dir).map_err(|e| ApiError::Internal(e.to_string()))
}
