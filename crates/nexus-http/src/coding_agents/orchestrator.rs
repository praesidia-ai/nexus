//! Coding Agent Orchestrator — coordinates the multi-agent pipeline.
//!
//! The orchestrator manages the full lifecycle of a coding task with 10
//! specialized agents. It supports three pipeline modes:
//!
//! **Standard Pipeline** (5 agents):
//! ```text
//! Architect → Coder → Reviewer → Tester → Verify/Debug
//! ```
//!
//! **Full Pipeline** (10 agents):
//! ```text
//! Architect → Coder → [Reviewer + Performance + Refactor] → Tester → Verify/Debug
//!                         ↓ (parallel, non-fatal)
//!                    [UX + Product + DevOps]
//! ```
//!
//! **Single Agent** mode runs just one specialist on demand.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::mpsc;
use tracing::{info, warn, error};

use crate::code_graph::CodeGraph;
use crate::project_brain::ProjectBrain;
use crate::state::AppState;

use super::agents::architect::ArchitectAgent;
use super::agents::coder::CoderAgent;
use super::agents::debugger::DebuggerAgent;
use super::agents::devops::DevOpsAgent;
use super::agents::performance::PerformanceAgent;
use super::agents::product::ProductAgent;
use super::agents::refactor::RefactorAgent;
use super::agents::reviewer::ReviewerAgent;
use super::agents::tester::TesterAgent;
use super::agents::ux::UxAgent;
use super::traits::*;
use super::types::*;
use super::verification;

const MAX_DEBUG_RETRIES: u32 = 3;

/// Configuration for the coding orchestrator.
pub struct OrchestratorConfig {
    pub task: CodingTask,
    pub project_dir: PathBuf,
    pub llm_config: CodingLlmConfig,
    pub enable_review: bool,
    pub enable_test: bool,
    pub enable_verification: bool,
    pub enable_performance: bool,
    pub enable_ux: bool,
    pub enable_devops: bool,
    pub enable_refactor: bool,
    pub enable_product: bool,
    pub max_retries: u32,
    /// Project ID (forwarded to cost tracker for per-tenant budget attribution).
    pub project_id: Option<String>,
}

/// Shared context built once and reused across all agents in a pipeline run.
struct PipelineContext {
    workspace: Arc<CodingWorkspace>,
    llm_config: CodingLlmConfig,
    code_graph_context: String,
    brain_context: String,
    memory_context: String,
    cost_tracker: Option<crate::cost_intelligence::CostTrackerRef>,
    project_id: Option<String>,
}

