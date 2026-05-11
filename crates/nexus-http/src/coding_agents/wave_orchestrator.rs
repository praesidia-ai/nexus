//! Wave Orchestrator — runs 50+ mini-agents in parallel waves.
//!
//! Each wave contains multiple agents that execute concurrently via tokio::spawn.
//! Waves execute sequentially (wave 0 → wave 1 → ...), but agents within a wave
//! run fully in parallel. Each agent gets the shared workspace context and outputs
//! from all previous waves.
//!
//! ```text
//! Wave 0: ──┬── agent_1 ──┬──▶ merge results
//!           ├── agent_2 ──┤
//!           ├── agent_3 ──┤
//!           └── agent_N ──┘
//!                              │
//! Wave 1: ──┬── agent_A ──┬──▶ merge results
//!           ├── agent_B ──┤
//!           └── agent_C ──┘
//!                              │
//! ...continues until all waves complete
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{info, warn, error};

use nexus_kernel::{EventSource, SystemEvent};

use crate::code_graph::CodeGraph;
use crate::project_brain::ProjectBrain;
use crate::state::AppState;

use super::engine::run_coding_agent_loop;
use super::mini_agent::{self, MiniAgent, Wave};
use super::traits::*;
use super::types::*;

// ---------------------------------------------------------------------------
// Wave pipeline configuration
// ---------------------------------------------------------------------------

