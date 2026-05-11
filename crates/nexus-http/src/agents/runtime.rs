//! Unified agent execution runtime.
//!
//! All agent types (project, coding, wave, background) execute through this
//! single runtime. It replaces the fragmented execution paths:
//! - `agent_loop::run_agent_loop` (project agents)
//! - `coding_agents::engine::run_coding_agent_loop` (coding agents)
//! - `coding_agents::wave_orchestrator` (parallel wave pipeline)
//! - `background_executor` (scheduled agents)
//!
//! The runtime takes an [`AgentDefinition`] and produces a stream of
//! [`AgentEvent`]s plus a final [`AgentResult`].

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::agent_tools::dependency_checker::ToolDependencyChecker;
use crate::agent_tools::safety::{SafetyResult, ToolSafetyLayer};
use crate::llm_client::{self, LlmConfig};
use crate::state::AppState;

use super::definition::AgentDefinition;
use super::events::{AgentEvent, AgentResult, ToolCallRecord};
use super::tools::{ToolInput, ToolOutput, ToolRegistry};

/// Unified agent execution runtime.
///
/// Holds shared references to the application state and tool registry.
/// Create one per server lifetime and call [`execute`](Self::execute) for
/// each agent invocation.
pub struct AgentRuntime {
    tool_registry: Arc<ToolRegistry>,
    app: Arc<AppState>,
}

/// Context provided to an agent during execution.
pub struct AgentContext {
    /// Project ID (if project-scoped).
    pub project_id: Option<String>,
    /// Project directory on disk (if applicable).
    pub project_dir: Option<std::path::PathBuf>,
    /// Additional context/instructions appended to the task prompt.
    pub context: String,
    /// Previous conversation history (for multi-turn agents).
    pub history: Vec<(String, String)>,
}

impl AgentRuntime {
    /// Create a new agent runtime.
    pub fn new(app: Arc<AppState>, tool_registry: Arc<ToolRegistry>) -> Self {
        Self {
            tool_registry,
            app,
        }
    }