/// Run the full coding agent pipeline.
pub async fn run_coding_pipeline(
    config: OrchestratorConfig,
    app: Arc<AppState>,
    event_tx: mpsc::Sender<CodingEvent>,
) -> TaskSummary {
    let start = Instant::now();
    let workspace = CodingWorkspace::new(config.task.clone(), config.project_dir.clone());

    let brain = ProjectBrain::load_or_scan(&config.project_dir);
    let brain_context = brain.to_context();

    let code_graph = CodeGraph::load_or_build(&config.project_dir);
    let code_graph_context = build_code_graph_context(&code_graph);

    let memory_context = {
        let db = app.db.lock().await;
        let mem_svc = nexus_store::GlobalMemoryService::new(&db);
        mem_svc.to_context()
    };

    let pctx = PipelineContext {
        workspace: workspace.clone(),
        llm_config: config.llm_config.clone(),
        code_graph_context,
        brain_context,
        memory_context,
        cost_tracker: Some(app.cost_tracker.clone_ref()),
        project_id: config.project_id.clone(),
    };

    let mut agent_stats: HashMap<String, AgentStats> = HashMap::new();
    let mut phases_completed: Vec<AgentPhase> = Vec::new();

    // ── Route based on mode ──────────────────────────────────────────────────
    match config.task.mode {
        CodingMode::PerformanceOnly => {
            return run_single_agent(
                &PerformanceAgent::new(), AgentPhase::Optimize, &pctx,
                &event_tx, &config, &brain, start,
            ).await;
        }
        CodingMode::UxOnly => {
            return run_single_agent(
                &UxAgent::new(), AgentPhase::Ux, &pctx,
                &event_tx, &config, &brain, start,
            ).await;
        }
        CodingMode::DevOpsOnly => {
            return run_single_agent(
                &DevOpsAgent::new(), AgentPhase::Deploy, &pctx,
                &event_tx, &config, &brain, start,
            ).await;
        }
        CodingMode::RefactorOnly => {
            return run_single_agent(
                &RefactorAgent::new(), AgentPhase::Refactor, &pctx,
                &event_tx, &config, &brain, start,
            ).await;
        }
        CodingMode::ProductOnly => {
            return run_single_agent(
                &ProductAgent::new(), AgentPhase::Product, &pctx,
                &event_tx, &config, &brain, start,
            ).await;
        }
        CodingMode::Architect => {
            return run_single_agent(
                &ArchitectAgent::new(), AgentPhase::Plan, &pctx,
                &event_tx, &config, &brain, start,
            ).await;
        }
        CodingMode::CoderOnly => {
            return run_single_agent(
                &CoderAgent::new(), AgentPhase::Implement, &pctx,
                &event_tx, &config, &brain, start,
            ).await;
        }
        CodingMode::ReviewOnly => {
            return run_single_agent(
                &ReviewerAgent::new(), AgentPhase::Review, &pctx,
                &event_tx, &config, &brain, start,
            ).await;
        }
        CodingMode::DebugOnly => {
            return run_single_agent(
                &DebuggerAgent::new(), AgentPhase::Debug, &pctx,
                &event_tx, &config, &brain, start,
            ).await;
        }
        // Pipeline and FullPipeline handled below
        _ => {}
    }

    // ══════════════════════════════════════════════════════════════════════════
    // PIPELINE MODE — sequential phases with optional parallel enhancement
    // ══════════════════════════════════════════════════════════════════════════

    // ── Phase 1: Architect (always runs) ─────────────────────────────────────
    let arch_output = match run_agent_phase(
        &ArchitectAgent::new(),
        AgentPhase::Plan,
        &pctx,
        &event_tx,
        None,
        &mut agent_stats,
    ).await {
        Ok(output) => {
            phases_completed.push(AgentPhase::Plan);
            output
        }
        Err(msg) => {
            emit_fatal_error(&event_tx, &msg).await;
            return build_failure_summary(&config.task, start, agent_stats);
        }
    };

    // ── Phase 2: Coder (always runs) ─────────────────────────────────────────
    let coder_output = match run_agent_phase(
        &CoderAgent::new(),
        AgentPhase::Implement,
        &pctx,
        &event_tx,
        Some(&arch_output.summary),
        &mut agent_stats,
    ).await {
        Ok(output) => {
            phases_completed.push(AgentPhase::Implement);
            output
        }
        Err(msg) => {
            emit_fatal_error(&event_tx, &msg).await;
            return build_failure_summary(&config.task, start, agent_stats);
        }
    };

    let last_summary = coder_output.summary.clone();

    // ── Phase 3: Quality Gate — Reviewer + optional parallel specialists ─────
    //
    // In FullPipeline mode, Reviewer, Performance, and Refactor run in parallel.
    // In standard Pipeline mode, only Reviewer runs (if enabled).

    let is_full = matches!(config.task.mode, CodingMode::FullPipeline);

    if config.enable_review || is_full {
        if is_full {
            // Parallel: Reviewer + Performance + Refactor
            run_parallel_quality_gate(
                &pctx,
                &event_tx,
                &last_summary,
                &mut agent_stats,
                &mut phases_completed,
                config.enable_review,
                config.enable_performance,
                config.enable_refactor,
            ).await;
        } else if config.enable_review {
            // Standard: just Reviewer
            match run_agent_phase(
                &ReviewerAgent::new(),
                AgentPhase::Review,
                &pctx,
                &event_tx,
                Some(&last_summary),
                &mut agent_stats,
            ).await {
                Ok(_) => { phases_completed.push(AgentPhase::Review); }
                Err(msg) => {
                    emit_nonfatal_error(&event_tx, &msg).await;
                }
            }
        }
    }

    // ── Phase 4: Test (optional) ─────────────────────────────────────────────
    if config.enable_test {
        match run_agent_phase(
            &TesterAgent::new(),
            AgentPhase::Test,
            &pctx,
            &event_tx,
            Some(&last_summary),
            &mut agent_stats,
        ).await {
            Ok(_) => { phases_completed.push(AgentPhase::Test); }
            Err(msg) => {
                emit_nonfatal_error(&event_tx, &msg).await;
            }
        }
    }

    // ── Phase 5: Verification + Debug Loop ───────────────────────────────────
    let verification_passed = if config.enable_verification {
        run_verification_loop(
            &pctx,
            &event_tx,
            &config,
            &brain,
            &mut agent_stats,
            &mut phases_completed,
        ).await
    } else {
        true
    };

    // ── Phase 6: Enhancement Gate (FullPipeline only) ────────────────────────
    //
    // UX, Product, and DevOps run in parallel after verification passes.
    // These are non-fatal enhancement agents.

    if is_full && verification_passed {
        run_parallel_enhancement_gate(
            &pctx,
            &event_tx,
            &last_summary,
            &mut agent_stats,
            &mut phases_completed,
            config.enable_ux,
            config.enable_product,
            config.enable_devops,
        ).await;
    }

    // ── Complete ─────────────────────────────────────────────────────────────
    let summary = build_success_summary(
        &config.task,
        &pctx.workspace,
        start,
        agent_stats,
        phases_completed,
        verification_passed,
    ).await;

    let _ = event_tx
        .send(CodingEvent::Complete { summary: summary.clone() })
        .await;

    // Persist decisions to Project Brain
    let mut brain = brain;
    for decision in &summary.decisions {
        brain.record_decision(
            &decision.decision,
            &format!("Task: {} | Phase: {:?}", config.task.description, decision.phase),
        );
    }
    brain.save(&config.project_dir);

    info!(
        task_id = %summary.task_id,
        status = %summary.status,
        files_changed = summary.files_created.len() + summary.files_modified.len(),
        duration_ms = summary.duration_ms,
        phases = ?summary.phases_completed,
        "Coding pipeline completed"
    );

    summary
}

