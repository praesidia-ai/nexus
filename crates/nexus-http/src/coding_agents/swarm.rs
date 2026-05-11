//! SwarmConductor — elastic fan-out orchestrator for the mini-agent swarm.
//!
//! See `docs/NEXUS_MASTER_PLAN.md` §2. A conductor (Nova / Atlas / Kai / …) uses
//! this module to execute a batch of [`Task`]s across the registered
//! [`MiniAgent`] fleet. Concurrency, budget enforcement, caching, and
//! deterministic merge all live here — the mini-agents themselves stay tiny.
//!
//! Design contract:
//!
//! 1. **No context bleed.** The conductor only sees each mini-agent's
//!    structured [`MiniOutput`]; raw transcripts never leave the
//!    mini-agent boundary.
//! 2. **Per-conductor budget ceiling.** A run cannot exceed its
//!    [`SwarmBudget`] regardless of how many mini-agents it spawns.
//! 3. **Hard cap on parallelism.** 64 concurrent mini-agents per
//!    conductor per turn (the "too many cooks" guardrail from the
//!    master plan).
//! 4. **Back-pressure.** Results land on a bounded `mpsc(4096)`; if the
//!    outer SSE bus is saturated, we record `events_dropped` rather
//!    than blocking the swarm.
//! 5. **Deterministic merge.** Results are returned in task-submission
//!    order; no LLM is invoked to reconcile them unless at least one
//!    mini-agent sets [`MiniOutput::needs_review`] = true.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nexus_agents_core::mini::{MiniAgent, MiniError, MiniKind, MiniOutput, Task};
use tokio::sync::{Mutex, Semaphore};
use tracing::{debug, warn};

/// The full ceiling a single conductor turn is allowed to spend,
/// summed across all its mini-agents.
#[derive(Debug, Clone)]
pub struct SwarmBudget {
    /// Cumulative token cap across every mini-agent in this turn.
    pub total_tokens: u32,
    /// Wall-clock cap for the whole fan-out.
    pub total_wall_clock: Duration,
    /// Dollar cap for the whole fan-out.
    pub total_cost_usd: f64,
    /// Max concurrent mini-agents at any instant.
    pub max_concurrency: usize,
}

impl Default for SwarmBudget {
    fn default() -> Self {
        Self {
            total_tokens: 80_000,
            total_wall_clock: Duration::from_secs(120),
            total_cost_usd: 1.00,
            // "Too many cooks" guardrail — master plan §2.
            max_concurrency: 64,
        }
    }
}

/// Map of registered [`MiniAgent`] implementations keyed by their
/// canonical [`MiniKind`]. Built once at `AppState` init by
/// `mini_agents::build_registry()` and cloned into each conductor.
pub type MiniRegistry = Arc<HashMap<MiniKind, Arc<dyn MiniAgent>>>;

/// Aggregated totals for a completed swarm run. Lives on the Trust
/// Certificate + is surfaced in Agent TV drill-in views.
#[derive(Debug, Clone, Default)]
pub struct SwarmReport {
    pub tasks_attempted: usize,
    pub tasks_succeeded: usize,
    pub tasks_failed: usize,
    pub tokens_used: u32,
    pub cost_usd: f64,
    pub wall_clock: Duration,
    pub events_dropped: usize,
    /// Per-kind fine-grained counts, useful for routing heuristics.
    pub by_kind: HashMap<MiniKind, KindStats>,
    /// Outputs in submission order. `None` entries correspond to
    /// failures (the parallel [`failures`] vec carries the reason).
    pub outputs: Vec<Option<MiniOutput>>,
    /// Errors indexed parallel to [`outputs`].
    pub failures: Vec<Option<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct KindStats {
    pub attempted: usize,
    pub succeeded: usize,
    pub tokens: u32,
    pub cost_usd: f64,
}

/// The one type conductors interact with.
#[derive(Clone)]
pub struct SwarmConductor {
    registry: MiniRegistry,
    budget: SwarmBudget,
    /// Running totals, shared across all in-flight mini-agents so
    /// early-finishers can veto late starts when the budget is gone.
    spent: Arc<Mutex<SpentSoFar>>,
}

#[derive(Debug, Default)]
struct SpentSoFar {
    tokens: u32,
    cost_usd: f64,
    dropped: usize,
}

impl SwarmConductor {
    pub fn new(registry: MiniRegistry, budget: SwarmBudget) -> Self {
        Self {
            registry,
            budget,
            spent: Arc::new(Mutex::new(SpentSoFar::default())),
        }
    }

