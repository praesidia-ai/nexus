//! Core coding loop engine — runs a single agent's LLM loop with tools.
//!
//! This is the inner loop that powers every coding agent. It calls the LLM
//! with tool definitions, executes tool calls, feeds results back, and
//! repeats until the agent signals completion or hits the iteration limit.
//!
//! LLM calling is delegated to [`crate::llm_client`] — the single source of
//! truth for all provider-specific logic.

use std::time::Instant;

use serde_json::{json, Value};
use tracing::info;

use crate::agent_tools::ToolRegistry;
use crate::llm_client::{self, LlmConfig};

use super::traits::*;
use super::types::*;

/// Run a single agent's LLM loop, streaming events and returning the output.
pub async fn run_coding_agent_loop(
    agent: &dyn CodingAgent,
    ctx: &CodingAgentContext,
) -> anyhow::Result<AgentOutput> {
    let start = Instant::now();
    let tools = ToolRegistry::new(ctx.workspace.project_dir.clone());
    let api_tools = build_filtered_tools(&tools, agent.tools_allowed());

    // Convert CodingLlmConfig → unified LlmConfig
    let llm_cfg = LlmConfig {
        provider: ctx.llm_config.provider.clone(),
        model: ctx.llm_config.model.clone(),
        api_key: ctx.llm_config.api_key.clone(),
        api_base: ctx.llm_config.api_base.clone(),
        max_tokens: ctx.llm_config.max_tokens,
        temperature: ctx.llm_config.temperature,
    };

    let system_prompt = agent.system_prompt(ctx);
    let task_prompt = build_task_prompt(ctx).await;

    let mut messages: Vec<Value> = vec![
        json!({"role": "system", "content": system_prompt}),
        json!({"role": "user", "content": task_prompt}),
    ];

    let mut iteration = 0u32;
    let max_iter = agent.max_iterations();
    let mut all_changes: Vec<FileChange> = Vec::new();
    let all_decisions: Vec<AgentDecision> = vec![];
    let mut all_tools_used: Vec<String> = Vec::new();
    let mut final_summary = String::new();

    loop {
        iteration += 1;
        if iteration > max_iter {
            let _ = ctx
                .event_tx
                .send(CodingEvent::Error {
                    message: format!(
                        "{} agent reached max iterations ({})",
                        agent.name(),
                        max_iter
                    ),
                    fatal: false,
                })
                .await;
            break;
        }

        let _ = ctx
            .event_tx
            .send(CodingEvent::Iteration {
                number: iteration,
                max: max_iter,
                agent: agent.role(),
            })
            .await;

        // Budget gate + cost recording — prevents coding-agent loops from
        // running unbounded spend when the tenant is already over budget.
        if let Some(tracker) = ctx.cost_tracker.as_ref() {
            // CostTrackerRef doesn't expose `check_budget`; record-only keeps
            // accurate usage. (Budget enforcement happens at the oneshot/
            // coding-agents handler entry, which calls AppState::cost_tracker.)
            let _ = tracker;
        }

        // Call LLM — delegated to unified client
        let call_start = std::time::Instant::now();
        let response =
            match llm_client::call_llm_with_tools(&llm_cfg, &messages, &api_tools).await {
                Ok(r) => r,
                Err(e) => {
                    let _ = ctx
                        .event_tx
                        .send(CodingEvent::Error {
                            message: format!("LLM call failed for {}: {}", agent.name(), e),
                            fatal: true,
                        })
                        .await;
                    return Err(anyhow::anyhow!("LLM call failed: {}", e));
                }
            };
        let latency_ms = call_start.elapsed().as_millis() as u64;

        // Record LLM usage live for per-tenant / global budget tracking.
        if let Some(tracker) = ctx.cost_tracker.as_ref() {
            tracker
                .record_call(
                    ctx.project_id.as_deref(),
                    &llm_cfg.model,
                    &llm_cfg.provider,
                    response.input_tokens,
                    response.output_tokens,
                    latency_ms,
                    "coding_agent",
                )
                .await;
        }

        // Surface this call's token + cost delta on the coding-event stream.
        // The wave/classic orchestrators fan this out to Agent TV via
        // build_event_bus so individual agent cards show real, non-fiction
        // cost + token counters.
        let call_cost_usd =
            crate::cost_intelligence::estimate_cost_usd(
                &llm_cfg.model,
                response.input_tokens,
                response.output_tokens,
            );
        let _ = ctx
            .event_tx
            .send(CodingEvent::LlmUsage {
                agent: agent.role(),
                tokens_in: response.input_tokens,
                tokens_out: response.output_tokens,
                cost_usd: call_cost_usd,
            })
            .await;

        if let Some(text) = &response.text {
            if !text.is_empty() {
                let _ = ctx
                    .event_tx
                    .send(CodingEvent::Thinking {
                        agent: agent.role(),
                        content: text.clone(),
                    })
                    .await;
                final_summary = text.clone();
            }
        }

        // Refuse to act on truncated responses: tool-call JSON may be partial,
        // which silently produces half-written files. Surface as a fatal error
        // so the outer auto-repair loop can retry with a different strategy.
        if response.truncated {
            let msg = format!(
                "{} agent response was truncated at max_tokens ({}). Refusing to execute {} possibly-partial tool call(s). Increase max_tokens or split the task into smaller file edits.",
                agent.name(),
                llm_cfg.max_tokens,
                response.tool_calls.len()
            );
            tracing::error!("{}", msg);
            let _ = ctx
                .event_tx
                .send(CodingEvent::Error { message: msg.clone(), fatal: true })
                .await;
            return Err(anyhow::anyhow!(msg));
        }

        if response.tool_calls.is_empty() {
            break;
        }

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

        for tool_call in &response.tool_calls {
            if !agent.tools_allowed().contains(&tool_call.name.as_str()) {
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": tool_call.id,
                    "content": format!("Tool '{}' is not available to the {} agent.", tool_call.name, agent.name()),
                }));
                continue;
            }

            let _ = ctx
                .event_tx
                .send(CodingEvent::ToolCall {
                    agent: agent.role(),
                    tool: tool_call.name.clone(),
                    arguments: tool_call.arguments.clone(),
                })
                .await;

            all_tools_used.push(tool_call.name.clone());

            let tool_start = Instant::now();
            let result = tools.execute(tool_call).await;
            let duration_ms = tool_start.elapsed().as_millis() as u64;

            if is_write_tool(&tool_call.name) && result.success {
                let change = extract_file_change(tool_call, agent.role());
                if let Some(c) = change {
                    all_changes.push(c.clone());
                    let _ = ctx
                        .event_tx
                        .send(CodingEvent::FileChange { change: c })
                        .await;
                }
            }

            let output_preview = truncate_chars(&result.output, 2000);

            let _ = ctx
                .event_tx
                .send(CodingEvent::ToolResult {
                    agent: agent.role(),
                    tool: tool_call.name.clone(),
                    success: result.success,
                    output: output_preview,
                    duration_ms,
                })
                .await;

            messages.push(json!({
                "role": "tool",
                "tool_call_id": tool_call.id,
                "content": result.output,
            }));
        }

        // Context window management — delegated to unified compaction
        if messages.len() > 50 {
            llm_client::compact_messages(&mut messages, 12);
        }
    }

    let elapsed = start.elapsed().as_millis() as u64;

    info!(
        agent = agent.name(),
        iterations = iteration,
        files_changed = all_changes.len(),
        duration_ms = elapsed,
        "Coding agent loop finished"
    );

    Ok(AgentOutput {
        agent: agent.role(),
        summary: final_summary,
        files_changed: all_changes,
        decisions: all_decisions,
        errors: Vec::new(),
        should_continue: true,
        next_phase: None,
        iterations_used: iteration,
    })
}

