//! Agentic loop — reason, act, observe, repeat.
//!
//! This is the brain of Nexus. It takes a user task, calls the LLM with tool
//! definitions, executes tool calls, feeds results back, and repeats until done.
//!
//! LLM calling is delegated to [`crate::llm_client`] — the single source of
//! truth for all provider-specific logic.

use std::sync::Arc;

use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tracing::info;

use crate::agent_tools::ToolRegistry;
use crate::llm_client::{self, LlmConfig, LlmToolResponse};
use crate::state::AppState;

/// Events streamed to the frontend during agent execution.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum AgentEvent {
    /// Agent is thinking / reasoning
    #[serde(rename = "thinking")]
    Thinking { content: String },
    /// Text content from the agent
    #[serde(rename = "text")]
    Text { content: String },
    /// Agent is about to call a tool
    #[serde(rename = "tool_start")]
    ToolStart { tool: String, arguments: Value },
    /// Tool execution completed
    #[serde(rename = "tool_result")]
    ToolResult {
        tool: String,
        success: bool,
        output: String,
        duration_ms: u64,
    },
    /// Iteration info
    #[serde(rename = "iteration")]
    Iteration { number: u32, max: u32 },
    /// Agent completed the task
    #[serde(rename = "done")]
    Done {
        summary: String,
        iterations: u32,
        tools_used: Vec<String>,
    },
    /// Agent encountered an error
    #[serde(rename = "error")]
    Error { message: String },
    /// Agent proposes changes (for approval/review)
    #[serde(rename = "proposal")]
    Proposal {
        intent: String,
        operations: Vec<serde_json::Value>,
        risk_level: String,
        warnings: Vec<String>,
    },
    /// Multi-agent pipeline phase change
    #[serde(rename = "phase")]
    Phase { name: String, status: String },
}

/// Configuration for the agent loop.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub max_iterations: u32,
    pub system_prompt: String,
    pub project_dir: std::path::PathBuf,
    pub provider: String,
    pub model: String,
    pub api_key: String,
    pub api_base: String,
    /// Project ID for cost tracking (None → record as global).
    #[doc(hidden)]
    pub project_id: Option<String>,
    /// Tenant for per-tenant cost + budget. Defaults to `"default"` when
    /// the caller hasn't established a tenant context.
    #[doc(hidden)]
    pub tenant_id: Option<String>,
}

impl AgentConfig {
    /// Convert to the unified [`LlmConfig`].
    fn to_llm_config(&self) -> LlmConfig {
        LlmConfig {
            provider: self.provider.clone(),
            model: self.model.clone(),
            api_key: self.api_key.clone(),
            api_base: self.api_base.clone(),
            max_tokens: 0,
            temperature: 0.0,
        }
    }
}

