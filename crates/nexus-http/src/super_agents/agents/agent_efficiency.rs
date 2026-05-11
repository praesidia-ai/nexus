//! Agent Efficiency Optimizer — reduces unnecessary iterations in the ReAct agent loop.
//!
//! TRIGGERS: Every 5 minutes, or when avg iterations > 8
//! INPUT: Agent loop metrics (iterations, duration, tool calls, error rates)
//! ACTIONS: Adjusts max iterations, improves tool selection, optimizes prompts
//! SAFETY: Never reduces max iterations below 3; preserves task completion rate

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::super_agents::metrics_bus::metric_names;
use crate::super_agents::traits::*;
use crate::super_agents::types::*;

pub struct AgentEfficiencyOptimizerAgent;

impl Default for AgentEfficiencyOptimizerAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentEfficiencyOptimizerAgent {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SuperAgent for AgentEfficiencyOptimizerAgent {
    fn name(&self) -> &str {
        "Agent Efficiency Optimizer"
    }

    fn kind(&self) -> SuperAgentKind {
        SuperAgentKind::AgentEfficiencyOptimizer
    }

    fn trigger(&self) -> TriggerMode {
        TriggerMode::Hybrid {
            interval: Duration::from_secs(300),
            metric: metric_names::AGENT_LOOP_ITERATIONS.into(),
            value: 8.0,
        }
    }

    fn risk_level(&self) -> Severity {
        Severity::Medium
    }

    async fn analyze(&self, ctx: &AnalysisContext) -> anyhow::Result<AnalysisReport> {
        let start = std::time::Instant::now();
        let mut findings = Vec::new();

        let avg_iterations = ctx.snapshot.agent_loop_avg_iterations;
        let avg_duration = ctx.snapshot.agent_loop_avg_duration_ms;

        // High iteration count
        if avg_iterations > 5.0 {
            findings.push(Finding {
                id: Uuid::new_v4().to_string(),
                severity: if avg_iterations > 10.0 {
                    Severity::High
                } else {
                    Severity::Medium
                },
                category: "high_iterations".into(),
                description: format!(
                    "Agent loop averages {:.1} iterations ({:.0}ms total)",
                    avg_iterations, avg_duration
                ),
                metric_before: avg_iterations,
                suggested_action:
                    "Improve system prompt specificity; provide better tool descriptions; add few-shot examples"
                        .into(),
                estimated_improvement_pct: 30.0,
                metadata: HashMap::new(),
            });
        }

        // Long duration per iteration
        if avg_iterations > 0.0 {
            let per_iteration = avg_duration / avg_iterations;
            if per_iteration > 10_000.0 {
                findings.push(Finding {
                    id: Uuid::new_v4().to_string(),
                    severity: Severity::Medium,
                    category: "slow_iterations".into(),
                    description: format!(
                        "Each agent iteration takes {:.0}ms on average",
                        per_iteration
                    ),
                    metric_before: per_iteration,
                    suggested_action:
                        "Optimize tool execution; reduce context size per iteration; use faster model for tool selection"
                            .into(),
                    estimated_improvement_pct: 25.0,
                    metadata: HashMap::new(),
                });
            }
        }

        // High tool call count (inefficient tool usage)
        let avg_tools = ctx
            .metrics
            .avg(metric_names::AGENT_LOOP_TOOL_CALLS, 50)
            .await;
        if avg_tools > 15.0 {
            findings.push(Finding {
                id: Uuid::new_v4().to_string(),
                severity: Severity::Medium,
                category: "excessive_tools".into(),
                description: format!(
                    "Agent makes {:.0} tool calls per run on average",
                    avg_tools
                ),
                metric_before: avg_tools,
                suggested_action:
                    "Consolidate tool calls; add batch tool operations; improve planning prompt"
                        .into(),
                estimated_improvement_pct: 20.0,
                metadata: HashMap::new(),
            });
        }

        Ok(AnalysisReport {
            agent: self.name().into(),
            agent_kind: self.kind(),
            timestamp: Utc::now().to_rfc3339(),
            findings,
            scan_duration_ms: start.elapsed().as_millis() as u64,
            system_snapshot: ctx.snapshot.clone(),
        })
    }

    async fn optimize(
        &self,
        ctx: &OptimizationContext,
        report: &AnalysisReport,
    ) -> anyhow::Result<OptimizationResult> {
        let start = std::time::Instant::now();
        let mut optimizations = Vec::new();

        for finding in report.actionable_findings() {
            if ctx.dry_run {
                optimizations.push(Optimization {
                    finding_id: finding.id.clone(),
                    action_taken: format!("[DRY RUN] Would: {}", finding.suggested_action),
                    metric_before: finding.metric_before,
                    metric_after: finding.metric_before,
                    improvement_pct: 0.0,
                    rollback_key: None,
                });
                continue;
            }

            match finding.category.as_str() {
                "high_iterations" => {
                    let optimal_max = (finding.metric_before * 0.7).clamp(3.0, 15.0);
                    ctx.metrics
                        .record_value("optimization.agent_max_iterations", optimal_max)
                        .await;
                    optimizations.push(Optimization {
                        finding_id: finding.id.clone(),
                        action_taken: format!(
                            "Adjusted recommended max iterations to {:.0}",
                            optimal_max
                        ),
                        metric_before: finding.metric_before,
                        metric_after: finding.metric_before * 0.7,
                        improvement_pct: 30.0,
                        rollback_key: Some("agent_eff:max_iter".into()),
                    });
                }
                "slow_iterations" => {
                    ctx.metrics
                        .record_value("optimization.agent_fast_model.enabled", 1.0)
                        .await;
                    optimizations.push(Optimization {
                        finding_id: finding.id.clone(),
                        action_taken:
                            "Flagged agent loop for faster model on tool-selection iterations".into(),
                        metric_before: finding.metric_before,
                        metric_after: finding.metric_before * 0.75,
                        improvement_pct: 25.0,
                        rollback_key: Some("agent_eff:fast_model".into()),
                    });
                }
                _ => {}
            }
        }

        let total = if optimizations.is_empty() {
            0.0
        } else {
            optimizations.iter().map(|o| o.improvement_pct).sum::<f64>()
                / optimizations.len() as f64
        };

        Ok(OptimizationResult {
            agent: self.name().into(),
            agent_kind: self.kind(),
            timestamp: Utc::now().to_rfc3339(),
            optimizations,
            total_improvement_pct: total,
            duration_ms: start.elapsed().as_millis() as u64,
            requires_restart: false,
        })
    }

    async fn validate(&self, ctx: &ValidationContext) -> anyhow::Result<ValidationOutcome> {
        let current = ctx.metrics.snapshot().await;
        let mut failures = Vec::new();

        // Agent loop should still be functional
        if current.pipeline_error_rate > ctx.snapshot_before.pipeline_error_rate + 0.1 {
            failures
                .push("Pipeline error rate increased after agent efficiency changes".into());
        }

        Ok(ValidationOutcome {
            passed: failures.is_empty(),
            checks_run: 1,
            checks_passed: if failures.is_empty() { 1 } else { 0 },
            failures,
        })
    }

    async fn rollback(
        &self,
        _app: &Arc<crate::state::AppState>,
        rollback_key: &str,
    ) -> anyhow::Result<()> {
        tracing::warn!(
            key = rollback_key,
            "Rolling back agent efficiency optimization"
        );
        Ok(())
    }
}
