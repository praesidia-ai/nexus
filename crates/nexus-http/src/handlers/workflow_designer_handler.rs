//! Smart Workflow Designer — LLM-powered idea-to-agents decomposition.
//!
//! Takes a natural language idea and uses the LLM to design a complete
//! multi-agent workflow: agents, their roles, connections, tools, and
//! system prompts. Users can then review, tweak, and materialize.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::info;

use crate::{
    error::{ApiError, ApiResult},
    llm_client::{call_llm_with_tools, LlmConfig},
    security::auth::AuthContext,
    security::tenant::validate_project_access,
    state::AppState,
};
use nexus_store::AgentBuilder;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct DesignWorkflowRequest {
    pub idea: String,
    #[serde(default)]
    pub complexity: Option<String>,
    #[serde(default)]
    pub industry: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignedWorkflowAgent {
    pub temp_id: String,
    pub name: String,
    pub role: String,
    pub description: String,
    pub system_prompt: String,
    pub tools: Vec<String>,
    pub model_suggestion: String,
    pub trigger: String,
    pub icon: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConnection {
    pub from_agent: String,
    pub to_agent: String,
    pub condition: String,
    pub data_passed: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignedWorkflow {
    pub title: String,
    pub description: String,
    pub agents: Vec<DesignedWorkflowAgent>,
    pub connections: Vec<WorkflowConnection>,
    pub execution_mode: String,
    pub estimated_complexity: String,
    pub tags: Vec<String>,
}

#[derive(Serialize)]
pub struct DesignWorkflowResponse {
    pub success: bool,
    pub workflow: DesignedWorkflow,
}

#[derive(Deserialize)]
pub struct MaterializeRequest {
    pub workflow: DesignedWorkflow,
}

#[derive(Serialize)]
pub struct MaterializeResponse {
    pub success: bool,
    pub agents_created: usize,
    pub agent_ids: Vec<String>,
}

#[derive(Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub role: String,
    pub tools: Vec<String>,
    #[serde(default = "default_memory")]
    pub memory_type: String,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_model")]
    pub model: String,
    pub system_prompt: String,
}

fn default_memory() -> String {
    "persistent".to_string()
}
fn default_provider() -> String {
    "anthropic".to_string()
}
fn default_model() -> String {
    "claude-sonnet-4-6".to_string()
}

// ---------------------------------------------------------------------------
// POST /projects/:id/agents — Create a single agent
// ---------------------------------------------------------------------------

pub async fn create_agent(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
    Json(req): Json<CreateAgentRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;

    let agents_dir = app.project_agents_dir(&project_id);
    let input = nexus_store::AgentDefinitionInput {
        name: req.name,
        role: req.role,
        tools: req.tools,
        memory_type: req.memory_type,
        provider: req.provider,
        model: req.model,
        system_prompt: req.system_prompt,
    };

    let ab = AgentBuilder::new(&db);
    let agent = ab.materialize_agent(&project_id, &agents_dir, &input)?;

    info!(
        project_id = %project_id,
        agent_id = %agent.id,
        agent_name = %agent.name,
        "Agent created via direct API"
    );

    Ok(Json(serde_json::to_value(agent)?))
}

// ---------------------------------------------------------------------------
// POST /projects/:id/agents/design — LLM-powered workflow decomposition
// ---------------------------------------------------------------------------

pub async fn design_workflow(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
    Json(req): Json<DesignWorkflowRequest>,
) -> ApiResult<Json<DesignWorkflowResponse>> {
    {
        let db = app.db.lock().await;
        validate_project_access(&db, &project_id, &auth.tenant_id)
            .map_err(ApiError::Forbidden)?;
    }

    let _slot = app
        .rate_limiter
        .acquire_llm_slot()
        .await
        .map_err(ApiError::TooManyRequests)?;

    let config = build_llm_config(&app);

    let system_prompt = build_designer_system_prompt();
    let user_prompt = build_designer_user_prompt(&req);

    let messages = vec![
        json!({ "role": "system", "content": system_prompt }),
        json!({ "role": "user", "content": user_prompt }),
    ];

    let start = std::time::Instant::now();
    let response = call_llm_with_tools(&config, &messages, &[])
        .await
        .map_err(|e| ApiError::Internal(format!("LLM workflow design failed: {e}")))?;
    let latency_ms = start.elapsed().as_millis() as u64;

    let text = response.text.unwrap_or_default();

    let workflow = parse_workflow_response(&text, &req.idea)?;

    app.cost_tracker
        .record_call(
            Some(&project_id),
            &config.model,
            &config.provider,
            response.input_tokens,
            response.output_tokens,
            latency_ms,
            "workflow_designer",
        )
        .await;

    info!(
        project_id = %project_id,
        agents = workflow.agents.len(),
        connections = workflow.connections.len(),
        latency_ms,
        "Workflow designed from idea"
    );

    Ok(Json(DesignWorkflowResponse {
        success: true,
        workflow,
    }))
}

// ---------------------------------------------------------------------------
// POST /projects/:id/agents/design/materialize — Persist all designed agents
// ---------------------------------------------------------------------------

pub async fn materialize_workflow(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
    Json(req): Json<MaterializeRequest>,
) -> ApiResult<Json<MaterializeResponse>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;

    let agents_dir = app.project_agents_dir(&project_id);
    let ab = AgentBuilder::new(&db);
    let mut agent_ids = Vec::new();

    for designed in &req.workflow.agents {
        let provider = if designed.model_suggestion.contains("claude") {
            "anthropic"
        } else if designed.model_suggestion.contains("gpt") {
            "openai"
        } else {
            "anthropic"
        };

        let input = nexus_store::AgentDefinitionInput {
            name: designed.name.clone(),
            role: designed.role.clone(),
            tools: designed.tools.clone(),
            memory_type: "persistent".into(),
            provider: provider.into(),
            model: designed.model_suggestion.clone(),
            system_prompt: designed.system_prompt.clone(),
        };

        match ab.materialize_agent(&project_id, &agents_dir, &input) {
            Ok(agent) => {
                info!(agent_id = %agent.id, name = %agent.name, "Materialized designed agent");
                agent_ids.push(agent.id);
            }
            Err(e) => {
                tracing::warn!(name = %designed.name, error = %e, "Failed to materialize agent");
            }
        }
    }

    let count = agent_ids.len();

    Ok(Json(MaterializeResponse {
        success: count > 0,
        agents_created: count,
        agent_ids,
    }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_llm_config(state: &AppState) -> LlmConfig {
    if let Some(ref key) = state.anthropic_api_key {
        if !key.is_empty() {
            return LlmConfig {
                provider: "anthropic".into(),
                model: "claude-sonnet-4-20250514".into(),
                api_key: key.clone(),
                api_base: "https://api.anthropic.com".into(),
                max_tokens: 8096,
                temperature: 0.7,
            };
        }
    }
    LlmConfig {
        provider: "openai".into(),
        model: state.model.clone(),
        api_key: state.openai_api_key.clone(),
        api_base: "https://api.openai.com/v1".into(),
        max_tokens: 4096,
        temperature: 0.7,
    }
}

pub fn build_designer_system_prompt() -> String {
    r#"You are Nexus Workflow Architect, an expert at designing multi-agent AI systems.

Given a user's idea or automation goal, you decompose it into a team of specialized AI agents
that work together in a workflow. Each agent has a clear role, tools, and system prompt.

RESPOND ONLY IN VALID JSON matching this exact schema:

{
  "title": "Short workflow title",
  "description": "2-3 sentence description of what this workflow accomplishes",
  "execution_mode": "sequential" | "parallel" | "pipeline" | "event_driven",
  "estimated_complexity": "simple" | "moderate" | "complex",
  "tags": ["tag1", "tag2"],
  "agents": [
    {
      "temp_id": "agent_1",
      "name": "Human-readable Agent Name",
      "role": "One-line role description",
      "description": "Detailed description of what this agent does and why it exists",
      "system_prompt": "Full system prompt for the agent (be specific and detailed)",
      "tools": ["tool_name_1", "tool_name_2"],
      "model_suggestion": "claude-sonnet-4-6" | "gpt-4.1" | "gpt-4.1-mini",
      "trigger": "on_demand" | "scheduled" | "event_driven" | "always_on",
      "icon": "emoji icon for the agent"
    }
  ],
  "connections": [
    {
      "from_agent": "agent_1",
      "to_agent": "agent_2",
      "condition": "When agent_1 completes analysis",
      "data_passed": "Description of what data flows between agents"
    }
  ]
}

RULES:
- Design 2-8 agents depending on complexity
- Each agent must have a UNIQUE, specific role — no overlapping responsibilities
- System prompts must be detailed, actionable, and specific to the agent's role
- Tools should be realistic: search_web, send_email, query_database, generate_report, analyze_data, write_document, code_review, monitor_metrics, send_notification, schedule_task, file_read, file_write, http_request, summarize_text
- Choose the right execution_mode for the workflow nature
- For model_suggestion: use claude-sonnet-4-6 for complex reasoning, gpt-4.1 for general tasks, gpt-4.1-mini for simple/fast tasks
- Connections define how agents communicate and pass data
- The workflow should feel complete — cover the full lifecycle of the automation
- Use descriptive emoji icons that represent each agent's role"#.to_string()
}

fn build_designer_user_prompt(req: &DesignWorkflowRequest) -> String {
    let mut prompt = format!(
        "Design a multi-agent workflow for this idea:\n\n\"{}\"",
        req.idea
    );

    if let Some(ref complexity) = req.complexity {
        prompt.push_str(&format!(
            "\n\nDesired complexity level: {}",
            complexity
        ));
    }
    if let Some(ref industry) = req.industry {
        prompt.push_str(&format!("\n\nIndustry/domain: {}", industry));
    }

    prompt
}

fn parse_workflow_response(text: &str, idea: &str) -> Result<DesignedWorkflow, ApiError> {
    let json_str = extract_json_block(text);

    match serde_json::from_str::<DesignedWorkflow>(json_str) {
        Ok(workflow) => Ok(workflow),
        Err(e) => {
            tracing::warn!(error = %e, "Failed to parse LLM workflow design response, using fallback");
            build_fallback_workflow(idea)
        }
    }
}

fn extract_json_block(text: &str) -> &str {
    if let Some(start) = text.find("```json") {
        let after_fence = &text[start + 7..];
        if let Some(end) = after_fence.find("```") {
            return after_fence[..end].trim();
        }
    }
    if let Some(start) = text.find("```") {
        let after_fence = &text[start + 3..];
        if let Some(end) = after_fence.find("```") {
            let block = after_fence[..end].trim();
            if block.starts_with('{') {
                return block;
            }
        }
    }
    let trimmed = text.trim();
    if trimmed.starts_with('{') {
        return trimmed;
    }
    text
}

fn build_fallback_workflow(idea: &str) -> Result<DesignedWorkflow, ApiError> {
    let short_idea = if idea.len() > 80 {
        format!("{}...", &idea[..80])
    } else {
        idea.to_string()
    };

    Ok(DesignedWorkflow {
        title: format!("Workflow: {}", short_idea),
        description: format!("Automated workflow for: {}", idea),
        execution_mode: "sequential".into(),
        estimated_complexity: "moderate".into(),
        tags: vec!["automation".into(), "custom".into()],
        agents: vec![
            DesignedWorkflowAgent {
                temp_id: "agent_1".into(),
                name: "Coordinator".into(),
                role: "Orchestrates the workflow and delegates tasks".into(),
                description: "Central coordinator that receives the input, breaks it into subtasks, and routes them to specialized agents.".into(),
                system_prompt: format!(
                    "You are a workflow coordinator for: {}. Break incoming requests into clear subtasks, delegate to the right specialist, and synthesize results into a coherent output.",
                    idea
                ),
                tools: vec!["analyze_data".into(), "summarize_text".into()],
                model_suggestion: "claude-sonnet-4-6".into(),
                trigger: "on_demand".into(),
                icon: "🎯".into(),
            },
            DesignedWorkflowAgent {
                temp_id: "agent_2".into(),
                name: "Executor".into(),
                role: "Executes the core task".into(),
                description: "Handles the primary execution of the workflow task.".into(),
                system_prompt: format!(
                    "You are a specialist executor for: {}. Carry out the assigned task with precision and report results clearly.",
                    idea
                ),
                tools: vec!["file_write".into(), "http_request".into()],
                model_suggestion: "gpt-4o".into(),
                trigger: "event_driven".into(),
                icon: "⚡".into(),
            },
            DesignedWorkflowAgent {
                temp_id: "agent_3".into(),
                name: "Quality Checker".into(),
                role: "Reviews output quality".into(),
                description: "Reviews and validates the output of other agents before delivery.".into(),
                system_prompt: "You review outputs for quality, completeness, and accuracy. Flag issues and suggest improvements. Be thorough but constructive.".into(),
                tools: vec!["analyze_data".into()],
                model_suggestion: "gpt-4.1-mini".into(),
                trigger: "event_driven".into(),
                icon: "✅".into(),
            },
        ],
        connections: vec![
            WorkflowConnection {
                from_agent: "agent_1".into(),
                to_agent: "agent_2".into(),
                condition: "After task decomposition".into(),
                data_passed: "Subtask assignments and context".into(),
            },
            WorkflowConnection {
                from_agent: "agent_2".into(),
                to_agent: "agent_3".into(),
                condition: "After execution completes".into(),
                data_passed: "Execution results for review".into(),
            },
        ],
    })
}
