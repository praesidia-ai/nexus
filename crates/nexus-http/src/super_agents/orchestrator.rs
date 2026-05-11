//! Performance Orchestrator — schedules, coordinates, and deconflicts Super Agents.
//!
//! The orchestrator is the central brain of the performance optimization system.
//! It runs as a long-lived tokio task and:
//!
//! 1. Polls metrics to decide which agents should run
//! 2. Checks conflict groups to avoid concurrent resource contention
//! 3. Executes the 3-phase lifecycle: analyze -> optimize -> validate
//! 4. Rolls back failed optimizations
//! 5. Records all activity for observability

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use uuid::Uuid;

use super::metrics_bus::MetricsBus;
use super::traits::*;
use super::types::*;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Orchestrator State
// ---------------------------------------------------------------------------

pub struct OrchestratorState {
    /// When each agent last ran.
    pub last_run: HashMap<SuperAgentKind, Instant>,
    /// Currently executing agents (for conflict detection).
    pub running: HashSet<SuperAgentKind>,
    /// Recent run history.
    pub history: Vec<AgentRunRecord>,
    /// Current operating mode.
    pub mode: OrchestratorMode,
    /// Whether the orchestrator is paused.
    pub paused: bool,
    /// Total cycles completed.
    pub cycles: u64,
}

