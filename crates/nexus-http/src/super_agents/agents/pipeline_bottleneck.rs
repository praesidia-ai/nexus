//! Pipeline Bottleneck Detector — identifies slow steps in the 11-step execution pipeline.
//!
//! TRIGGERS: After every pipeline run, or every 2 minutes
//! INPUT: Per-step metrics, retry counts, error rates
//! ACTIONS: Flags bottlenecks, suggests skip conditions, recommends parallelism
//! SAFETY: Read-only analysis — never modifies pipeline structure directly

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::super_agents::traits::*;
use crate::super_agents::types::*;

pub struct PipelineBottleneckDetectorAgent;

impl Default for PipelineBottleneckDetectorAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineBottleneckDetectorAgent {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SuperAgent for PipelineBottleneckDetectorAgent {
    fn name(&self) -> &str {
        "Pipeline Bottleneck Detector"
    }

    fn kind(&self) -> SuperAgentKind {
        SuperAgentKind::PipelineBottleneckDetector
    }

    fn trigger(&self) -> TriggerMode {
        TriggerMode::Periodic {
            interval: Duration::from_secs(120),
        }
    }

    fn risk_level(&self) -> Severity {
        Severity::Info
    }

    async fn analyze(&self, ctx: &AnalysisContext) -> anyhow::Result<AnalysisReport> {
        let start = std::time::Instant::now();
        let mut findings = Vec::new();

        let steps = [
            "generate_spec",
            "validate_spec",
            "generate_schema",
            "generate_config",
            "generate_api",
            "generate_ui",
            "generate_agents",
            "generate_auth",
            "validate_all",
            "commit_files",
            "install_and_start",
        ];

        // Collect per-step timing data
        let mut step_data: Vec<(&str, f64, f64)> = Vec::new();
        let mut total_avg = 0.0;

        for step in &steps {
            let metric = format!("pipeline.step.{}.latency_ms", step);
            let avg = ctx.metrics.avg(&metric, 50).await;
            let p95 = ctx.metrics.p95(&metric, 50).await;
            step_data.push((step, avg, p95));
            total_avg += avg;
        }

        if total_avg == 0.0 {
            return Ok(AnalysisReport {
                agent: self.name().into(),
                agent_kind: self.kind(),
                timestamp: Utc::now().to_rfc3339(),
                findings: vec![],
                scan_duration_ms: start.elapsed().as_millis() as u64,
                system_snapshot: ctx.snapshot.clone(),
            });
        }

        // Find steps that take > 25% of total time
        for (step, avg, p95) in &step_data {
            let pct = avg / total_avg * 100.0;
            if pct > 25.0 {
                findings.push(Finding {
                    id: Uuid::new_v4().to_string(),
                    severity: if pct > 40.0 {
                        Severity::High
                    } else {
                        Severity::Medium
                    },
                    category: "bottleneck".into(),
                    description: format!(
                        "Step '{}' consumes {:.0}% of pipeline time (avg {:.0}ms, P95 {:.0}ms)",
                        step, pct, avg, p95
                    ),
                    metric_before: *avg,
                    suggested_action: format!(
                        "Optimize step '{}' — it dominates the critical path",
                        step
                    ),
                    estimated_improvement_pct: pct * 0.4,
                    metadata: {
                        let mut m = HashMap::new();
                        m.insert("step".into(), serde_json::json!(step));
                        m.insert("pct_of_total".into(), serde_json::json!(pct));
                        m
                    },
                });
            }
        }

        // Detect high variance steps (P95 > 3x avg)
        for (step, avg, p95) in &step_data {
            if *avg > 0.0 && *p95 > avg * 3.0 {
                findings.push(Finding {
                    id: Uuid::new_v4().to_string(),
                    severity: Severity::Medium,
                    category: "high_variance".into(),
                    description: format!(
                        "Step '{}' has high variance: avg {:.0}ms but P95 {:.0}ms ({:.1}x)",
                        step,
                        avg,
                        p95,
                        p95 / avg
                    ),
                    metric_before: *p95,
                    suggested_action: format!(
                        "Investigate inconsistent execution of step '{}'",
                        step
                    ),
                    estimated_improvement_pct: 15.0,
                    metadata: HashMap::new(),
                });
            }
        }

        // Identify parallelizable step groups
        let independent_groups: &[&[&str]] = &[
            &["generate_schema", "generate_config"],
            &["generate_api", "generate_auth"],
        ];

        for group in independent_groups {
            let group_total: f64 = group
                .iter()
                .filter_map(|s| step_data.iter().find(|(name, _, _)| name == s))
                .map(|(_, avg, _)| avg)
                .sum();
            let group_max: f64 = group
                .iter()
                .filter_map(|s| step_data.iter().find(|(name, _, _)| name == s))
                .map(|(_, avg, _)| *avg)
                .fold(0.0f64, f64::max);

            if group_total > 5_000.0 && group_max < group_total {
                let savings_ms = group_total - group_max;
                findings.push(Finding {
                    id: Uuid::new_v4().to_string(),
                    severity: Severity::Medium,
                    category: "parallelizable".into(),
                    description: format!(
                        "Steps [{}] could run in parallel, saving {:.0}ms",
                        group.join(", "),
                        savings_ms
                    ),
                    metric_before: group_total,
                    suggested_action: format!(
                        "Run [{}] concurrently using tokio::join!",
                        group.join(", ")
                    ),
                    estimated_improvement_pct: (savings_ms / group_total) * 100.0,
                    metadata: HashMap::new(),
                });
            }
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
        _ctx: &OptimizationContext,
        report: &AnalysisReport,
    ) -> anyhow::Result<OptimizationResult> {
        // This agent is primarily a detector — it produces recommendations
        // that other agents (LatencyOptimizer, ConcurrencyOptimizer) act on.
        Ok(OptimizationResult {
            agent: self.name().into(),
            agent_kind: self.kind(),
            timestamp: Utc::now().to_rfc3339(),
            optimizations: report
                .actionable_findings()
                .iter()
                .map(|f| Optimization {
                    finding_id: f.id.clone(),
                    action_taken: format!("Flagged for action: {}", f.suggested_action),
                    metric_before: f.metric_before,
                    metric_after: f.metric_before,
                    improvement_pct: 0.0,
                    rollback_key: None,
                })
                .collect(),
            total_improvement_pct: 0.0,
            duration_ms: 0,
            requires_restart: false,
        })
    }

    async fn validate(&self, _ctx: &ValidationContext) -> anyhow::Result<ValidationOutcome> {
        Ok(ValidationOutcome {
            passed: true,
            checks_run: 0,
            checks_passed: 0,
            failures: vec![],
        })
    }

    async fn rollback(
        &self,
        _app: &Arc<crate::state::AppState>,
        _rollback_key: &str,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}