// =============================================================================
// Single-Agent Execution
// =============================================================================

async fn run_single_agent(
    agent: &dyn CodingAgent,
    phase: AgentPhase,
    pctx: &PipelineContext,
    event_tx: &mpsc::Sender<CodingEvent>,
    config: &OrchestratorConfig,
    brain: &ProjectBrain,
    start: Instant,
) -> TaskSummary {
    let mut agent_stats: HashMap<String, AgentStats> = HashMap::new();

    let _ = event_tx
        .send(CodingEvent::PhaseChange {
            phase,
            agent: Some(agent.role()),
        })
        .await;

    let ctx = CodingAgentContext {
        workspace: pctx.workspace.clone(),
        llm_config: pctx.llm_config.clone(),
        event_tx: event_tx.clone(),
        code_graph_context: pctx.code_graph_context.clone(),
        brain_context: pctx.brain_context.clone(),
        memory_context: pctx.memory_context.clone(),
        previous_agent_output: None,
        cost_tracker: pctx.cost_tracker.clone(),
        project_id: pctx.project_id.clone(),
    };

    let (status, verification_passed) = match agent.execute(&ctx).await {
        Ok(output) => {
            record_agent_stats(&mut agent_stats, &output);
            update_workspace_from_output(&pctx.workspace, &output).await;
            ("completed".to_string(), true)
        }
        Err(e) => {
            error!(error = %e, agent = agent.name(), "Single agent failed");
            let _ = event_tx
                .send(CodingEvent::Error {
                    message: format!("{} failed: {}", agent.name(), e),
                    fatal: true,
                })
                .await;
            ("failed".to_string(), false)
        }
    };

    let summary = build_success_summary(
        &config.task,
        &pctx.workspace,
        start,
        agent_stats,
        vec![phase],
        verification_passed,
    ).await;

    let final_summary = TaskSummary { status, ..summary.clone() };

    let _ = event_tx
        .send(CodingEvent::Complete { summary: final_summary.clone() })
        .await;

    // Persist decisions
    let mut brain = brain.clone();
    for decision in &final_summary.decisions {
        brain.record_decision(
            &decision.decision,
            &format!("Task: {} | Phase: {:?}", config.task.description, decision.phase),
        );
    }
    brain.save(&config.project_dir);

    final_summary
}

// =============================================================================
// Agent Phase Runner
// =============================================================================

