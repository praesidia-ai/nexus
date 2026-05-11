//! Adaptive Runtime — real-time intelligence that adjusts behavior DURING execution.
//!
//! Monitors LLM latency, token usage, failures, and agent iterations in real-time.
//! Reacts instantly: switches models, compresses context, adjusts retries, parallelizes.
//!
//! This is NOT post-hoc analysis. This runs WHILE the pipeline executes.

use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;
use tokio::sync::RwLock;
use tracing::info;

// ---------------------------------------------------------------------------
// Runtime Events (observed during execution)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum RuntimeEvent {
    /// An LLM call completed.
    LlmCallComplete {
        step: String,
        model: String,
        latency_ms: u64,
        input_tokens: u64,
        output_tokens: u64,
        success: bool,
    },
    /// A pipeline step completed.
    StepComplete {
        step: String,
        duration_ms: u64,
        success: bool,
    },
    /// An agent loop iteration completed.
    AgentIteration {
        agent: String,
        iteration: u32,
        tool_calls: u32,
    },
    /// A retry was triggered.
    RetryTriggered {
        step: String,
        attempt: u32,
        reason: String,
    },
    /// Token budget consumed.
    TokenBudgetUpdate {
        total_tokens_used: u64,
        budget_remaining_pct: f64,
    },
}

// ---------------------------------------------------------------------------
// Adjustment Actions (decisions made by the runtime)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub enum AdjustmentAction {
    /// Switch to a different model.
    SwitchModel {
        from: String,
        to: String,
        reason: String,
    },
    /// Compress context to reduce tokens.
    CompressContext {
        target_reduction_pct: u32,
        reason: String,
    },
    /// Adjust retry strategy.
    AdjustRetries {
        new_max: u32,
        reason: String,
    },
    /// Fall back to deterministic generation.
    FallbackDeterministic {
        step: String,
        reason: String,
    },
    /// Parallelize remaining subtasks.
    ParallelizeSubtasks {
        tasks: Vec<String>,
        reason: String,
    },
    /// No adjustment needed.
    NoAction,
}

// ---------------------------------------------------------------------------
// Runtime State (accumulated during execution)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct RuntimeState {
    /// Running average LLM latency in ms.
    avg_latency_ms: f64,
    /// Total LLM calls made.
    total_calls: u32,
    /// Total failures.
    total_failures: u32,
    /// Consecutive failures.
    consecutive_failures: u32,
    /// Total tokens consumed.
    total_tokens: u64,
    /// Total retries triggered.
    total_retries: u32,
    /// Slowest step name and duration.
    slowest_step: Option<(String, u64)>,
    /// Agent iteration counts by agent name.
    agent_iterations: Vec<(String, u32)>,
    /// Time the runtime started.
    started_at: Instant,
    /// Actions already taken (to avoid repeating).
    actions_taken: Vec<String>,
}