impl Default for OrchestratorState {
    fn default() -> Self {
        Self {
            last_run: HashMap::new(),
            running: HashSet::new(),
            history: Vec::new(),
            mode: OrchestratorMode::Conservative,
            paused: false,
            cycles: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Performance Orchestrator
// ---------------------------------------------------------------------------

pub struct PerformanceOrchestrator {
    app: Arc<AppState>,
    metrics: Arc<MetricsBus>,
    agents: Vec<Box<dyn SuperAgent>>,
    state: Arc<RwLock<OrchestratorState>>,
}

/// Ensures a super-agent's `running` flag is cleared even if the lifecycle
/// future panics. Happy path calls `std::mem::forget` so normal execution
/// skips the Drop spawn.
struct RunningGuard {
    state: Arc<RwLock<OrchestratorState>>,
    kind: SuperAgentKind,
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        let state = self.state.clone();
        let kind = self.kind;
        // We can't await in Drop; spawn a detached task. The removal is
        // idempotent and contention-safe.
        tokio::spawn(async move {
            let mut s = state.write().await;
            s.running.remove(&kind);
        });
    }
}

impl PerformanceOrchestrator {
    pub fn new(
        app: Arc<AppState>,
        metrics: Arc<MetricsBus>,
        agents: Vec<Box<dyn SuperAgent>>,
    ) -> Self {
        Self {
            app,
            metrics,
            agents,
            state: Arc::new(RwLock::new(OrchestratorState::default())),
        }
    }

    /// Start the orchestrator loop. Runs until the server shuts down.
    pub async fn run(self: Arc<Self>) {
        info!(
            agent_count = self.agents.len(),
            "Performance Orchestrator started"
        );

        let tick_interval = Duration::from_secs(30);

        loop {
            tokio::time::sleep(tick_interval).await;

            {
                let state = self.state.read().await;
                if state.paused {
                    continue;
                }
            }

            if let Err(e) = self.tick().await {
                error!(error = %e, "Orchestrator tick failed");
            }
        }
    }

    /// Single orchestrator tick — determine which agents should run and execute them.
    async fn tick(&self) -> anyhow::Result<()> {
        let snapshot = self.metrics.snapshot().await;
        let mode = {
            let state = self.state.read().await;
            state.mode
        };

        // Collect agents that should fire this tick
        let mut candidates: Vec<(usize, u8)> = Vec::new(); // (agent_index, priority)
        let now = Instant::now();

        for (idx, agent) in self.agents.iter().enumerate() {
            let state = self.state.read().await;

            // Skip if already running
            if state.running.contains(&agent.kind()) {
                continue;
            }

            // Check if agent should fire
            let should_fire = match agent.trigger() {
                TriggerMode::Periodic { interval } => {
                    state
                        .last_run
                        .get(&agent.kind())
                        .is_none_or(|last| now.duration_since(*last) >= interval)
                }
                TriggerMode::Threshold { ref metric, value } => {
                    self.metrics.latest(metric).await >= value
                }
                TriggerMode::Event { .. } => false, // event-driven agents are triggered explicitly
                TriggerMode::Hybrid {
                    interval,
                    ref metric,
                    value,
                } => {
                    let time_trigger = state
                        .last_run
                        .get(&agent.kind())
                        .is_none_or(|last| now.duration_since(*last) >= interval);
                    let metric_trigger = self.metrics.latest(metric).await >= value;
                    time_trigger || metric_trigger
                }
            };

            // Check risk level against mode
            let risk_ok = match mode {
                OrchestratorMode::Observe => agent.risk_level() <= Severity::Info,
                OrchestratorMode::Conservative => agent.risk_level() <= Severity::Low,
                OrchestratorMode::Balanced => agent.risk_level() <= Severity::Medium,
                OrchestratorMode::Aggressive => true,
            };

            if should_fire && risk_ok {
                candidates.push((idx, agent.priority()));
            }
        }

        if candidates.is_empty() {
            let mut state = self.state.write().await;
            state.cycles += 1;
            return Ok(());
        }

        // Sort by priority (highest first)
        candidates.sort_by(|a, b| b.1.cmp(&a.1));

        // Execute non-conflicting agents
        let mut active_groups: HashSet<ConflictGroup> = HashSet::new();

        for (idx, _priority) in &candidates {
            let agent = &self.agents[*idx];

            // Check conflict groups
            let groups = agent.conflict_groups();
            let conflicts = groups.iter().any(|g| active_groups.contains(g));
            if conflicts {
                continue;
            }

            // Mark groups as active
            for g in groups {
                active_groups.insert(*g);
            }

            // Mark agent as running. If `execute_agent_lifecycle` panics,
            // `RunningGuard`'s Drop re-acquires the write lock from a spawned
            // task and removes the entry — preventing the previously-observed
            // permanent-lockout where a panicked agent could never run again.
            let kind = agent.kind();
            {
                let mut state = self.state.write().await;
                state.running.insert(kind);
            }
            let running_guard = RunningGuard {
                state: self.state.clone(),
                kind,
            };

            // Execute the agent lifecycle
            let record = self
                .execute_agent_lifecycle(agent.as_ref(), &snapshot, mode)
                .await;

            // Normal-path release + bookkeeping. We explicitly forget the
            // guard so Drop doesn't double-remove (the remove is idempotent
            // anyway, but avoiding the extra spawn is cheaper).
            {
                let mut state = self.state.write().await;
                state.running.remove(&kind);
                state.last_run.insert(kind, Instant::now());

                // Keep history bounded
                state.history.push(record);
                if state.history.len() > 500 {
                    state.history.drain(..100);
                }
            }
            std::mem::forget(running_guard);

            // Release conflict groups
            for g in groups {
                active_groups.remove(g);
            }
        }

        {
            let mut state = self.state.write().await;
            state.cycles += 1;
        }

        Ok(())
    }

    /// Execute the full 3-phase lifecycle for a single agent.
    async fn execute_agent_lifecycle(
        &self,
        agent: &dyn SuperAgent,
        snapshot: &SystemSnapshot,
        mode: OrchestratorMode,
    ) -> AgentRunRecord {
        let started_at = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        info!(
            agent = agent.name(),
            mode = ?mode,
            "Running super agent"
        );

        // Phase 1: Analyze
        let analysis_ctx = AnalysisContext {
            app: self.app.clone(),
            metrics: self.metrics.clone(),
            snapshot: snapshot.clone(),
            mode,
        };

        let report = match agent.analyze(&analysis_ctx).await {
            Ok(r) => r,
            Err(e) => {
                error!(agent = agent.name(), error = %e, "Analysis failed");
                return AgentRunRecord {
                    id,
                    agent_kind: agent.kind(),
                    started_at,
                    finished_at: Utc::now().to_rfc3339(),
                    mode,
                    findings_count: 0,
                    optimizations_applied: 0,
                    total_improvement_pct: 0.0,
                    validation_passed: false,
                    error: Some(e.to_string()),
                };
            }
        };

        let findings_count = report.findings.len() as u32;
        let actionable = report.actionable_findings().len();

        info!(
            agent = agent.name(),
            findings = findings_count,
            actionable = actionable,
            scan_ms = report.scan_duration_ms,
            "Analysis complete"
        );

        // Record findings in metrics bus
        self.metrics
            .record_value(
                &format!("super_agent.{}.findings", agent.kind().as_str()),
                findings_count as f64,
            )
            .await;

        if actionable == 0 || mode == OrchestratorMode::Observe {
            return AgentRunRecord {
                id,
                agent_kind: agent.kind(),
                started_at,
                finished_at: Utc::now().to_rfc3339(),
                mode,
                findings_count,
                optimizations_applied: 0,
                total_improvement_pct: 0.0,
                validation_passed: true,
                error: None,
            };
        }

        // Phase 2: Optimize
        let opt_ctx = OptimizationContext {
            app: self.app.clone(),
            metrics: self.metrics.clone(),
            mode,
            dry_run: mode == OrchestratorMode::Observe,
        };

        let result = match agent.optimize(&opt_ctx, &report).await {
            Ok(r) => r,
            Err(e) => {
                error!(agent = agent.name(), error = %e, "Optimization failed");
                return AgentRunRecord {
                    id,
                    agent_kind: agent.kind(),
                    started_at,
                    finished_at: Utc::now().to_rfc3339(),
                    mode,
                    findings_count,
                    optimizations_applied: 0,
                    total_improvement_pct: 0.0,
                    validation_passed: false,
                    error: Some(e.to_string()),
                };
            }
        };

        let optimizations_applied = result.optimizations.len() as u32;
        let total_improvement = result.total_improvement_pct;

        info!(
            agent = agent.name(),
            optimizations = optimizations_applied,
            improvement_pct = format!("{:.1}", total_improvement),
            duration_ms = result.duration_ms,
            "Optimization complete"
        );

        // Record optimization metrics
        self.metrics
            .record_value(
                &format!("super_agent.{}.improvement_pct", agent.kind().as_str()),
                total_improvement,
            )
            .await;

        // Phase 3: Validate
        let val_ctx = ValidationContext {
            app: self.app.clone(),
            metrics: self.metrics.clone(),
            snapshot_before: snapshot.clone(),
        };

        let validation = match agent.validate(&val_ctx).await {
            Ok(v) => v,
            Err(e) => {
                warn!(agent = agent.name(), error = %e, "Validation failed — attempting rollback");

                // Rollback all optimizations
                for opt in &result.optimizations {
                    if let Some(ref key) = opt.rollback_key {
                        if let Err(re) = agent.rollback(&self.app, key).await {
                            error!(
                                agent = agent.name(),
                                key = key,
                                error = %re,
                                "Rollback failed"
                            );
                        }
                    }
                }

                return AgentRunRecord {
                    id,
                    agent_kind: agent.kind(),
                    started_at,
                    finished_at: Utc::now().to_rfc3339(),
                    mode,
                    findings_count,
                    optimizations_applied,
                    total_improvement_pct: 0.0,
                    validation_passed: false,
                    error: Some(e.to_string()),
                };
            }
        };

        if !validation.passed {
            warn!(
                agent = agent.name(),
                failures = ?validation.failures,
                "Validation failed — rolling back"
            );

            for opt in &result.optimizations {
                if let Some(ref key) = opt.rollback_key {
                    let _ = agent.rollback(&self.app, key).await;
                }
            }
        }

        info!(
            agent = agent.name(),
            passed = validation.passed,
            checks = validation.checks_run,
            "Validation complete"
        );

        AgentRunRecord {
            id,
            agent_kind: agent.kind(),
            started_at,
            finished_at: Utc::now().to_rfc3339(),
            mode,
            findings_count,
            optimizations_applied,
            total_improvement_pct: if validation.passed {
                total_improvement
            } else {
                0.0
            },
            validation_passed: validation.passed,
            error: if validation.passed {
                None
            } else {
                Some(format!("Validation failures: {:?}", validation.failures))
            },
        }
    }

    // -----------------------------------------------------------------------
    // Public API for handlers
    // -----------------------------------------------------------------------

    /// Get current orchestrator status.
    pub async fn status(&self) -> OrchestratorStatus {
        let state = self.state.read().await;
        OrchestratorStatus {
            mode: state.mode,
            paused: state.paused,
            cycles: state.cycles,
            agents_registered: self.agents.len(),
            agents_running: state.running.len(),
            running_agents: state.running.iter().map(|k| k.as_str().to_string()).collect(),
            total_optimizations: state
                .history
                .iter()
                .map(|r| r.optimizations_applied as u64)
                .sum(),
            total_findings: state
                .history
                .iter()
                .map(|r| r.findings_count as u64)
                .sum(),
            avg_improvement_pct: {
                let successful: Vec<f64> = state
                    .history
                    .iter()
                    .filter(|r| r.validation_passed && r.total_improvement_pct > 0.0)
                    .map(|r| r.total_improvement_pct)
                    .collect();
                if successful.is_empty() {
                    0.0
                } else {
                    successful.iter().sum::<f64>() / successful.len() as f64
                }
            },
            uptime_secs: self.metrics.uptime_secs(),
        }
    }

    /// Get recent run history.
    pub async fn history(&self, limit: usize) -> Vec<AgentRunRecord> {
        let state = self.state.read().await;
        state
            .history
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Change operating mode.
    pub async fn set_mode(&self, mode: OrchestratorMode) {
        let mut state = self.state.write().await;
        info!(
            old_mode = ?state.mode,
            new_mode = ?mode,
            "Orchestrator mode changed"
        );
        state.mode = mode;
    }

    /// Pause/resume the orchestrator.
    pub async fn set_paused(&self, paused: bool) {
        let mut state = self.state.write().await;
        state.paused = paused;
        info!(paused = paused, "Orchestrator pause state changed");
    }

    /// Manually trigger a specific agent.
    pub async fn trigger_agent(&self, kind: SuperAgentKind) -> Option<AgentRunRecord> {
        let agent = self.agents.iter().find(|a| a.kind() == kind)?;
        let snapshot = self.metrics.snapshot().await;
        let mode = {
            let state = self.state.read().await;
            state.mode
        };

        let record = self
            .execute_agent_lifecycle(agent.as_ref(), &snapshot, mode)
            .await;

        {
            let mut state = self.state.write().await;
            state.last_run.insert(kind, Instant::now());
            state.history.push(record.clone());
        }

        Some(record)
    }
}

// ---------------------------------------------------------------------------
// Status DTO
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OrchestratorStatus {
    pub mode: OrchestratorMode,
    pub paused: bool,
    pub cycles: u64,
    pub agents_registered: usize,
    pub agents_running: usize,
    pub running_agents: Vec<String>,
    pub total_optimizations: u64,
    pub total_findings: u64,
    pub avg_improvement_pct: f64,
    pub uptime_secs: u64,
}

// ---------------------------------------------------------------------------
// Factory: create the full agent registry
// ---------------------------------------------------------------------------

pub fn create_all_agents() -> Vec<Box<dyn SuperAgent>> {
    use super::agents::*;

    vec![
        Box::new(pipeline_bottleneck::PipelineBottleneckDetectorAgent::new()),
        Box::new(latency_optimizer::LatencyOptimizerAgent::new()),
        Box::new(llm_cost_optimizer::LlmCostOptimizerAgent::new()),
        Box::new(agent_efficiency::AgentEfficiencyOptimizerAgent::new()),
        Box::new(context_compressor::ContextCompressorAgent::new()),
        Box::new(cache_optimizer::CacheOptimizerAgent::new()),
        Box::new(database_optimizer::DatabaseOptimizerAgent::new()),
        Box::new(concurrency_optimizer::ConcurrencyOptimizerAgent::new()),
        Box::new(sse_optimizer::SseStreamOptimizerAgent::new()),
        Box::new(build_runtime::BuildRuntimeOptimizerAgent::new()),
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_creates_all_agents() {
        let agents = create_all_agents();
        assert_eq!(agents.len(), 10);

        let kinds: HashSet<SuperAgentKind> =
            agents.iter().map(|a| a.kind()).collect();
        assert_eq!(kinds.len(), 10);
    }

    #[test]
    fn no_duplicate_priorities() {
        let agents = create_all_agents();
        let priorities: Vec<u8> = agents.iter().map(|a| a.priority()).collect();
        let unique: HashSet<u8> = priorities.iter().copied().collect();
        assert_eq!(priorities.len(), unique.len(), "Agent priorities must be unique");
    }
}