async fn run_agent_phase(
    agent: &dyn CodingAgent,
    phase: AgentPhase,
    pctx: &PipelineContext,
    event_tx: &mpsc::Sender<CodingEvent>,
    previous_output: Option<&str>,
    agent_stats: &mut HashMap<String, AgentStats>,
) -> Result<AgentOutput, String> {
    let _ = event_tx
        .send(CodingEvent::PhaseChange {
            phase,
            agent: Some(agent.role()),
        })
        .await;

    let ctx = CodingAgentContext {
        workspace: pctx.workspace.clone(),
        llm_config: pctx.llm_config.clone(),
        event_tx: event_tx.clone(),
        code_graph_context: pctx.code_graph_context.clone(),
        brain_context: pctx.brain_context.clone(),
        memory_context: pctx.memory_context.clone(),
        previous_agent_output: previous_output.map(|s| s.to_string()),
        cost_tracker: pctx.cost_tracker.clone(),
        project_id: pctx.project_id.clone(),
    };

    match agent.execute(&ctx).await {
        Ok(output) => {
            record_agent_stats(agent_stats, &output);
            update_workspace_from_output(&pctx.workspace, &output).await;
            Ok(output)
        }
        Err(e) => {
            error!(error = %e, agent = agent.name(), "Agent phase failed");
            Err(format!("{} failed: {}", agent.name(), e))
        }
    }
}

// =============================================================================
// Parallel Quality Gate — Reviewer + Performance + Refactor
// =============================================================================

#[allow(clippy::too_many_arguments)]
async fn run_parallel_quality_gate(
    pctx: &PipelineContext,
    event_tx: &mpsc::Sender<CodingEvent>,
    last_summary: &str,
    agent_stats: &mut HashMap<String, AgentStats>,
    phases_completed: &mut Vec<AgentPhase>,
    enable_review: bool,
    enable_performance: bool,
    enable_refactor: bool,
) {
    // Build contexts for each enabled agent
    let mut handles = Vec::new();

    if enable_review {
        let ctx = build_agent_context(pctx, event_tx, Some(last_summary));
        handles.push(("Reviewer", AgentPhase::Review, AgentRole::Reviewer, tokio::spawn(async move {
            let agent = ReviewerAgent::new();
            agent.execute(&ctx).await
        })));
    }

    if enable_performance {
        let ctx = build_agent_context(pctx, event_tx, Some(last_summary));
        handles.push(("Performance", AgentPhase::Optimize, AgentRole::Performance, tokio::spawn(async move {
            let agent = PerformanceAgent::new();
            agent.execute(&ctx).await
        })));
    }

    if enable_refactor {
        let ctx = build_agent_context(pctx, event_tx, Some(last_summary));
        handles.push(("Refactor", AgentPhase::Refactor, AgentRole::Refactor, tokio::spawn(async move {
            let agent = RefactorAgent::new();
            agent.execute(&ctx).await
        })));
    }

    // Emit phase start events for all parallel agents
    for (name, phase, role, _) in &handles {
        let _ = event_tx
            .send(CodingEvent::PhaseChange {
                phase: *phase,
                agent: Some(*role),
            })
            .await;
        info!(agent = *name, "Started parallel quality agent");
    }

    // Await all results
    for (name, phase, _role, handle) in handles {
        match handle.await {
            Ok(Ok(output)) => {
                record_agent_stats(agent_stats, &output);
                update_workspace_from_output(&pctx.workspace, &output).await;
                phases_completed.push(phase);
                info!(agent = name, "Parallel quality agent completed");
            }
            Ok(Err(e)) => {
                warn!(agent = name, error = %e, "Parallel quality agent failed (non-fatal)");
                let _ = event_tx
                    .send(CodingEvent::Error {
                        message: format!("{} failed: {}", name, e),
                        fatal: false,
                    })
                    .await;
            }
            Err(e) => {
                warn!(agent = name, error = %e, "Parallel quality agent panicked (non-fatal)");
            }
        }
    }
}

// =============================================================================
// Parallel Enhancement Gate — UX + Product + DevOps
// =============================================================================