impl RuntimeState {
    fn new() -> Self {
        Self {
            avg_latency_ms: 0.0,
            total_calls: 0,
            total_failures: 0,
            consecutive_failures: 0,
            total_tokens: 0,
            total_retries: 0,
            slowest_step: None,
            agent_iterations: Vec::new(),
            started_at: Instant::now(),
            actions_taken: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Adaptive Runtime
// ---------------------------------------------------------------------------

/// The adaptive runtime — lives for one pipeline execution.
/// Thread-safe, async-safe, lightweight.
pub struct AdaptiveRuntime {
    state: Arc<RwLock<RuntimeState>>,
    /// Thresholds (configurable).
    config: RuntimeConfig,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Latency threshold before switching models (ms).
    pub slow_step_threshold_ms: u64,
    /// Max consecutive failures before fallback.
    pub max_consecutive_failures: u32,
    /// Token budget warning threshold (0.0-1.0).
    pub token_budget_warn_pct: f64,
    /// Max agent iterations before intervention.
    pub max_agent_iterations: u32,
    /// Max retries before strategy change.
    pub max_retries_before_change: u32,
    /// Total token budget for this execution (triggers compression at 75%).
    pub token_budget: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            slow_step_threshold_ms: 20_000,
            max_consecutive_failures: 2,
            token_budget_warn_pct: 0.2,
            max_agent_iterations: 10,
            max_retries_before_change: 2,
            token_budget: 200_000,
        }
    }
}

impl Default for AdaptiveRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl AdaptiveRuntime {
    /// Create a new runtime for a pipeline execution.
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(RuntimeState::new())),
            config: RuntimeConfig::default(),
        }
    }

    pub fn with_config(config: RuntimeConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(RuntimeState::new())),
            config,
        }
    }

    /// Observe a runtime event and update state.
    pub async fn observe(&self, event: RuntimeEvent) {
        let mut state = self.state.write().await;

        match &event {
            RuntimeEvent::LlmCallComplete {
                latency_ms,
                input_tokens,
                output_tokens,
                success,
                ..
            } => {
                state.total_calls += 1;
                state.total_tokens += input_tokens + output_tokens;
                // Exponential moving average
                let alpha = 0.3;
                state.avg_latency_ms =
                    state.avg_latency_ms * (1.0 - alpha) + *latency_ms as f64 * alpha;

                if *success {
                    state.consecutive_failures = 0;
                } else {
                    state.total_failures += 1;
                    state.consecutive_failures += 1;
                }
            }
            RuntimeEvent::StepComplete {
                step, duration_ms, ..
            } => {
                if let Some((_, slowest)) = &state.slowest_step {
                    if duration_ms > slowest {
                        state.slowest_step = Some((step.clone(), *duration_ms));
                    }
                } else {
                    state.slowest_step = Some((step.clone(), *duration_ms));
                }
            }
            RuntimeEvent::AgentIteration {
                agent, iteration, ..
            } => {
                if let Some(entry) = state
                    .agent_iterations
                    .iter_mut()
                    .find(|(a, _)| a == agent)
                {
                    entry.1 = *iteration;
                } else {
                    state.agent_iterations.push((agent.clone(), *iteration));
                }
            }
            RuntimeEvent::RetryTriggered { .. } => {
                state.total_retries += 1;
            }
            RuntimeEvent::TokenBudgetUpdate {
                total_tokens_used,
                budget_remaining_pct,
            } => {
                state.total_tokens = *total_tokens_used;
                // If budget is critically low, mark for immediate action
                if *budget_remaining_pct < self.config.token_budget_warn_pct {
                    tracing::info!(
                        remaining_pct = budget_remaining_pct,
                        "Adaptive runtime: token budget critically low"
                    );
                }
            }
        }
    }

    /// Decide what adjustment to make based on current state.
    pub async fn decide_adjustment(&self, context: &str) -> AdjustmentAction {
        let mut state = self.state.write().await;

        // Rule 1: Consecutive failures → change strategy
        if state.consecutive_failures >= self.config.max_consecutive_failures {
            let action_key = format!("fallback_{}", context);
            if !state.actions_taken.contains(&action_key) {
                state.actions_taken.push(action_key);
                info!(
                    failures = state.consecutive_failures,
                    context = context,
                    "Adaptive runtime: switching to fallback after failures"
                );
                return AdjustmentAction::FallbackDeterministic {
                    step: context.to_string(),
                    reason: format!(
                        "{} consecutive failures — switching to deterministic fallback",
                        state.consecutive_failures
                    ),
                };
            }
        }

        // Rule 2: High latency → switch to faster model
        if state.avg_latency_ms > self.config.slow_step_threshold_ms as f64
            && state.total_calls >= 2
        {
            let action_key = "switch_model_fast".to_string();
            if !state.actions_taken.contains(&action_key) {
                state.actions_taken.push(action_key);
                info!(
                    avg_latency = state.avg_latency_ms,
                    threshold = self.config.slow_step_threshold_ms,
                    "Adaptive runtime: switching to faster model"
                );
                return AdjustmentAction::SwitchModel {
                    from: "current".into(),
                    to: "gpt-4.1-mini".into(),
                    reason: format!(
                        "Average latency {:.0}ms exceeds {:.0}ms threshold",
                        state.avg_latency_ms, self.config.slow_step_threshold_ms
                    ),
                };
            }
        }

        // Rule 3: High token usage → compress context (at 75% of budget)
        let token_threshold = (self.config.token_budget as f64 * 0.75) as u64;
        if state.total_tokens > token_threshold {
            let action_key = "compress_context".to_string();
            if !state.actions_taken.contains(&action_key) {
                state.actions_taken.push(action_key);
                info!(
                    tokens = state.total_tokens,
                    "Adaptive runtime: compressing context"
                );
                return AdjustmentAction::CompressContext {
                    target_reduction_pct: 30,
                    reason: format!(
                        "Token usage at {} — compressing to save budget",
                        state.total_tokens
                    ),
                };
            }
        }

        // Rule 4: Too many retries → reduce max retries and simplify
        if state.total_retries >= self.config.max_retries_before_change {
            let action_key = "adjust_retries".to_string();
            if !state.actions_taken.contains(&action_key) {
                state.actions_taken.push(action_key);
                return AdjustmentAction::AdjustRetries {
                    new_max: 1,
                    reason: format!(
                        "{} retries triggered — reducing retry budget, simplifying prompts",
                        state.total_retries
                    ),
                };
            }
        }

        // Rule 5: Agent running too many iterations → intervene
        let high_iter_agent = state
            .agent_iterations
            .iter()
            .find(|(_, iters)| *iters >= self.config.max_agent_iterations)
            .map(|(agent, iters)| (agent.clone(), *iters));

        if let Some((agent, iters)) = high_iter_agent {
            let action_key = format!("agent_intervene_{}", agent);
            if !state.actions_taken.contains(&action_key) {
                state.actions_taken.push(action_key);
                return AdjustmentAction::CompressContext {
                    target_reduction_pct: 40,
                    reason: format!(
                        "Agent '{}' at {} iterations — compressing context to help convergence",
                        agent, iters
                    ),
                };
            }
        }

        AdjustmentAction::NoAction
    }

    /// Get a snapshot of runtime metrics for logging/debugging.
    pub async fn snapshot(&self) -> RuntimeSnapshot {
        let state = self.state.read().await;
        RuntimeSnapshot {
            elapsed_ms: state.started_at.elapsed().as_millis() as u64,
            total_llm_calls: state.total_calls,
            total_failures: state.total_failures,
            avg_latency_ms: state.avg_latency_ms,
            total_tokens: state.total_tokens,
            total_retries: state.total_retries,
            actions_taken: state.actions_taken.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeSnapshot {
    pub elapsed_ms: u64,
    pub total_llm_calls: u32,
    pub total_failures: u32,
    pub avg_latency_ms: f64,
    pub total_tokens: u64,
    pub total_retries: u32,
    pub actions_taken: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn detects_consecutive_failures() {
        let rt = AdaptiveRuntime::new();
        for _ in 0..3 {
            rt.observe(RuntimeEvent::LlmCallComplete {
                step: "gen".into(),
                model: "gpt-4o".into(),
                latency_ms: 1000,
                input_tokens: 500,
                output_tokens: 200,
                success: false,
            })
            .await;
        }
        let action = rt.decide_adjustment("gen_step").await;
        assert!(matches!(action, AdjustmentAction::FallbackDeterministic { .. }));
    }

    #[tokio::test]
    async fn detects_high_latency() {
        let rt = AdaptiveRuntime::new();
        // Need enough observations for the EMA to exceed threshold
        for _ in 0..5 {
            rt.observe(RuntimeEvent::LlmCallComplete {
                step: "gen".into(),
                model: "gpt-4o".into(),
                latency_ms: 40_000,
                input_tokens: 500,
                output_tokens: 200,
                success: true,
            })
            .await;
        }
        let action = rt.decide_adjustment("gen_step").await;
        assert!(
            matches!(action, AdjustmentAction::SwitchModel { .. } | AdjustmentAction::CompressContext { .. }),
            "Expected model switch or compression for high latency, got: {:?}",
            action,
        );
    }

    #[tokio::test]
    async fn no_action_when_healthy() {
        let rt = AdaptiveRuntime::new();
        rt.observe(RuntimeEvent::LlmCallComplete {
            step: "gen".into(),
            model: "gpt-4o".into(),
            latency_ms: 2000,
            input_tokens: 500,
            output_tokens: 200,
            success: true,
        })
        .await;
        let action = rt.decide_adjustment("gen_step").await;
        assert!(matches!(action, AdjustmentAction::NoAction));
    }

    #[tokio::test]
    async fn snapshot_tracks_state() {
        let rt = AdaptiveRuntime::new();
        rt.observe(RuntimeEvent::LlmCallComplete {
            step: "s".into(), model: "m".into(),
            latency_ms: 100, input_tokens: 50, output_tokens: 25, success: true,
        }).await;
        let snap = rt.snapshot().await;
        assert_eq!(snap.total_llm_calls, 1);
        assert_eq!(snap.total_tokens, 75);
    }
}
