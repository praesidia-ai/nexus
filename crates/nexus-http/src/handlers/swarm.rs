//! `/projects/:id/swarm/run` — the first end-to-end swarm endpoint.
//!
//! Accepts a batch of [`Task`]s, dispatches them through the
//! canonical [`SwarmConductor`] wired up with the default
//! mini-agent [`MiniRegistry`], persists every step into
//! `agent_tv_events`, and returns a structured [`SwarmReport`]
//! plus the replayable `run_id` so the caller can embed / share /
//! drill-in via Agent TV.
//!
//! This is the seam where the swarm architecture becomes visible
//! to external clients. Future oneshot / wave pipelines will build
//! on top of this handler — swapping the hand-written task list
//! for a conductor-planned one.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use nexus_agents_core::mini::{Budget, MiniKind, Task};
use serde::Deserialize;
use serde_json::json;

use crate::{
    agent_tv_sink,
    coding_agents::swarm::{SwarmBudget, SwarmConductor},
    error::{ApiError, ApiResult},
    mini_agents::build_registry,
    security::{auth::Scope, project_access::ProjectAccess},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct RunRequest {
    /// The prompt or human-readable label for this run. Persisted onto
    /// `agent_tv_runs.prompt` for the gallery + replay views.
    pub prompt: Option<String>,
    /// Tasks to fan out across the swarm. Each task specifies a
    /// canonical [`MiniKind`] and an opaque JSON input whose shape is
    /// defined by the kind's implementation.
    pub tasks: Vec<TaskRequest>,
    /// Optional override of the default [`SwarmBudget`] — callers on
    /// a budget can tighten the cap without changing server config.
    #[serde(default)]
    pub budget: Option<BudgetRequest>,
}

#[derive(Debug, Deserialize)]
pub struct TaskRequest {
    pub kind: MiniKind,
    pub input: serde_json::Value,
    #[serde(default)]
    pub parent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BudgetRequest {
    pub total_tokens: Option<u32>,
    pub total_wall_clock_secs: Option<u64>,
    pub total_cost_usd: Option<f64>,
    pub max_concurrency: Option<usize>,
}

impl BudgetRequest {
    fn merge_into(self, base: SwarmBudget) -> SwarmBudget {
        SwarmBudget {
            total_tokens: self.total_tokens.unwrap_or(base.total_tokens),
            total_wall_clock: self
                .total_wall_clock_secs
                .map(std::time::Duration::from_secs)
                .unwrap_or(base.total_wall_clock),
            total_cost_usd: self.total_cost_usd.unwrap_or(base.total_cost_usd),
            max_concurrency: self.max_concurrency.unwrap_or(base.max_concurrency),
        }
    }
}

/// `POST /projects/:id/swarm/run` — fan out a task batch through the
/// canonical swarm and return a replayable summary.
#[tracing::instrument(skip(app, body))]
pub async fn run_swarm(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Path(project_id): Path<String>,
    Json(body): Json<RunRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("Missing scope: project:write".into()))?;

    if body.tasks.is_empty() {
        return Err(ApiError::BadRequest("tasks must not be empty".into()));
    }
    if body.tasks.len() > 256 {
        return Err(ApiError::BadRequest(
            "swarm batch capped at 256 tasks per run".into(),
        ));
    }

    // Per-tenant + global LLM concurrency. A swarm run can fan out to up to
    // 256 mini-agent tasks, each potentially driving an LLM call — without a
    // slot one tenant can saturate the entire generation budget.
    if let Err(reason) = app.tenant_rate_limiter.check(&access.tenant_id, "agent") {
        return Err(ApiError::TooManyRequests(reason));
    }
    let _llm_slot = app
        .rate_limiter
        .acquire_llm_slot()
        .await
        .map_err(|e| ApiError::TooManyRequests(format!("swarm queue is full: {e}")))?;

    // Build a conductor scoped to this project's workspace. The
    // mini-agent registry encodes the fs-sandbox root, so spawning
    // one per request keeps projects hermetic from each other.
    let project_root = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");
    tokio::fs::create_dir_all(&project_root)
        .await
        .map_err(|e| ApiError::Internal(format!("mkdir project root: {e}")))?;
    let registry = build_registry(project_root.clone());
    let budget = body
        .budget
        .map(|b| b.merge_into(SwarmBudget::default()))
        .unwrap_or_default();
    let conductor = SwarmConductor::new(registry, budget);

    // Start a replayable run. Persistence is best-effort: a SQL
    // failure here shouldn't abort the swarm — we fall back to a
    // synthetic run-id so the caller can still reason about the
    // result shape.
    let run_id =
        match agent_tv_sink::start_run(&app, &project_id, "swarm", body.prompt.clone()).await {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!(error = %e, "start_run failed, using synthetic id");
                uuid::Uuid::new_v4().to_string()
            }
        };

    // Announce the run + seed task list on the event log so a
    // replay can reconstruct what was asked of the swarm.
    agent_tv_sink::record_event(
        &app,
        &run_id,
        "swarm_started",
        None,
        None,
        &json!({
            "prompt": body.prompt,
            "task_count": body.tasks.len(),
        }),
    )
    .await;

    // Ensure the project has a CLAUDE.md on disk and record the fact
    // that it was injected into this run's system-prompt chain.
    // Mini-agents that end up making an LLM call in a follow-up phase
    // will run `claude_md_injector::inject_for_project` against the
    // same root and see the same bootstrap / stored document.
    let project_name = {
        let db = app.db.lock().await;
        db.query_row(
            "SELECT name FROM projects WHERE id = ?1",
            [&project_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_else(|_| project_id.clone())
    };
    if let Ok(md) = crate::claude_md::load(&project_root, &project_name).await {
        // `save` is idempotent + atomic — we always write so that
        // replays see a deterministic CLAUDE.md even if the user
        // hadn't touched it.
        let _ = crate::claude_md::save(&project_root, &md).await;
        agent_tv_sink::record_event(
            &app,
            &run_id,
            "claude_md_injected",
            None,
            None,
            &json!({
                "sections": md.sections.len(),
                "title": md.title,
            }),
        )
        .await;
    }

    // Convert the request-level Tasks to the internal Task shape.
    // Budgets default to the per-kind narrow budget; request-level
    // budget overrides apply to the outer SwarmBudget, not per-task.
    let tasks: Vec<Task> = body
        .tasks
        .into_iter()
        .enumerate()
        .map(|(i, t)| Task {
            id: format!("{run_id}-{i}"),
            kind: t.kind,
            input: t.input,
            budget: Budget::default(),
            parent_id: t.parent_id,
        })
        .collect();

    let start = std::time::Instant::now();
    let report = conductor.fan_out(tasks).await;
    let duration_ms = start.elapsed().as_millis() as i64;

    // Emit one event per finished (or failed) mini-agent so the
    // Agent TV scrubber can animate the fan-out.
    for (idx, out) in report.outputs.iter().enumerate() {
        if let Some(o) = out {
            agent_tv_sink::record_event(
                &app,
                &run_id,
                "mini_complete",
                None,
                Some(o.kind.as_wire_str()),
                &json!({
                    "seq": idx,
                    "task_id": o.task_id,
                    "tokens": o.tokens_used,
                    "cost_usd": o.cost_usd,
                    "duration_ms": o.duration.as_millis() as u64,
                    "needs_review": o.needs_review,
                }),
            )
            .await;
        } else if let Some(err_msg) = report.failures.get(idx).and_then(|x| x.as_ref()) {
            agent_tv_sink::record_event(
                &app,
                &run_id,
                "mini_failed",
                None,
                None,
                &json!({ "seq": idx, "error": err_msg }),
            )
            .await;
        }
    }

    // Terminal event.
    let status = if report.tasks_failed == 0 {
        "completed"
    } else if report.tasks_succeeded == 0 {
        "failed"
    } else {
        "partial"
    };
    agent_tv_sink::record_event(
        &app,
        &run_id,
        "complete",
        None,
        None,
        &json!({
            "status": status,
            "tasks_succeeded": report.tasks_succeeded,
            "tasks_failed": report.tasks_failed,
            "tokens_used": report.tokens_used,
            "cost_usd": report.cost_usd,
        }),
    )
    .await;

    if let Err(e) = agent_tv_sink::complete_run(
        &app,
        &run_id,
        status,
        report.tokens_used as i64,
        report.cost_usd,
        duration_ms,
    )
    .await
    {
        tracing::warn!(error = %e, "complete_run failed");
    }

    // Shape the response for the frontend / SDK. Mini-agent outputs
    // are included inline so a caller who wanted synchronous results
    // can avoid a second round-trip to `/tv/:runId`.
    let outputs: Vec<serde_json::Value> = report
        .outputs
        .iter()
        .enumerate()
        .map(|(i, o)| match o {
            Some(out) => json!({
                "seq": i,
                "task_id": out.task_id,
                "kind": out.kind.as_wire_str(),
                "output": out.output,
                "tokens_used": out.tokens_used,
                "cost_usd": out.cost_usd,
                "needs_review": out.needs_review,
                "duration_ms": out.duration.as_millis() as u64,
            }),
            None => json!({
                "seq": i,
                "error": report
                    .failures
                    .get(i)
                    .and_then(|f| f.as_ref())
                    .cloned()
                    .unwrap_or_default(),
            }),
        })
        .collect();

    Ok(Json(json!({
        "run_id": run_id,
        "replay_url": format!("/tv/{run_id}"),
        "embed_url": format!("/tv/{run_id}/embed"),
        "status": status,
        "tasks_attempted": report.tasks_attempted,
        "tasks_succeeded": report.tasks_succeeded,
        "tasks_failed": report.tasks_failed,
        "tokens_used": report.tokens_used,
        "cost_usd": report.cost_usd,
        "duration_ms": duration_ms,
        "by_kind": report
            .by_kind
            .iter()
            .map(|(k, s)| {
                (
                    k.as_wire_str().to_string(),
                    json!({
                        "attempted": s.attempted,
                        "succeeded": s.succeeded,
                        "tokens": s.tokens,
                        "cost_usd": s.cost_usd,
                    }),
                )
            })
            .collect::<serde_json::Map<String, serde_json::Value>>(),
        "outputs": outputs,
    })))
}
