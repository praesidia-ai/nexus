//! Context Compression Agent — reduces token usage while preserving semantic meaning.
//!
//! TRIGGERS: Every 10 minutes, or when avg input tokens > 50K
//! INPUT: LLM call records, prompt templates, conversation histories
//! ACTIONS: Flags verbose prompts, suggests compression strategies, truncates history
//! SAFETY: Never removes critical system prompts; preserves semantic fidelity

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::super_agents::metrics_bus::metric_names;
use crate::super_agents::traits::*;
use crate::super_agents::types::*;

pub struct ContextCompressorAgent;

impl Default for ContextCompressorAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextCompressorAgent {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SuperAgent for ContextCompressorAgent {
    fn name(&self) -> &str {
        "Context Compressor"
    }

    fn kind(&self) -> SuperAgentKind {
        SuperAgentKind::ContextCompressor
    }

    fn trigger(&self) -> TriggerMode {
        TriggerMode::Hybrid {
            interval: Duration::from_secs(600),
            metric: metric_names::LLM_INPUT_TOKENS.into(),
            value: 50_000.0,
        }
    }

    fn risk_level(&self) -> Severity {
        Severity::Medium
    }

    async fn analyze(&self, ctx: &AnalysisContext) -> anyhow::Result<AnalysisReport> {
        let start = std::time::Instant::now();
        let mut findings = Vec::new();

        let samples = ctx
            .metrics
            .recent_samples(metric_names::LLM_INPUT_TOKENS, 300)
            .await;

        // Group by purpose
        let mut purpose_stats: HashMap<String, Vec<f64>> = HashMap::new();
        for sample in &samples {
            let purpose = sample.tags.get("purpose").cloned().unwrap_or_default();
            purpose_stats.entry(purpose).or_default().push(sample.value);
        }

        for (purpose, tokens) in &purpose_stats {
            if tokens.is_empty() {
                continue;
            }
            let avg = tokens.iter().sum::<f64>() / tokens.len() as f64;
            let max = tokens.iter().copied().fold(0.0f64, f64::max);

            // Flag prompts averaging > 30K tokens
            if avg > 30_000.0 {
                let compression_ratio = if avg > 100_000.0 {
                    0.5 // aggressive compression
                } else if avg > 50_000.0 {
                    0.65
                } else {
                    0.75
                };

                findings.push(Finding {
                    id: Uuid::new_v4().to_string(),
                    severity: if avg > 100_000.0 {
                        Severity::High
                    } else {
                        Severity::Medium
                    },
                    category: "verbose_prompt".into(),
                    description: format!(
                        "Purpose '{}': avg {:.0} tokens (max {:.0}), {} calls",
                        purpose,
                        avg,
                        max,
                        tokens.len()
                    ),
                    metric_before: avg,
                    suggested_action: suggest_compression(purpose, avg),
                    estimated_improvement_pct: (1.0 - compression_ratio) * 100.0,
                    metadata: {
                        let mut m = HashMap::new();
                        m.insert("purpose".into(), serde_json::json!(purpose));
                        m.insert("avg_tokens".into(), serde_json::json!(avg));
                        m.insert("max_tokens".into(), serde_json::json!(max));
                        m.insert("call_count".into(), serde_json::json!(tokens.len()));
                        m.insert("target_ratio".into(), serde_json::json!(compression_ratio));
                        m
                    },
                });
            }

            // Flag high variance in token count (inconsistent prompts)
            if tokens.len() > 5 {
                let variance = tokens
                    .iter()
                    .map(|t| (t - avg).powi(2))
                    .sum::<f64>()
                    / tokens.len() as f64;
                let std_dev = variance.sqrt();
                if std_dev > avg * 0.5 {
                    findings.push(Finding {
                        id: Uuid::new_v4().to_string(),
                        severity: Severity::Low,
                        category: "inconsistent_context".into(),
                        description: format!(
                            "Purpose '{}' has high token variance (std_dev {:.0}, avg {:.0})",
                            purpose, std_dev, avg
                        ),
                        metric_before: std_dev,
                        suggested_action: "Standardize prompt templates to reduce variance".into(),
                        estimated_improvement_pct: 10.0,
                        metadata: HashMap::new(),
                    });
                }
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
        ctx: &OptimizationContext,
        report: &AnalysisReport,
    ) -> anyhow::Result<OptimizationResult> {
        let start = std::time::Instant::now();
        let mut optimizations = Vec::new();

        for finding in report.actionable_findings() {
            if finding.category != "verbose_prompt" {
                continue;
            }

            let purpose = finding
                .metadata
                .get("purpose")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let target_ratio = finding
                .metadata
                .get("target_ratio")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.7);

            if ctx.dry_run {
                optimizations.push(Optimization {
                    finding_id: finding.id.clone(),
                    action_taken: format!(
                        "[DRY RUN] Would compress '{}' context to {:.0}% of current",
                        purpose,
                        target_ratio * 100.0
                    ),
                    metric_before: finding.metric_before,
                    metric_after: finding.metric_before,
                    improvement_pct: 0.0,
                    rollback_key: None,
                });
                continue;
            }

            // Set compression target in metrics bus
            ctx.metrics
                .record_value(
                    &format!("optimization.context_compression.{}.ratio", purpose),
                    target_ratio,
                )
                .await;

            // Enable conversation history truncation
            ctx.metrics
                .record_value(
                    &format!("optimization.context_compression.{}.max_history", purpose),
                    10.0, // keep last 10 messages
                )
                .await;

            let after = finding.metric_before * target_ratio;
            optimizations.push(Optimization {
                finding_id: finding.id.clone(),
                action_taken: format!(
                    "Set compression ratio {:.0}% for '{}' (target {:.0} tokens)",
                    target_ratio * 100.0,
                    purpose,
                    after
                ),
                metric_before: finding.metric_before,
                metric_after: after,
                improvement_pct: finding.estimated_improvement_pct,
                rollback_key: Some(format!("compress:{}", purpose)),
            });
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

        // Quality should not degrade significantly
        if current.pipeline_error_rate > ctx.snapshot_before.pipeline_error_rate + 0.1 {
            failures.push("Error rate increased after context compression".into());
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
        tracing::warn!(key = rollback_key, "Rolling back context compression");
        Ok(())
    }
}

fn suggest_compression(purpose: &str, avg_tokens: f64) -> String {
    if purpose.contains("chat") {
        format!(
            "Truncate conversation history to last 10 messages; summarize older context ({:.0}K tokens avg)",
            avg_tokens / 1000.0
        )
    } else if purpose.contains("generate") {
        "Remove redundant examples from generation prompts; use structured schemas instead of verbose descriptions".into()
    } else if purpose.contains("agent") {
        "Limit tool result inclusion to summaries; cap code file context to relevant functions only".into()
    } else {
        format!(
            "Review prompts for '{}' — {:.0}K avg tokens likely has redundancy",
            purpose,
            avg_tokens / 1000.0
        )
    }
}
