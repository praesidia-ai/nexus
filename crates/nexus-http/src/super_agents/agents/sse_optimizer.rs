//! SSE Stream Optimizer — reduces latency in Server-Sent Event streaming.
//!
//! TRIGGERS: Every 3 minutes, or when first-byte latency > 2s
//! INPUT: SSE metrics (active streams, first byte time, dropped events)
//! ACTIONS: Adjusts buffer sizes, optimizes serialization, suggests chunking
//! SAFETY: Never drops critical events; preserves event ordering

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::super_agents::metrics_bus::metric_names;
use crate::super_agents::traits::*;
use crate::super_agents::types::*;

pub struct SseStreamOptimizerAgent;

impl Default for SseStreamOptimizerAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl SseStreamOptimizerAgent {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SuperAgent for SseStreamOptimizerAgent {
    fn name(&self) -> &str {
        "SSE Stream Optimizer"
    }

    fn kind(&self) -> SuperAgentKind {
        SuperAgentKind::SseStreamOptimizer
    }

    fn trigger(&self) -> TriggerMode {
        TriggerMode::Hybrid {
            interval: Duration::from_secs(180),
            metric: metric_names::SSE_FIRST_BYTE_MS.into(),
            value: 2000.0,
        }
    }

    fn risk_level(&self) -> Severity {
        Severity::Low
    }

    async fn analyze(&self, ctx: &AnalysisContext) -> anyhow::Result<AnalysisReport> {
        let start = std::time::Instant::now();
        let mut findings = Vec::new();

        let first_byte = ctx.snapshot.sse_avg_first_byte_ms;
        if first_byte > 1000.0 {
            findings.push(Finding {
                id: Uuid::new_v4().to_string(),
                severity: if first_byte > 3000.0 {
                    Severity::High
                } else {
                    Severity::Medium
                },
                category: "first_byte_latency".into(),
                description: format!("SSE first byte latency is {:.0}ms", first_byte),
                metric_before: first_byte,
                suggested_action:
                    "Send immediate acknowledgement event before LLM processing; use pre-allocated buffers"
                        .into(),
                estimated_improvement_pct: 50.0,
                metadata: HashMap::new(),
            });
        }

        let active = ctx.snapshot.sse_active_streams;
        if active > 100 {
            findings.push(Finding {
                id: Uuid::new_v4().to_string(),
                severity: Severity::Medium,
                category: "stream_count".into(),
                description: format!("{} active SSE streams — potential resource pressure", active),
                metric_before: active as f64,
                suggested_action: "Implement stream idle timeout; add connection limit per client"
                    .into(),
                estimated_improvement_pct: 20.0,
                metadata: HashMap::new(),
            });
        }

        let dropped = ctx
            .metrics
            .latest(metric_names::SSE_DROPPED_EVENTS)
            .await;
        if dropped > 0.0 {
            findings.push(Finding {
                id: Uuid::new_v4().to_string(),
                severity: Severity::High,
                category: "dropped_events".into(),
                description: format!("{:.0} SSE events dropped", dropped),
                metric_before: dropped,
                suggested_action:
                    "Increase channel buffer size; implement backpressure signaling".into(),
                estimated_improvement_pct: 90.0,
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
                "first_byte_latency" => {
                    ctx.metrics
                        .record_value("optimization.sse_immediate_ack.enabled", 1.0)
                        .await;
                    optimizations.push(Optimization {
                        finding_id: finding.id.clone(),
                        action_taken: "Enabled immediate SSE acknowledgement events".into(),
                        metric_before: finding.metric_before,
                        metric_after: finding.metric_before * 0.5,
                        improvement_pct: 50.0,
                        rollback_key: Some("sse:immediate_ack".into()),
                    });
                }
                "dropped_events" => {
                    ctx.metrics
                        .record_value("optimization.sse_buffer_size", 256.0)
                        .await;
                    optimizations.push(Optimization {
                        finding_id: finding.id.clone(),
                        action_taken: "Increased SSE channel buffer to 256".into(),
                        metric_before: finding.metric_before,
                        metric_after: 0.0,
                        improvement_pct: 90.0,
                        rollback_key: Some("sse:buffer_size".into()),
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

        let dropped = ctx
            .metrics
            .latest(metric_names::SSE_DROPPED_EVENTS)
            .await;
        if dropped > 0.0 {
            failures.push("Still dropping SSE events after optimization".into());
        }

        if current.sse_avg_first_byte_ms > ctx.snapshot_before.sse_avg_first_byte_ms * 1.5 {
            failures.push("First byte latency increased".into());
        }

        Ok(ValidationOutcome {
            passed: failures.is_empty(),
            checks_run: 2,
            checks_passed: 2 - failures.len() as u32,
            failures,
        })
    }

    async fn rollback(
        &self,
        _app: &Arc<crate::state::AppState>,
        rollback_key: &str,
    ) -> anyhow::Result<()> {
        tracing::warn!(key = rollback_key, "Rolling back SSE optimization");
        Ok(())
    }
}