/// Run the agentic loop.
pub async fn run_agent_loop(
    config: AgentConfig,
    task: String,
    tx: mpsc::Sender<AgentEvent>,
    app: Arc<AppState>,
) {
    let tools = ToolRegistry::new(config.project_dir.clone());
    let api_tools = tools.to_api_tools();
    let llm_cfg = config.to_llm_config();

    let mut messages: Vec<Value> = vec![
        json!({"role": "system", "content": config.system_prompt}),
        json!({"role": "user", "content": task}),
    ];

    let mut iteration = 0u32;
    let mut all_tools_used: Vec<String> = Vec::new();
    let mut _consecutive_no_tool_calls = 0u32;
    let mut last_tool_names: Vec<String> = Vec::new();
    let mut consecutive_same_tool = 0u32;

    loop {
        iteration += 1;
        if iteration > config.max_iterations {
            let _ = tx
                .send(AgentEvent::Error {
                    message: format!(
                        "Max iterations ({}) reached. Task may be incomplete.",
                        config.max_iterations
                    ),
                })
                .await;
            break;
        }

        let _ = tx
            .send(AgentEvent::Iteration {
                number: iteration,
                max: config.max_iterations,
            })
            .await;

        // Budget gate: block the call up-front if the tenant is over cap.
        let budget = app.cost_tracker.check_budget(config.project_id.as_deref()).await;
        if !budget.allowed {
            let _ = tx
                .send(AgentEvent::Error {
                    message: format!(
                        "LLM budget exhausted (${:.2} of ${:.2} used). Aborting agent loop.",
                        budget.daily_spent, budget.daily_limit
                    ),
                })
                .await;
            break;
        }

        // Call LLM with tools through the unified envelope path so the
        // per-tenant budget brake (ADR-005) and Prometheus cost metrics
        // (ADR-005 §4) fire on every iteration. The cost estimate is a
        // 4-bytes-per-token heuristic of the prompt, capped by the model's
        // max output tokens — good enough for a guardrail.
        let call_start = std::time::Instant::now();
        let envelope = llm_client::LlmCallEnvelope::for_tenant(
            config.tenant_id.as_deref().unwrap_or("default"),
            "agent_loop.iteration",
        )
        .with_estimated_cost_from_messages(&messages, &llm_cfg.model, 4096);
        let envelope = llm_client::LlmCallEnvelope {
            project_id: config.project_id.as_deref(),
            ..envelope
        };
        let response: LlmToolResponse = match llm_client::call_llm_with_envelope(
            &app.db,
            &envelope,
            &llm_cfg,
            &messages,
            &api_tools,
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                let _ = tx
                    .send(AgentEvent::Error {
                        message: format!("LLM call failed: {}", e),
                    })
                    .await;
                break;
            }
        };
        let latency_ms = call_start.elapsed().as_millis() as u64;

        // Record LLM usage for per-tenant / global cost tracking. Without
        // this, agent-loop calls never increment the cost tracker and every
        // subsequent `check_budget` passes regardless of actual spend.
        app.cost_tracker
            .record_call(
                config.project_id.as_deref(),
                &config.model,
                &config.provider,
                response.input_tokens,
                response.output_tokens,
                latency_ms,
                "agent_loop",
            )
            .await;

        // Stream text content
        if let Some(text) = &response.text {
            if !text.is_empty() {
                let _ = tx
                    .send(AgentEvent::Text {
                        content: text.clone(),
                    })
                    .await;
            }
        }

        // Refuse truncated tool responses — partial JSON would write half-files.
        if response.truncated {
            let msg = format!(
                "LLM response truncated at max_tokens; refusing to execute {} possibly-partial tool call(s). Increase max_tokens or split the task.",
                response.tool_calls.len()
            );
            tracing::error!("{}", msg);
            let _ = tx.send(AgentEvent::Error { message: msg }).await;
            break;
        }

        // If no tool calls, agent is done
        if response.tool_calls.is_empty() {
            let final_text = response.text.unwrap_or_default();
            messages.push(json!({"role": "assistant", "content": &final_text}));

            let summary = if final_text.is_empty() {
                "Task completed.".to_string()
            } else {
                final_text
            };

            let _ = tx
                .send(AgentEvent::Done {
                    summary,
                    iterations: iteration,
                    tools_used: all_tools_used.clone(),
                })
                .await;
            break;
        }

        // Build assistant message with tool calls
        let mut assistant_msg = json!({"role": "assistant"});
        if let Some(text) = &response.text {
            assistant_msg["content"] = json!(text);
        }

        let tool_calls_json: Vec<Value> = response
            .tool_calls
            .iter()
            .map(|tc| {
                json!({
                    "id": tc.id,
                    "type": "function",
                    "function": {
                        "name": tc.name,
                        "arguments": if tc.arguments.is_string() {
                            tc.arguments.clone()
                        } else {
                            Value::String(tc.arguments.to_string())
                        }
                    }
                })
            })
            .collect();
        assistant_msg["tool_calls"] = json!(tool_calls_json);
        messages.push(assistant_msg);

        // ── Early stopping: detect repetitive tool use (stuck loop) ────────────
        let current_tool_names: Vec<String> = response.tool_calls.iter().map(|tc| tc.name.clone()).collect();
        if !current_tool_names.is_empty() && current_tool_names == last_tool_names {
            consecutive_same_tool += 1;
            if consecutive_same_tool >= 3 {
                let _ = tx.send(AgentEvent::Error {
                    message: format!(
                        "Early stop: repeated identical tool sequence ({}) 3 times. Breaking loop to prevent infinite retry.",
                        current_tool_names.join(", ")
                    ),
                }).await;
                break;
            }
        } else {
            consecutive_same_tool = 0;
        }
        last_tool_names = current_tool_names;

        // Execute each tool call
        for tool_call in &response.tool_calls {
            let _ = tx
                .send(AgentEvent::ToolStart {
                    tool: tool_call.name.clone(),
                    arguments: tool_call.arguments.clone(),
                })
                .await;

            all_tools_used.push(tool_call.name.clone());

            let start = std::time::Instant::now();
            let result = tools.execute(tool_call).await;
            let duration_ms = start.elapsed().as_millis() as u64;

            let _ = tx
                .send(AgentEvent::ToolResult {
                    tool: tool_call.name.clone(),
                    success: result.success,
                    output: if result.output.len() > 2000 {
                        format!("{}...[truncated]", &result.output[..2000])
                    } else {
                        result.output.clone()
                    },
                    duration_ms,
                })
                .await;

            // Add tool result to messages
            messages.push(json!({
                "role": "tool",
                "tool_call_id": tool_call.id,
                "content": result.output,
            }));
        }

        // ── Track iterations without tool calls ─────────────────────────────
        if response.tool_calls.is_empty() {
            _consecutive_no_tool_calls += 1;
        } else {
            _consecutive_no_tool_calls = 0;
        }

        // Context window management
        if messages.len() > 40 {
            llm_client::compact_messages(&mut messages, 10);
        }
    }

    info!(
        iterations = iteration,
        tools = all_tools_used.len(),
        "Agent loop finished"
    );
}