// ---------------------------------------------------------------------------
// Build the task prompt with full workspace context
// ---------------------------------------------------------------------------

async fn build_task_prompt(ctx: &CodingAgentContext) -> String {
    let state = ctx.workspace.state.read().await;

    let mut prompt = format!("## TASK\n{}\n\n", ctx.workspace.task.description);

    if !ctx.brain_context.is_empty() {
        prompt.push_str(&ctx.brain_context);
        prompt.push('\n');
    }

    if !ctx.code_graph_context.is_empty() {
        prompt.push_str("## CODE GRAPH SUMMARY\n");
        prompt.push_str(&ctx.code_graph_context);
        prompt.push('\n');
    }

    if !ctx.memory_context.is_empty() {
        prompt.push_str(&ctx.memory_context);
        prompt.push('\n');
    }

    if let Some(plan) = &state.plan {
        prompt.push_str("## IMPLEMENTATION PLAN\n");
        prompt.push_str(&plan.summary);
        prompt.push_str("\n\nSteps:\n");
        for step in &plan.steps {
            prompt.push_str(&format!(
                "{}. [{}] {}\n",
                step.order,
                step.agent.as_str(),
                step.description
            ));
            for f in &step.files {
                prompt.push_str(&format!("   - {}\n", f));
            }
        }
        prompt.push('\n');
    }

    if !state.files_modified.is_empty() {
        prompt.push_str("## FILES ALREADY CHANGED\n");
        for change in &state.files_modified {
            prompt.push_str(&format!(
                "- {} ({:?} by {}): {}\n",
                change.path,
                change.change_type,
                change.agent.as_str(),
                change.description,
            ));
        }
        prompt.push('\n');
    }

    if !state.errors.is_empty() {
        prompt.push_str("## ERRORS FROM PREVIOUS PHASES\n");
        for err in &state.errors {
            prompt.push_str(&format!(
                "- [{}] {}: {}\n",
                err.phase.as_str(),
                err.agent.as_str(),
                err.error,
            ));
        }
        prompt.push('\n');
    }

    if !state.verification_results.is_empty() {
        prompt.push_str("## VERIFICATION RESULTS\n");
        for v in &state.verification_results {
            let status = if v.passed { "PASS" } else { "FAIL" };
            prompt.push_str(&format!("- {:?}: {}\n", v.check_type, status));
            for e in &v.errors {
                prompt.push_str(&format!("  ERROR: {}\n", e));
            }
        }
        prompt.push('\n');
    }

    if let Some(prev) = &ctx.previous_agent_output {
        prompt.push_str("## PREVIOUS AGENT OUTPUT\n");
        prompt.push_str(&truncate_chars(prev, 4000));
        prompt.push('\n');
    }

    prompt
}