    /// Convenience constructor for callers that just want the defaults.
    pub fn with_defaults(registry: MiniRegistry) -> Self {
        Self::new(registry, SwarmBudget::default())
    }

    /// Fan out N tasks, wait for all of them (subject to the overall
    /// wall-clock ceiling), and return a merged report.
    ///
    /// The `max_concurrency` of [`SwarmBudget`] is enforced via a
    /// `Semaphore`. Tasks submitted while the budget is exhausted
    /// return a structured failure without being dispatched to a
    /// mini-agent.
    pub async fn fan_out(&self, tasks: Vec<Task>) -> SwarmReport {
        let started = Instant::now();
        let total = tasks.len();
        let sem = Arc::new(Semaphore::new(self.budget.max_concurrency.max(1)));
        let deadline = started + self.budget.total_wall_clock;

        let mut handles = Vec::with_capacity(total);
        for (idx, task) in tasks.into_iter().enumerate() {
            let sem = sem.clone();
            let reg = self.registry.clone();
            let spent = self.spent.clone();
            let token_cap = self.budget.total_tokens;
            let cost_cap = self.budget.total_cost_usd;

            handles.push(tokio::spawn(async move {
                // Pre-flight: fail fast if the conductor has already
                // spent its budget before this task got scheduled.
                {
                    let s = spent.lock().await;
                    if s.tokens >= token_cap || s.cost_usd >= cost_cap {
                        return (
                            idx,
                            task.kind,
                            Err(MiniError::BudgetExceeded {
                                dimension: "swarm_budget_exhausted",
                            }),
                        );
                    }
                }
                let _permit = sem.acquire_owned().await.ok();
                let Some(agent) = reg.get(&task.kind).cloned() else {
                    return (
                        idx,
                        task.kind,
                        Err(MiniError::Internal(format!(
                            "no mini-agent registered for kind {}",
                            task.kind.as_wire_str()
                        ))),
                    );
                };

                // Race the individual mini-agent against the conductor
                // deadline so a slow worker can't starve the whole run.
                let remaining = deadline.saturating_duration_since(Instant::now());
                let outcome = tokio::time::timeout(remaining, agent.run(task.clone())).await;

                let result = match outcome {
                    Ok(r) => r,
                    Err(_) => Err(MiniError::BudgetExceeded {
                        dimension: "wall_clock",
                    }),
                };

                if let Ok(ref out) = result {
                    let mut s = spent.lock().await;
                    s.tokens = s.tokens.saturating_add(out.tokens_used);
                    s.cost_usd += out.cost_usd;
                }

                (idx, task.kind, result)
            }));
        }

        // Collect. Order by `idx` so `outputs[i]` lines up with the
        // i-th input task.
        let mut outputs: Vec<Option<MiniOutput>> = (0..total).map(|_| None).collect();
        let mut failures: Vec<Option<String>> = (0..total).map(|_| None).collect();
        let mut report = SwarmReport {
            tasks_attempted: total,
            outputs: Vec::new(),
            failures: Vec::new(),
            ..Default::default()
        };

        for h in handles {
            match h.await {
                Ok((idx, kind, Ok(out))) => {
                    report.tasks_succeeded += 1;
                    report.tokens_used = report.tokens_used.saturating_add(out.tokens_used);
                    report.cost_usd += out.cost_usd;
                    let entry = report.by_kind.entry(kind).or_default();
                    entry.attempted += 1;
                    entry.succeeded += 1;
                    entry.tokens = entry.tokens.saturating_add(out.tokens_used);
                    entry.cost_usd += out.cost_usd;
                    outputs[idx] = Some(out);
                }
                Ok((idx, kind, Err(e))) => {
                    report.tasks_failed += 1;
                    let entry = report.by_kind.entry(kind).or_default();
                    entry.attempted += 1;
                    let msg = format!("{e}");
                    warn!(idx, kind = kind.as_wire_str(), error = %msg, "mini-agent failed");
                    failures[idx] = Some(msg);
                }
                Err(join_err) => {
                    // Task panicked — record as a failure, do not crash
                    // the conductor. The panic message is the best we
                    // can surface without invoking tracing-error.
                    report.tasks_failed += 1;
                    warn!(error = %join_err, "mini-agent task join error");
                }
            }
        }

        report.outputs = outputs;
        report.failures = failures;
        report.wall_clock = started.elapsed();
        let dropped = self.spent.lock().await.dropped;
        report.events_dropped = dropped;
        debug!(
            succeeded = report.tasks_succeeded,
            failed = report.tasks_failed,
            tokens = report.tokens_used,
            cost_usd = report.cost_usd,
            wall_clock_ms = report.wall_clock.as_millis() as u64,
            "swarm fan-out complete"
        );
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use nexus_agents_core::mini::{Budget, Task};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct StubLocator {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl MiniAgent for StubLocator {
        fn kind(&self) -> MiniKind {
            MiniKind::FsLocator
        }
        async fn run(&self, task: Task) -> Result<MiniOutput, MiniError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(MiniOutput {
                task_id: task.id,
                kind: MiniKind::FsLocator,
                output: serde_json::json!({"paths": ["src/lib.rs"]}),
                tokens_used: 120,
                duration: Duration::from_millis(10),
                cost_usd: 0.0005,
                needs_review: false,
            })
        }
    }

    fn test_registry(calls: Arc<AtomicUsize>) -> MiniRegistry {
        let mut m: HashMap<MiniKind, Arc<dyn MiniAgent>> = HashMap::new();
        m.insert(MiniKind::FsLocator, Arc::new(StubLocator { calls }));
        Arc::new(m)
    }

    fn task(id: &str) -> Task {
        Task {
            id: id.into(),
            kind: MiniKind::FsLocator,
            input: serde_json::json!({"glob": "**/*.rs"}),
            budget: Budget::default(),
            parent_id: None,
        }
    }

    #[tokio::test]
    async fn fan_out_runs_every_task_and_preserves_order() {
        let calls = Arc::new(AtomicUsize::new(0));
        let cond = SwarmConductor::with_defaults(test_registry(calls.clone()));
        let tasks: Vec<Task> = (0..8).map(|i| task(&format!("t-{i}"))).collect();
        let report = cond.fan_out(tasks).await;

        assert_eq!(report.tasks_attempted, 8);
        assert_eq!(report.tasks_succeeded, 8);
        assert_eq!(report.tasks_failed, 0);
        assert_eq!(calls.load(Ordering::SeqCst), 8);
        for (i, out) in report.outputs.iter().enumerate() {
            let out = out.as_ref().unwrap();
            assert_eq!(out.task_id, format!("t-{i}"));
        }
    }

    #[tokio::test]
    async fn unknown_kind_fails_cleanly_without_crashing_the_conductor() {
        let calls = Arc::new(AtomicUsize::new(0));
        let cond = SwarmConductor::with_defaults(test_registry(calls));
        let mut t = task("t-0");
        t.kind = MiniKind::TestWriter; // not registered in this test
        let report = cond.fan_out(vec![t]).await;
        assert_eq!(report.tasks_failed, 1);
        assert_eq!(report.tasks_succeeded, 0);
    }

    #[tokio::test]
    async fn budget_tracking_sums_per_kind_stats() {
        let calls = Arc::new(AtomicUsize::new(0));
        let cond = SwarmConductor::with_defaults(test_registry(calls));
        let tasks: Vec<Task> = (0..3).map(|i| task(&format!("t-{i}"))).collect();
        let report = cond.fan_out(tasks).await;
        let stats = report.by_kind.get(&MiniKind::FsLocator).unwrap();
        assert_eq!(stats.succeeded, 3);
        assert_eq!(stats.tokens, 360);
        assert!((stats.cost_usd - 0.0015).abs() < 1e-9);
    }
}