    /// Execute an agent with the given definition and task.
    ///
    /// Returns an event receiver for streaming progress and a join handle
    /// that resolves to the final [`AgentResult`].
    pub async fn execute(
        &self,
        definition: &AgentDefinition,
        task: &str,
        context: AgentContext,
    ) -> Result<
        (
            mpsc::Receiver<AgentEvent>,
            tokio::task::JoinHandle<Result<AgentResult, String>>,
        ),
        String,
    > {
        let (tx, rx) = mpsc::channel::<AgentEvent>(64);
        let def = definition.clone();
        let task = task.to_string();
        let app = self.app.clone();
        let registry = self.tool_registry.clone();
        let timeout = Duration::from_secs(def.timeout_secs);

        let handle = tokio::spawn(async move {
            let start = Instant::now();
            let _ = tx
                .send(AgentEvent::Started {
                    agent_id: def.id.clone(),
                    task: task.clone(),
                })
                .await;

            // Resolve tools from registry
            let (tools, missing) = registry.resolve(&def.tools);
            if !missing.is_empty() {
                warn!(
                    agent = %def.id,
                    missing = ?missing,
                    "Some tools not found in registry"
                );
            }

            // Build tool definitions for LLM function calling
            let tool_defs: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": t.name(),
                            "description": t.description(),
                            "parameters": t.schema(),
                        }
                    })
                })
                .collect();

            // Build the LLM config from agent's model preference or app defaults
            let (provider, model_name) = def
                .model_preference
                .as_ref()
                .map(|p| (p.provider.as_str(), p.model.as_str()))
                .unwrap_or(("openai", app.model.as_str()));

            let api_key = match provider {
                "anthropic" => app.anthropic_api_key.clone().unwrap_or_default(),
                _ => app.openai_api_key.clone(),
            };

            let api_base = match provider {
                "anthropic" => "https://api.anthropic.com".to_string(),
                "ollama" => "http://localhost:11434".to_string(),
                _ => "https://api.openai.com/v1".to_string(),
            };

            let llm_cfg = LlmConfig {
                provider: provider.to_string(),
                model: model_name.to_string(),
                api_key,
                api_base,
                max_tokens: 16384,
                temperature: 0.0,
            };

            // Build message history
            let mut messages = vec![json!({"role": "system", "content": def.system_prompt})];

            // Add prior conversation history
            for (user_msg, assistant_msg) in &context.history {
                messages.push(json!({"role": "user", "content": user_msg}));
                messages
                    .push(json!({"role": "assistant", "content": assistant_msg}));
            }

            // Add context + task
            let full_task = if context.context.is_empty() {
                task.clone()
            } else {
                format!("{}\n\nContext:\n{}", task, context.context)
            };
            messages.push(json!({"role": "user", "content": full_task}));

            // Build the safety layer scoped to the project directory.
            let safety = context
                .project_dir
                .as_ref()
                .map(|dir| ToolSafetyLayer::new(dir));

            let mut tool_calls = Vec::new();
            let mut files_modified = Vec::new();
            let mut final_output = String::new();
            let mut iteration = 0u32;

            // ── Agent loop ──────────────────────────────────────────────
            loop {
                if iteration >= def.max_iterations {
                    warn!(
                        agent = %def.id,
                        iterations = iteration,
                        "Max iterations reached"
                    );
                    let _ = tx
                        .send(AgentEvent::Progress {
                            agent_id: def.id.clone(),
                            iteration,
                            max_iterations: def.max_iterations,
                            summary: format!(
                                "Max iterations ({}) reached, stopping",
                                def.max_iterations
                            ),
                        })
                        .await;
                    break;
                }
                if start.elapsed() > timeout {
                    warn!(agent = %def.id, "Agent timed out");
                    let _ = tx
                        .send(AgentEvent::Failed {
                            agent_id: def.id.clone(),
                            error: "Execution timed out".into(),
                            iteration,
                        })
                        .await;
                    return Err("Agent execution timed out".into());
                }

                // Budget check before each LLM call
                let budget = app.cost_tracker.check_budget(context.project_id.as_deref()).await;
                if !budget.allowed {
                    warn!(
                        agent = %def.id,
                        reason = ?budget.reason,
                        "Agent stopped: budget exceeded"
                    );
                    let _ = tx
                        .send(AgentEvent::Failed {
                            agent_id: def.id.clone(),
                            error: budget.reason.unwrap_or_else(|| "Budget exceeded".into()),
                            iteration,
                        })
                        .await;
                    return Err("Budget exceeded".into());
                }

                iteration += 1;
                let _ = tx
                    .send(AgentEvent::Progress {
                        agent_id: def.id.clone(),
                        iteration,
                        max_iterations: def.max_iterations,
                        summary: format!("Iteration {}/{}", iteration, def.max_iterations),
                    })
                    .await;

                // Call LLM with tools
                let call_start = std::time::Instant::now();
                let response =
                    llm_client::call_llm_with_tools(&llm_cfg, &messages, &tool_defs).await;
                let latency_ms = call_start.elapsed().as_millis() as u64;

                match response {
                    Ok(llm_response) => {
                        // Record real token usage so per-tenant budget tracking
                        // isn't stuck at $0. Without this, the `check_budget`
                        // above always passes because nothing ever ticks up.
                        app.cost_tracker
                            .record_call(
                                context.project_id.as_deref(),
                                &llm_cfg.model,
                                &llm_cfg.provider,
                                llm_response.input_tokens,
                                llm_response.output_tokens,
                                latency_ms,
                                "agent_runtime",
                            )
                            .await;

                        // Stream any text content
                        if let Some(text) = &llm_response.text {
                            if !text.is_empty() {
                                let _ = tx
                                    .send(AgentEvent::Thinking {
                                        agent_id: def.id.clone(),
                                        content: text.clone(),
                                    })
                                    .await;
                                final_output = text.clone();
                            }
                        }

                        // If no tool calls, agent is done
                        if llm_response.tool_calls.is_empty() {
                            break;
                        }

                        // Build assistant message with tool calls
                        let tool_calls_json: Vec<serde_json::Value> = llm_response
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
                                            serde_json::Value::String(tc.arguments.to_string())
                                        }
                                    }
                                })
                            })
                            .collect();

                        let mut assistant_msg = json!({"role": "assistant"});
                        if let Some(text) = &llm_response.text {
                            assistant_msg["content"] = json!(text);
                        }
                        assistant_msg["tool_calls"] = json!(tool_calls_json);
                        messages.push(assistant_msg);

                        // ── Detect task_complete early ──────────────
                        if let Some(tc) = llm_response
                            .tool_calls
                            .iter()
                            .find(|tc| tc.name == "task_complete")
                        {
                            let summary = tc
                                .arguments
                                .get("summary")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Task completed.")
                                .to_string();
                            final_output = summary;
                            break;
                        }

                        // ── Build dependency-aware execution plan ──
                        let indexed_calls: Vec<(String, serde_json::Value)> = llm_response
                            .tool_calls
                            .iter()
                            .map(|tc| (tc.name.clone(), tc.arguments.clone()))
                            .collect();
                        let plan =
                            ToolDependencyChecker::plan_execution(&indexed_calls);

                        // ── Execute groups (parallel within, sequential across) ──
                        for group in &plan.groups {
                            if group.len() == 1 {
                                // Single tool — run directly (no spawn overhead).
                                let idx = group[0];
                                let tc = &llm_response.tool_calls[idx];
                                let (record, output, msg) = execute_single_tool(
                                    &tc.id,
                                    &tc.name,
                                    &tc.arguments,
                                    &registry,
                                    &safety,
                                    &def.id,
                                    &tx,
                                )
                                .await;

                                track_file_modification(
                                    &tc.name,
                                    &tc.arguments,
                                    &mut files_modified,
                                );
                                tool_calls.push(record);

                                let _ = tx
                                    .send(AgentEvent::ToolResult {
                                        agent_id: def.id.clone(),
                                        tool: tc.name.clone(),
                                        output,
                                    })
                                    .await;

                                messages.push(msg);
                            } else {
                                // Multiple non-conflicting tools — run in parallel.
                                let futs: Vec<_> = group
                                    .iter()
                                    .map(|&idx| {
                                        let tc = &llm_response.tool_calls[idx];
                                        let id = tc.id.clone();
                                        let name = tc.name.clone();
                                        let args = tc.arguments.clone();
                                        let reg = registry.clone();
                                        let safety_ref = safety.clone();
                                        let agent_id = def.id.clone();
                                        let tx2 = tx.clone();

                                        async move {
                                            let (record, output, msg) =
                                                execute_single_tool(
                                                    &id,
                                                    &name,
                                                    &args,
                                                    &reg,
                                                    &safety_ref,
                                                    &agent_id,
                                                    &tx2,
                                                )
                                                .await;

                                            let _ = tx2
                                                .send(AgentEvent::ToolResult {
                                                    agent_id,
                                                    tool: name.clone(),
                                                    output,
                                                })
                                                .await;

                                            (idx, name, args, record, msg)
                                        }
                                    })
                                    .collect();

                                let results = futures::future::join_all(futs).await;

                                for (_, name, args, record, msg) in results {
                                    track_file_modification(
                                        &name,
                                        &args,
                                        &mut files_modified,
                                    );
                                    tool_calls.push(record);
                                    messages.push(msg);
                                }
                            }
                        }

                        // Context window management
                        if messages.len() > 50 {
                            llm_client::compact_messages(&mut messages, 12);
                        }

                        // Continue loop for next iteration
                        continue;
                    }
                    Err(e) => {
                        warn!(agent = %def.id, error = %e, "LLM call failed");
                        let _ = tx
                            .send(AgentEvent::Failed {
                                agent_id: def.id.clone(),
                                error: e.to_string(),
                                iteration,
                            })
                            .await;
                        return Err(e.to_string());
                    }
                }
            }

            let result = AgentResult {
                output: final_output,
                files_modified,
                iterations_used: iteration,
                duration_ms: start.elapsed().as_millis() as u64,
                tool_calls,
            };

            let _ = tx
                .send(AgentEvent::Completed {
                    agent_id: def.id.clone(),
                    result: result.clone(),
                })
                .await;

            info!(
                agent = %def.id,
                iterations = iteration,
                duration_ms = result.duration_ms,
                "Agent completed"
            );
            Ok(result)
        });

        Ok((rx, handle))
    }

    /// Execute multiple agents in parallel (wave pipeline pattern).
    ///
    /// Each `(AgentDefinition, task, AgentContext)` tuple spawns an independent
    /// agent execution. Results are collected once all agents finish.
    pub async fn execute_parallel(
        &self,
        definitions: Vec<(AgentDefinition, String, AgentContext)>,
    ) -> Vec<Result<AgentResult, String>> {
        let mut handles = Vec::new();

        for (def, task, ctx) in definitions {
            let runtime = AgentRuntime::new(self.app.clone(), self.tool_registry.clone());
            let handle = tokio::spawn(async move {
                match runtime.execute(&def, &task, ctx).await {
                    Ok((_rx, join_handle)) => {
                        join_handle.await.unwrap_or(Err("Task panicked".into()))
                    }
                    Err(e) => Err(e),
                }
            });
            handles.push(handle);
        }

        let mut results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => results.push(Err(format!("Task panicked: {}", e))),
            }
        }
        results
    }
}