#[allow(clippy::too_many_arguments)]
async fn run_parallel_enhancement_gate(
    pctx: &PipelineContext,
    event_tx: &mpsc::Sender<CodingEvent>,
    last_summary: &str,
    agent_stats: &mut HashMap<String, AgentStats>,
    phases_completed: &mut Vec<AgentPhase>,
    enable_ux: bool,
    enable_product: bool,
    enable_devops: bool,
) {
    let mut handles = Vec::new();

    if enable_ux {
        let ctx = build_agent_context(pctx, event_tx, Some(last_summary));
        handles.push(("UX", AgentPhase::Ux, AgentRole::Ux, tokio::spawn(async move {
            let agent = UxAgent::new();
            agent.execute(&ctx).await
        })));
    }

    if enable_product {
        let ctx = build_agent_context(pctx, event_tx, Some(last_summary));
        handles.push(("Product", AgentPhase::Product, AgentRole::Product, tokio::spawn(async move {
            let agent = ProductAgent::new();
            agent.execute(&ctx).await
        })));
    }

    if enable_devops {
        let ctx = build_agent_context(pctx, event_tx, Some(last_summary));
        handles.push(("DevOps", AgentPhase::Deploy, AgentRole::DevOps, tokio::spawn(async move {
            let agent = DevOpsAgent::new();
            agent.execute(&ctx).await
        })));
    }

    for (name, phase, role, _) in &handles {
        let _ = event_tx
            .send(CodingEvent::PhaseChange {
                phase: *phase,
                agent: Some(*role),
            })
            .await;
        info!(agent = *name, "Started parallel enhancement agent");
    }

    for (name, phase, _role, handle) in handles {
        match handle.await {
            Ok(Ok(output)) => {
                record_agent_stats(agent_stats, &output);
                update_workspace_from_output(&pctx.workspace, &output).await;
                phases_completed.push(phase);
                info!(agent = name, "Parallel enhancement agent completed");
            }
            Ok(Err(e)) => {
                warn!(agent = name, error = %e, "Enhancement agent failed (non-fatal)");
                let _ = event_tx
                    .send(CodingEvent::Error {
                        message: format!("{} failed: {}", name, e),
                        fatal: false,
                    })
                    .await;
            }
            Err(e) => {
                warn!(agent = name, error = %e, "Enhancement agent panicked (non-fatal)");
            }
        }
    }
}

// =============================================================================
// Verification + Debug Loop
// =============================================================================