/// Configuration for the wave-based pipeline.
pub struct WavePipelineConfig {
    pub task: CodingTask,
    pub project_dir: PathBuf,
    pub llm_config: CodingLlmConfig,
    /// Which waves to run (None = all)
    pub waves: Option<Vec<u8>>,
    /// Specific agent IDs to skip
    pub skip_agents: Vec<String>,
    /// Specific agent IDs to run (if set, only these run)
    pub only_agents: Option<Vec<String>>,
    /// Max agents to run in parallel per wave (default: unlimited)
    pub max_parallel: Option<usize>,
    /// Max total iterations across all agents
    pub max_total_iterations: u32,
    /// Whether to stop the pipeline if a fatal agent fails
    pub fail_fast: bool,
    /// Project ID — plumbed into CodingAgentContext for cost attribution.
    pub project_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Wave events — streamed to the frontend
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum WaveEvent {
    /// Pipeline has started
    #[serde(rename = "wave_pipeline_start")]
    PipelineStart {
        total_waves: usize,
        total_agents: usize,
    },

    /// A wave is starting
    #[serde(rename = "wave_start")]
    WaveStart {
        wave_index: u8,
        wave_label: String,
        agent_count: usize,
        agent_ids: Vec<String>,
    },

    /// An individual agent within a wave has started
    #[serde(rename = "wave_agent_start")]
    AgentStart {
        wave_index: u8,
        agent_id: String,
        agent_label: String,
        agent_icon: String,
        agent_color: String,
    },

    /// An agent's tool call
    #[serde(rename = "wave_agent_tool")]
    AgentTool {
        agent_id: String,
        tool: String,
        arguments: serde_json::Value,
    },

    /// An agent's tool result
    #[serde(rename = "wave_agent_tool_result")]
    AgentToolResult {
        agent_id: String,
        tool: String,
        success: bool,
        output_preview: String,
        duration_ms: u64,
    },

    /// An agent iteration tick
    #[serde(rename = "wave_agent_iteration")]
    AgentIteration {
        agent_id: String,
        number: u32,
        max: u32,
    },

    /// An agent has completed
    #[serde(rename = "wave_agent_done")]
    AgentDone {
        wave_index: u8,
        agent_id: String,
        agent_label: String,
        success: bool,
        summary: String,
        iterations_used: u32,
        files_changed: usize,
        duration_ms: u64,
    },

    /// A wave has completed (all agents in wave done)
    #[serde(rename = "wave_done")]
    WaveDone {
        wave_index: u8,
        wave_label: String,
        agents_succeeded: usize,
        agents_failed: usize,
        duration_ms: u64,
    },

    /// Thinking output from an agent
    #[serde(rename = "wave_agent_thinking")]
    AgentThinking {
        agent_id: String,
        content: String,
    },

    /// File changed by an agent
    #[serde(rename = "wave_file_change")]
    FileChange {
        agent_id: String,
        path: String,
        change_type: String,
    },

    /// Pipeline completed
    #[serde(rename = "wave_pipeline_done")]
    PipelineDone {
        summary: WavePipelineSummary,
    },

    /// Error (fatal or non-fatal)
    #[serde(rename = "wave_error")]
    Error {
        agent_id: Option<String>,
        message: String,
        fatal: bool,
    },

    /// Human-friendly narration for non-technical users
    #[serde(rename = "narration")]
    Narration {
        message: String,
        progress_pct: u8,
    },
}

/// Summary of the entire wave pipeline execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WavePipelineSummary {
    pub task_id: String,
    pub status: String,
    pub waves_completed: Vec<u8>,
    pub total_agents_run: usize,
    pub agents_succeeded: usize,
    pub agents_failed: usize,
    pub total_iterations: u32,
    pub files_created: Vec<String>,
    pub files_modified: Vec<String>,
    pub total_tool_calls: u64,
    pub duration_ms: u64,
    pub agent_results: Vec<MiniAgentResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiniAgentResult {
    pub agent_id: String,
    pub agent_label: String,
    pub wave: u8,
    pub success: bool,
    pub summary: String,
    pub iterations_used: u32,
    pub files_changed: usize,
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// ConfigurableAgent — implements CodingAgent from MiniAgent config
// ---------------------------------------------------------------------------

struct ConfigurableAgent {
    config: MiniAgent,
}

#[async_trait::async_trait]
impl CodingAgent for ConfigurableAgent {
    fn role(&self) -> AgentRole {
        // Map mini-agent to closest existing role for backwards compat
        match self.config.wave {
            0 => AgentRole::Architect,   // analysis agents
            1 => AgentRole::Architect,   // planning agents
            2 => AgentRole::Coder,       // implementation agents
            3 => AgentRole::Reviewer,    // quality agents
            4 => AgentRole::Tester,      // test agents
            5 => AgentRole::Refactor,    // polish agents
            6 => AgentRole::DevOps,      // devops agents
            _ => AgentRole::Coder,
        }
    }

    fn name(&self) -> &str {
        &self.config.label
    }

    fn max_iterations(&self) -> u32 {
        self.config.max_iterations
    }

    fn tools_allowed(&self) -> Vec<&str> {
        self.config.tools.iter().map(|s| s.as_str()).collect()
    }

    fn system_prompt(&self, ctx: &CodingAgentContext) -> String {
        let mut prompt = format!(
            "You are the {} agent (ID: {}). {}\n\n{}\n",
            self.config.label,
            self.config.id,
            self.config.description,
            self.config.system_prompt,
        );

        if !ctx.code_graph_context.is_empty() {
            prompt.push_str(&format!("\n## CODE GRAPH\n{}\n", ctx.code_graph_context));
        }

        prompt
    }

    async fn execute(&self, ctx: &CodingAgentContext) -> anyhow::Result<AgentOutput> {
        run_coding_agent_loop(self, ctx).await
    }
}

// ---------------------------------------------------------------------------
// Wave Pipeline Runner
// ---------------------------------------------------------------------------

pub async fn run_wave_pipeline(
    config: WavePipelineConfig,
    app: Arc<AppState>,
    event_tx: mpsc::Sender<WaveEvent>,
) -> WavePipelineSummary {
    let start = Instant::now();

    // Build registry and waves
    let all_agents = mini_agent::build_registry();
    let waves = mini_agent::build_waves(&all_agents);
    let agent_map: HashMap<String, MiniAgent> = all_agents
        .into_iter()
        .map(|a| (a.id.clone(), a))
        .collect();

    // Filter waves
    let active_waves: Vec<&Wave> = waves
        .iter()
        .filter(|w| config.waves.as_ref().is_none_or(|wv| wv.contains(&w.index)))
        .collect();

    // Count total agents
    let total_agents: usize = active_waves
        .iter()
        .map(|w| {
            w.agents
                .iter()
                .filter(|id| !config.skip_agents.contains(id))
                .filter(|id| config.only_agents.as_ref().is_none_or(|o| o.contains(id)))
                .count()
        })
        .sum();

    let _ = event_tx.send(WaveEvent::PipelineStart {
        total_waves: active_waves.len(),
        total_agents,
    }).await;
    let _ = event_tx.send(WaveEvent::Narration {
        message: "Setting up your project...".into(),
        progress_pct: 5,
    }).await;

    // Publish to kernel event bus
    app.event_bus.publish(SystemEvent::new(
        "pipeline.wave.start",
        "pipeline_start",
        serde_json::json!({ "total_waves": active_waves.len(), "total_agents": total_agents }),
        EventSource::System { component: "wave_pipeline".into() },
    )).await;

    // Build shared context
    let brain = ProjectBrain::load_or_scan(&config.project_dir);
    let brain_context = brain.to_context();
    let code_graph = CodeGraph::load_or_build(&config.project_dir);
    let code_graph_context = build_code_graph_context(&code_graph);
    let memory_context = {
        let db = app.db.lock().await;
        let mem_svc = nexus_store::GlobalMemoryService::new(&db);
        mem_svc.to_context()
    };

    let workspace = CodingWorkspace::new(config.task.clone(), config.project_dir.clone());

    let mut all_results: Vec<MiniAgentResult> = Vec::new();
    let mut waves_completed: Vec<u8> = Vec::new();
    let mut total_iterations: u32 = 0;
    let mut pipeline_failed = false;

    // Run each wave
    for wave in &active_waves {
        if pipeline_failed && config.fail_fast {
            break;
        }

        // Filter agents for this wave
        let wave_agent_ids: Vec<&String> = wave
            .agents
            .iter()
            .filter(|id| !config.skip_agents.contains(id))
            .filter(|id| config.only_agents.as_ref().is_none_or(|o| o.contains(id)))
            .collect();

        if wave_agent_ids.is_empty() {
            continue;
        }

        let _ = event_tx.send(WaveEvent::WaveStart {
            wave_index: wave.index,
            wave_label: wave.label.clone(),
            agent_count: wave_agent_ids.len(),
            agent_ids: wave_agent_ids.iter().map(|s| s.to_string()).collect(),
        }).await;

        // Human-friendly narration per wave
        let narration = wave_narration(wave.index, active_waves.len());
        let progress = ((wave.index as f32 / active_waves.len() as f32) * 80.0 + 10.0) as u8;
        let _ = event_tx.send(WaveEvent::Narration {
            message: narration,
            progress_pct: progress,
        }).await;

        let wave_start = Instant::now();

        // Collect current workspace summary for agents in this wave
        let previous_wave_summary = {
            let state = workspace.state.read().await;
            let mut summary = String::new();
            if !state.context_summary.is_empty() {
                summary.push_str(&state.context_summary);
                summary.push('\n');
            }
            for r in &all_results {
                if r.success && !r.summary.is_empty() {
                    summary.push_str(&format!("[{}] {}\n", r.agent_id, truncate(&r.summary, 200)));
                }
            }
            if summary.is_empty() { None } else { Some(summary) }
        };

        // Spawn all agents in this wave concurrently
        let mut handles = Vec::new();

        for agent_id in &wave_agent_ids {
            let agent_config = match agent_map.get(*agent_id) {
                Some(c) => c.clone(),
                None => continue,
            };

            let _ = event_tx.send(WaveEvent::AgentStart {
                wave_index: wave.index,
                agent_id: agent_config.id.clone(),
                agent_label: agent_config.label.clone(),
                agent_icon: agent_config.icon.clone(),
                agent_color: agent_config.color.clone(),
            }).await;

            // Build a per-agent event forwarder
            let agent_event_tx = event_tx.clone();
            let agent_id_clone = agent_config.id.clone();

            // Build CodingEvent channel that we'll bridge to WaveEvent
            let (coding_tx, mut coding_rx) = mpsc::channel::<CodingEvent>(100);

            // Bridge task: convert CodingEvents to WaveEvents AND fan-out a
            // subset to the project-wide build_event_bus so Agent TV sees
            // live thought / tool / token activity.
            let bridge_tx = agent_event_tx.clone();
            let bridge_id = agent_id_clone.clone();
            let bridge_app = app.clone();
            let bridge_project_id = config.project_id.clone();
            tokio::spawn(async move {
                while let Some(event) = coding_rx.recv().await {
                    // Fan-out to Agent TV via build_event_bus — best-effort,
                    // no-op when no project_id is set (library callers).
                    if let Some(ref pid) = bridge_project_id {
                        match &event {
                            CodingEvent::Thinking { agent, content } => {
                                crate::handlers::live_build_handler::emit_agent_thought(
                                    &bridge_app,
                                    pid,
                                    agent.as_str(),
                                    content.clone(),
                                )
                                .await;
                            }
                            CodingEvent::ToolCall { agent, tool, arguments } => {
                                crate::handlers::live_build_handler::emit_agent_tool_call(
                                    &bridge_app,
                                    pid,
                                    agent.as_str(),
                                    tool,
                                    arguments,
                                )
                                .await;
                            }
                            CodingEvent::LlmUsage {
                                agent,
                                tokens_in,
                                tokens_out,
                                cost_usd,
                            } => {
                                crate::handlers::live_build_handler::emit_agent_tokens(
                                    &bridge_app,
                                    pid,
                                    agent.as_str(),
                                    *tokens_in,
                                    *tokens_out,
                                    *cost_usd,
                                )
                                .await;
                            }
                            _ => {}
                        }
                    }

                    let wave_event = match event {
                        CodingEvent::ToolCall { tool, arguments, .. } => {
                            Some(WaveEvent::AgentTool {
                                agent_id: bridge_id.clone(),
                                tool,
                                arguments,
                            })
                        }
                        CodingEvent::ToolResult { tool, success, output, duration_ms, .. } => {
                            Some(WaveEvent::AgentToolResult {
                                agent_id: bridge_id.clone(),
                                tool,
                                success,
                                output_preview: truncate(&output, 200),
                                duration_ms,
                            })
                        }
                        CodingEvent::Iteration { number, max, .. } => {
                            Some(WaveEvent::AgentIteration {
                                agent_id: bridge_id.clone(),
                                number,
                                max,
                            })
                        }
                        CodingEvent::Thinking { content, .. } => {
                            Some(WaveEvent::AgentThinking {
                                agent_id: bridge_id.clone(),
                                content,
                            })
                        }
                        CodingEvent::FileChange { change } => {
                            Some(WaveEvent::FileChange {
                                agent_id: bridge_id.clone(),
                                path: change.path,
                                change_type: format!("{:?}", change.change_type),
                            })
                        }
                        _ => None,
                    };
                    if let Some(we) = wave_event {
                        let _ = bridge_tx.send(we).await;
                    }
                }
            });

            let ctx = CodingAgentContext {
                workspace: workspace.clone(),
                llm_config: config.llm_config.clone(),
                event_tx: coding_tx,
                code_graph_context: code_graph_context.clone(),
                brain_context: brain_context.clone(),
                memory_context: memory_context.clone(),
                previous_agent_output: previous_wave_summary.clone(),
                cost_tracker: Some(app.cost_tracker.clone_ref()),
                project_id: config.project_id.clone(),
            };

            let agent = ConfigurableAgent { config: agent_config.clone() };
            let ws = workspace.clone();

            let handle = tokio::spawn(async move {
                let agent_start = Instant::now();
                let result = agent.execute(&ctx).await;
                let duration_ms = agent_start.elapsed().as_millis() as u64;

                match result {
                    Ok(output) => {
                        // Update shared workspace
                        {
                            let mut state = ws.state.write().await;
                            state.files_modified.extend(output.files_changed.clone());
                            state.decisions.extend(output.decisions.clone());
                            state.iteration_count += output.iterations_used;
                        }

                        MiniAgentResult {
                            agent_id: agent_config.id.clone(),
                            agent_label: agent_config.label.clone(),
                            wave: agent_config.wave,
                            success: true,
                            summary: output.summary,
                            iterations_used: output.iterations_used,
                            files_changed: output.files_changed.len(),
                            duration_ms,
                        }
                    }
                    Err(e) => {
                        MiniAgentResult {
                            agent_id: agent_config.id.clone(),
                            agent_label: agent_config.label.clone(),
                            wave: agent_config.wave,
                            success: false,
                            summary: format!("Failed: {}", e),
                            iterations_used: 0,
                            files_changed: 0,
                            duration_ms,
                        }
                    }
                }
            });

            handles.push((agent_id.to_string(), handle));
        }

        // Await all agents in this wave
        let mut wave_succeeded = 0usize;
        let mut wave_failed = 0usize;

        for (agent_id, handle) in handles {
            match handle.await {
                Ok(result) => {
                    let success = result.success;
                    let is_fatal = agent_map.get(&agent_id).is_some_and(|a| a.fatal);

                    let _ = event_tx.send(WaveEvent::AgentDone {
                        wave_index: wave.index,
                        agent_id: result.agent_id.clone(),
                        agent_label: result.agent_label.clone(),
                        success,
                        summary: truncate(&result.summary, 300),
                        iterations_used: result.iterations_used,
                        files_changed: result.files_changed,
                        duration_ms: result.duration_ms,
                    }).await;

                    total_iterations += result.iterations_used;

                    if success {
                        wave_succeeded += 1;
                    } else {
                        wave_failed += 1;
                        if is_fatal {
                            error!(agent = %agent_id, "Fatal agent failed — aborting pipeline");
                            let _ = event_tx.send(WaveEvent::Error {
                                agent_id: Some(agent_id.clone()),
                                message: format!("Fatal agent {} failed: {}", agent_id, result.summary),
                                fatal: true,
                            }).await;
                            pipeline_failed = true;
                        } else {
                            warn!(agent = %agent_id, "Non-fatal agent failed, continuing");
                            let _ = event_tx.send(WaveEvent::Error {
                                agent_id: Some(agent_id.clone()),
                                message: format!("{} failed: {}", agent_id, result.summary),
                                fatal: false,
                            }).await;
                        }
                    }

                    all_results.push(result);
                }
                Err(e) => {
                    wave_failed += 1;
                    warn!(agent = %agent_id, error = %e, "Agent panicked");
                    all_results.push(MiniAgentResult {
                        agent_id: agent_id.clone(),
                        agent_label: agent_id.clone(),
                        wave: wave.index,
                        success: false,
                        summary: format!("Panicked: {}", e),
                        iterations_used: 0,
                        files_changed: 0,
                        duration_ms: 0,
                    });
                }
            }
        }

        let wave_duration = wave_start.elapsed().as_millis() as u64;

        let _ = event_tx.send(WaveEvent::WaveDone {
            wave_index: wave.index,
            wave_label: wave.label.clone(),
            agents_succeeded: wave_succeeded,
            agents_failed: wave_failed,
            duration_ms: wave_duration,
        }).await;

        waves_completed.push(wave.index);

        info!(
            wave = wave.index,
            label = %wave.label,
            succeeded = wave_succeeded,
            failed = wave_failed,
            duration_ms = wave_duration,
            "Wave completed"
        );

        // Update workspace context summary with this wave's results
        {
            let mut state = workspace.state.write().await;
            let wave_summary: String = all_results
                .iter()
                .filter(|r| r.wave == wave.index && r.success)
                .map(|r| format!("[{}] {}", r.agent_id, truncate(&r.summary, 150)))
                .collect::<Vec<_>>()
                .join("\n");
            state.context_summary.push_str(&format!("\n## Wave {} ({})\n{}\n", wave.index, wave.label, wave_summary));
        }
    }

    // Build final summary
    let final_state = workspace.state.read().await;
    let duration_ms = start.elapsed().as_millis() as u64;

    let summary = WavePipelineSummary {
        task_id: config.task.id.clone(),
        status: if pipeline_failed { "failed".into() } else { "completed".into() },
        waves_completed,
        total_agents_run: all_results.len(),
        agents_succeeded: all_results.iter().filter(|r| r.success).count(),
        agents_failed: all_results.iter().filter(|r| !r.success).count(),
        total_iterations,
        files_created: final_state.files_modified.iter()
            .filter(|f| f.change_type == ChangeType::Created)
            .map(|f| f.path.clone()).collect(),
        files_modified: final_state.files_modified.iter()
            .filter(|f| f.change_type == ChangeType::Modified)
            .map(|f| f.path.clone()).collect(),
        total_tool_calls: 0, // tracked via events
        duration_ms,
        agent_results: all_results,
    };

    // Terminal event must not be silently dropped on backpressure.
    match tokio::time::timeout(
        std::time::Duration::from_secs(5),
        event_tx.send(WaveEvent::PipelineDone { summary: summary.clone() }),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(_closed)) => tracing::error!(
            "failed to deliver terminal PipelineDone event — channel closed; SSE client may hang"
        ),
        Err(_timeout) => tracing::error!(
            "timed out delivering terminal PipelineDone event — SSE client may hang"
        ),
    }

    // Publish completion to kernel event bus
    app.event_bus.publish(SystemEvent::new(
        "pipeline.wave.done",
        "pipeline_done",
        serde_json::json!({
            "status": &summary.status,
            "agents_succeeded": summary.agents_succeeded,
            "agents_failed": summary.agents_failed,
            "duration_ms": duration_ms,
        }),
        EventSource::System { component: "wave_pipeline".into() },
    )).await;

    // Persist decisions
    let mut brain = brain;
    for decision in &final_state.decisions {
        brain.record_decision(
            &decision.decision,
            &format!("Task: {} | Wave pipeline", config.task.description),
        );
    }
    brain.save(&config.project_dir);

    info!(
        task_id = %summary.task_id,
        status = %summary.status,
        agents_run = summary.total_agents_run,
        agents_ok = summary.agents_succeeded,
        agents_fail = summary.agents_failed,
        duration_ms = duration_ms,
        "Wave pipeline completed"
    );

    summary
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generate human-friendly narration messages for each wave.
fn wave_narration(wave_index: u8, total_waves: usize) -> String {
    // Map wave index to friendly descriptions
    match wave_index {
        0 => "Planning your app structure...".into(),
        1 => "Designing your pages and layout...".into(),
        2 => "Building your core features...".into(),
        3 => "Adding your data and content...".into(),
        4 => "Setting up user interactions...".into(),
        5 => "Polishing the design...".into(),
        6 => "Running quality checks...".into(),
        7 => "Final touches and optimization...".into(),
        n if n as usize >= total_waves.saturating_sub(1) => "Almost done — finishing up!".into(),
        _ => format!("Building... step {} of {}", wave_index + 1, total_waves),
    }
}

fn build_code_graph_context(graph: &CodeGraph) -> String {
    let mut ctx = String::new();
    ctx.push_str(&format!(
        "Files: {} | Symbols: {} | Dependencies: {}\n",
        graph.stats.total_files, graph.stats.total_symbols, graph.stats.total_edges,
    ));
    if !graph.stats.most_imported.is_empty() {
        ctx.push_str("Most imported:\n");
        for (file, count) in graph.stats.most_imported.iter().take(5) {
            ctx.push_str(&format!("  - {} ({})\n", file, count));
        }
    }
    ctx
}

/// Truncate `s` to `max` characters (not bytes). Byte-slicing panics when
/// LLM output contains emoji or CJK. See coding_agents::engine::truncate_chars.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push_str("...");
    out
}