/// Execute a single tool call, performing the safety check and emitting the
/// `ToolInvoked` event. Returns `(record, output_value, tool_result_message)`.
async fn execute_single_tool(
    call_id: &str,
    tool_name: &str,
    arguments: &serde_json::Value,
    registry: &Arc<ToolRegistry>,
    safety: &Option<ToolSafetyLayer>,
    agent_id: &str,
    tx: &mpsc::Sender<AgentEvent>,
) -> (ToolCallRecord, serde_json::Value, serde_json::Value) {
    let _ = tx
        .send(AgentEvent::ToolInvoked {
            agent_id: agent_id.to_string(),
            tool: tool_name.to_string(),
            input: arguments.clone(),
        })
        .await;

    // Safety check
    let safety_blocked = if let Some(ref sl) = safety {
        match sl.check(tool_name, arguments) {
            SafetyResult::Deny(reason) => Some(reason),
            SafetyResult::Allow => None,
        }
    } else {
        None
    };

    let tool_start = Instant::now();
    let output = if let Some(reason) = safety_blocked {
        warn!(
            agent = %agent_id,
            tool = %tool_name,
            reason = %reason,
            "Tool call blocked by safety layer"
        );
        ToolOutput {
            result: json!({"error": format!("Blocked: {}", reason)}),
            success: false,
            error: Some(format!("Blocked by safety: {}", reason)),
        }
    } else if let Some(tool) = registry.get(tool_name) {
        tool.execute(ToolInput {
            parameters: arguments.clone(),
        })
        .await
    } else {
        ToolOutput {
            result: json!({"error": format!("Tool '{}' not found", tool_name)}),
            success: false,
            error: Some(format!("Tool '{}' not found", tool_name)),
        }
    };
    let tool_duration = tool_start.elapsed().as_millis() as u64;

    let record = ToolCallRecord {
        tool: tool_name.to_string(),
        input_summary: truncate_json(arguments, 200),
        output_summary: truncate_json(&output.result, 200),
        duration_ms: tool_duration,
        success: output.success,
    };

    let output_str = serde_json::to_string(&output.result).unwrap_or_default();
    let msg = json!({
        "role": "tool",
        "tool_call_id": call_id,
        "content": output_str,
    });

    (record, output.result, msg)
}

/// Track file modifications from write/edit tools.
fn track_file_modification(
    tool_name: &str,
    arguments: &serde_json::Value,
    files_modified: &mut Vec<String>,
) {
    if tool_name.contains("write") || tool_name.contains("edit") {
        if let Some(path) = arguments.get("path").and_then(|p| p.as_str()) {
            if !files_modified.contains(&path.to_string()) {
                files_modified.push(path.to_string());
            }
        }
    }
}

/// Truncate a JSON value to a string of at most `max_len` characters.
fn truncate_json(value: &serde_json::Value, max_len: usize) -> String {
    let s = serde_json::to_string(value).unwrap_or_default();
    if s.len() > max_len {
        format!("{}...", &s[..max_len])
    } else {
        s
    }
}