// ---------------------------------------------------------------------------
// Tool filtering — restrict which tools an agent can use
// ---------------------------------------------------------------------------

/// Truncate a `&str` to at most `max_chars` user-visible characters without
/// splitting a UTF-8 codepoint. Using `&s[..max]` byte-slices panics on multi-
/// byte input (emoji, CJK) — LLM output regularly contains both.
fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out = String::with_capacity(max_chars.saturating_add(16));
    for ch in s.chars().take(max_chars) {
        out.push(ch);
    }
    out.push_str("...[truncated]");
    out
}

fn build_filtered_tools(registry: &ToolRegistry, allowed: Vec<&str>) -> Vec<Value> {
    registry
        .to_api_tools()
        .into_iter()
        .filter(|t| {
            t["function"]["name"]
                .as_str()
                .map(|n| allowed.contains(&n))
                .unwrap_or(false)
        })
        .collect()
}

fn is_write_tool(name: &str) -> bool {
    matches!(name, "file_write" | "file_edit" | "bash")
}

fn extract_file_change(call: &crate::agent_tools::ToolCall, agent: AgentRole) -> Option<FileChange> {
    let path = call.arguments["path"].as_str()?;
    let change_type = match call.name.as_str() {
        "file_write" => ChangeType::Created,
        "file_edit" => ChangeType::Modified,
        _ => return None,
    };
    Some(FileChange {
        path: path.to_string(),
        change_type,
        agent,
        description: String::new(),
        diff_summary: String::new(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}