async fn run_verification_loop(
    pctx: &PipelineContext,
    event_tx: &mpsc::Sender<CodingEvent>,
    config: &OrchestratorConfig,
    brain: &ProjectBrain,
    agent_stats: &mut HashMap<String, AgentStats>,
    phases_completed: &mut Vec<AgentPhase>,
) -> bool {
    let _ = event_tx
        .send(CodingEvent::PhaseChange {
            phase: AgentPhase::Verify,
            agent: None,
        })
        .await;

    let mut retry = 0u32;
    loop {
        let results = verification::run_verification_suite(
            &config.project_dir,
            &brain.stack,
        )
        .await;

        for r in &results {
            let _ = event_tx
                .send(CodingEvent::Verification { result: r.clone() })
                .await;
        }

        {
            let mut state = pctx.workspace.state.write().await;
            state.verification_results = results.clone();
        }

        let all_passed = results.iter().all(|r| r.passed);
        if all_passed {
            info!("All verification checks passed");
            phases_completed.push(AgentPhase::Verify);
            return true;
        }

        retry += 1;
        if retry > config.max_retries.min(MAX_DEBUG_RETRIES) {
            warn!(retries = retry, "Max debug retries exceeded");
            return false;
        }

        let _ = event_tx
            .send(CodingEvent::ErrorRecovery {
                error: format!(
                    "{} verification checks failed",
                    results.iter().filter(|r| !r.passed).count()
                ),
                strategy: "Invoking Debugger agent".to_string(),
                retry_count: retry,
            })
            .await;

        let error_context: String = results
            .iter()
            .filter(|r| !r.passed)
            .map(|r| {
                format!(
                    "## {:?} FAILED\n{}\n\nErrors:\n{}",
                    r.check_type,
                    r.output,
                    r.errors.join("\n"),
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        match run_agent_phase(
            &DebuggerAgent::new(),
            AgentPhase::Debug,
            pctx,
            event_tx,
            Some(&error_context),
            agent_stats,
        ).await {
            Ok(_) => {
                phases_completed.push(AgentPhase::Debug);
                let mut state = pctx.workspace.state.write().await;
                state.retry_count = retry;
            }
            Err(msg) => {
                warn!(error = %msg, "Debugger failed");
                return false;
            }
        }
    }
}

// =============================================================================
// Helpers
// =============================================================================

fn build_agent_context(
    pctx: &PipelineContext,
    event_tx: &mpsc::Sender<CodingEvent>,
    previous_output: Option<&str>,
) -> CodingAgentContext {
    CodingAgentContext {
        workspace: pctx.workspace.clone(),
        llm_config: pctx.llm_config.clone(),
        event_tx: event_tx.clone(),
        code_graph_context: pctx.code_graph_context.clone(),
        brain_context: pctx.brain_context.clone(),
        memory_context: pctx.memory_context.clone(),
        previous_agent_output: previous_output.map(|s| s.to_string()),
        cost_tracker: pctx.cost_tracker.clone(),
        project_id: pctx.project_id.clone(),
    }
}

fn build_code_graph_context(graph: &CodeGraph) -> String {
    let mut ctx = String::new();

    ctx.push_str(&format!(
        "Files: {} | Symbols: {} | Dependencies: {}\n",
        graph.stats.total_files, graph.stats.total_symbols, graph.stats.total_edges,
    ));

    if !graph.stats.most_imported.is_empty() {
        ctx.push_str("Most imported files (modify with caution):\n");
        for (file, count) in graph.stats.most_imported.iter().take(5) {
            ctx.push_str(&format!("  - {} (imported by {} files)\n", file, count));
        }
    }

    if !graph.stats.orphan_files.is_empty() {
        ctx.push_str(&format!(
            "Orphan files (no imports/exports): {}\n",
            graph.stats.orphan_files.len()
        ));
    }

    ctx
}

fn record_agent_stats(stats: &mut HashMap<String, AgentStats>, output: &AgentOutput) {
    let role = output.agent.as_str().to_string();
    let entry = stats.entry(role).or_insert_with(|| AgentStats {
        iterations: 0,
        tools_used: Vec::new(),
        files_touched: Vec::new(),
        duration_ms: 0,
    });
    entry.iterations += output.iterations_used;
    for change in &output.files_changed {
        if !entry.files_touched.contains(&change.path) {
            entry.files_touched.push(change.path.clone());
        }
    }
}

async fn update_workspace_from_output(workspace: &CodingWorkspace, output: &AgentOutput) {
    let mut state = workspace.state.write().await;
    state.files_modified.extend(output.files_changed.clone());
    state.decisions.extend(output.decisions.clone());
    state.errors.extend(output.errors.clone());
    state.iteration_count += output.iterations_used;
}

async fn emit_fatal_error(event_tx: &mpsc::Sender<CodingEvent>, msg: &str) {
    let _ = event_tx
        .send(CodingEvent::Error {
            message: msg.to_string(),
            fatal: true,
        })
        .await;
}

async fn emit_nonfatal_error(event_tx: &mpsc::Sender<CodingEvent>, msg: &str) {
    let _ = event_tx
        .send(CodingEvent::Error {
            message: msg.to_string(),
            fatal: false,
        })
        .await;
}

async fn build_success_summary(
    task: &CodingTask,
    workspace: &CodingWorkspace,
    start: Instant,
    agent_stats: HashMap<String, AgentStats>,
    phases_completed: Vec<AgentPhase>,
    verification_passed: bool,
) -> TaskSummary {
    let final_state = workspace.state.read().await;
    let duration_ms = start.elapsed().as_millis() as u64;

    TaskSummary {
        task_id: task.id.clone(),
        status: if verification_passed {
            "completed".to_string()
        } else {
            "completed_with_errors".to_string()
        },
        phases_completed,
        files_created: final_state
            .files_modified
            .iter()
            .filter(|f| f.change_type == ChangeType::Created)
            .map(|f| f.path.clone())
            .collect(),
        files_modified: final_state
            .files_modified
            .iter()
            .filter(|f| f.change_type == ChangeType::Modified)
            .map(|f| f.path.clone())
            .collect(),
        files_deleted: final_state
            .files_modified
            .iter()
            .filter(|f| f.change_type == ChangeType::Deleted)
            .map(|f| f.path.clone())
            .collect(),
        total_iterations: final_state.iteration_count,
        total_retries: final_state.retry_count,
        verification_passed,
        decisions: final_state.decisions.clone(),
        duration_ms,
        agent_stats,
    }
}

fn build_failure_summary(
    task: &CodingTask,
    start: Instant,
    agent_stats: HashMap<String, AgentStats>,
) -> TaskSummary {
    TaskSummary {
        task_id: task.id.clone(),
        status: "failed".to_string(),
        phases_completed: Vec::new(),
        files_created: Vec::new(),
        files_modified: Vec::new(),
        files_deleted: Vec::new(),
        total_iterations: 0,
        total_retries: 0,
        verification_passed: false,
        decisions: Vec::new(),
        duration_ms: start.elapsed().as_millis() as u64,
        agent_stats,
    }
}
