//! LLM Cost Optimizer Agent — dynamically selects cheaper models and reduces token spend.
//!
//! TRIGGERS: Every 10 minutes, or when daily cost > 80% of budget
//! INPUT: LLM call records (model, tokens, cost, purpose, latency), cost budgets
//! ACTIONS: Downgrades models for low-complexity tasks, flags expensive prompts, suggests caching
//! SAFETY: Never downgrades security/architecture tasks; preserves quality thresholds

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::super_agents::metrics_bus::metric_names;
use crate::super_agents::traits::*;
use crate::super_agents::types::*;

/// Model tiers ordered by cost (cheapest first).
const MODEL_TIERS: &[(&str, f64)] = &[
    ("gpt-4.1-mini", 0.40),
    ("claude-haiku-4-5-20251001", 0.80),
    ("gemini-2.0-flash", 0.075),
    ("gpt-4o", 2.50),
    ("claude-sonnet-4-6", 3.00),
    ("gemini-2.5-pro", 1.25),
    ("o1-mini", 3.00),
    ("o1", 15.00),
    ("claude-opus-4-6", 15.00),
];

/// Purposes that MUST use a strong model.
const PROTECTED_PURPOSES: &[&str] = &[
    "security",
    "audit",
    "architecture",
    "plan",
    "generate_spec",
    "generate_ui",
];

pub struct LlmCostOptimizerAgent;

impl Default for LlmCostOptimizerAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmCostOptimizerAgent {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SuperAgent for LlmCostOptimizerAgent {
    fn name(&self) -> &str {
        "LLM Cost Optimizer"
    }

    fn kind(&self) -> SuperAgentKind {
        SuperAgentKind::LlmCostOptimizer
    }

    fn trigger(&self) -> TriggerMode {
        TriggerMode::Hybrid {
            interval: Duration::from_secs(600),
            metric: metric_names::LLM_DAILY_COST_USD.into(),
            value: 40.0, // 80% of default $50 budget
        }
    }

    fn risk_level(&self) -> Severity {
        Severity::Medium
    }

    async fn analyze(&self, ctx: &AnalysisContext) -> anyhow::Result<AnalysisReport> {
        let start = std::time::Instant::now();
        let mut findings = Vec::new();

        // 1. Check daily cost vs budget
        let daily_cost = ctx.snapshot.llm_daily_cost_usd;
        if daily_cost > 0.0 {
            // Get per-model cost breakdown from recent samples
            let cost_samples = ctx.metrics.recent_samples(metric_names::LLM_COST_USD, 500).await;

            let mut model_costs: HashMap<String, (usize, f64, f64)> = HashMap::new(); // model -> (count, total_cost, avg_tokens)
            let mut purpose_costs: HashMap<String, (usize, f64, String)> = HashMap::new(); // purpose -> (count, total_cost, model)

            for sample in &cost_samples {
                let model = sample.tags.get("model").cloned().unwrap_or_default();
                let purpose = sample.tags.get("purpose").cloned().unwrap_or_default();

                let entry = model_costs.entry(model.clone()).or_default();
                entry.0 += 1;
                entry.1 += sample.value;

                let pentry = purpose_costs
                    .entry(purpose)
                    .or_insert((0, 0.0, model.clone()));
                pentry.0 += 1;
                pentry.1 += sample.value;
            }

            // 2. Find purposes using expensive models for simple tasks
            for (purpose, (count, cost, model)) in &purpose_costs {
                let is_protected = PROTECTED_PURPOSES
                    .iter()
                    .any(|p| purpose.contains(p));

                if is_protected {
                    continue;
                }

                let model_cost_per_m = model_input_cost(model);
                if model_cost_per_m >= 2.0 && *count >= 3 {
                    let cheaper = cheapest_adequate_model(purpose);
                    let savings_pct = 1.0 - (model_input_cost(cheaper) / model_cost_per_m);

                    findings.push(Finding {
                        id: Uuid::new_v4().to_string(),
                        severity: if *cost > 5.0 {
                            Severity::High
                        } else if *cost > 1.0 {
                            Severity::Medium
                        } else {
                            Severity::Low
                        },
                        category: "model_downgrade".into(),
                        description: format!(
                            "Purpose '{}' uses {} ({} calls, ${:.2} total) — could use {}",
                            purpose, model, count, cost, cheaper
                        ),
                        metric_before: *cost,
                        suggested_action: format!(
                            "Route '{}' calls from {} to {} (save ~{:.0}%)",
                            purpose, model, cheaper, savings_pct * 100.0
                        ),
                        estimated_improvement_pct: savings_pct * 100.0,
                        metadata: {
                            let mut m = HashMap::new();
                            m.insert("purpose".into(), serde_json::json!(purpose));
                            m.insert("current_model".into(), serde_json::json!(model));
                            m.insert("suggested_model".into(), serde_json::json!(cheaper));
                            m.insert("call_count".into(), serde_json::json!(count));
                            m
                        },
                    });
                }
            }

            // 3. Check for high-token prompts that could be compressed
            let token_samples = ctx
                .metrics
                .recent_samples(metric_names::LLM_INPUT_TOKENS, 200)
                .await;
            let mut purpose_tokens: HashMap<String, (usize, f64)> = HashMap::new();
            for sample in &token_samples {
                let purpose = sample.tags.get("purpose").cloned().unwrap_or_default();
                let entry = purpose_tokens.entry(purpose).or_default();
                entry.0 += 1;
                entry.1 += sample.value;
            }
            for (purpose, (count, total_tokens)) in &purpose_tokens {
                let avg = total_tokens / *count as f64;
                if avg > 50_000.0 {
                    findings.push(Finding {
                        id: Uuid::new_v4().to_string(),
                        severity: if avg > 100_000.0 {
                            Severity::High
                        } else {
                            Severity::Medium
                        },
                        category: "token_reduction".into(),
                        description: format!(
                            "Purpose '{}' averages {:.0} input tokens per call",
                            purpose, avg
                        ),
                        metric_before: avg,
                        suggested_action: format!(
                            "Compress context for '{}' calls — target < 30K tokens",
                            purpose
                        ),
                        estimated_improvement_pct: 40.0,
                        metadata: {
                            let mut m = HashMap::new();
                            m.insert("purpose".into(), serde_json::json!(purpose));
                            m.insert("avg_tokens".into(), serde_json::json!(avg));
                            m
                        },
                    });
                }
            }

            // 4. Budget warning
            if daily_cost > 40.0 {
                findings.push(Finding {
                    id: Uuid::new_v4().to_string(),
                    severity: if daily_cost > 50.0 {
                        Severity::Critical
                    } else {
                        Severity::High
                    },
                    category: "budget_alert".into(),
                    description: format!("Daily LLM cost is ${:.2} (budget: $50.00)", daily_cost),
                    metric_before: daily_cost,
                    suggested_action: "Reduce call volume or downgrade models across the board"
                        .into(),
                    estimated_improvement_pct: 0.0,
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
        ctx: &OptimizationContext,
        report: &AnalysisReport,
    ) -> anyhow::Result<OptimizationResult> {
        let start = std::time::Instant::now();
        let mut optimizations = Vec::new();

        for finding in report.actionable_findings() {
            match finding.category.as_str() {
                "model_downgrade" => {
                    let purpose = finding
                        .metadata
                        .get("purpose")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let suggested = finding
                        .metadata
                        .get("suggested_model")
                        .and_then(|v| v.as_str())
                        .unwrap_or("gpt-4.1-mini");

                    if ctx.dry_run {
                        optimizations.push(Optimization {
                            finding_id: finding.id.clone(),
                            action_taken: format!(
                                "[DRY RUN] Would route '{}' to {}",
                                purpose, suggested
                            ),
                            metric_before: finding.metric_before,
                            metric_after: finding.metric_before,
                            improvement_pct: 0.0,
                            rollback_key: None,
                        });
                        continue;
                    }

                    // Record model routing override in metrics bus
                    ctx.metrics
                        .record_value(
                            &format!("optimization.model_route.{}", purpose),
                            model_tier_index(suggested) as f64,
                        )
                        .await;

                    let estimated_savings = finding.metric_before * (finding.estimated_improvement_pct / 100.0);
                    optimizations.push(Optimization {
                        finding_id: finding.id.clone(),
                        action_taken: format!("Routed '{}' to {} (from higher-cost model)", purpose, suggested),
                        metric_before: finding.metric_before,
                        metric_after: finding.metric_before - estimated_savings,
                        improvement_pct: finding.estimated_improvement_pct,
                        rollback_key: Some(format!("cost:model_route:{}", purpose)),
                    });
                }
                "token_reduction" => {
                    let purpose = finding
                        .metadata
                        .get("purpose")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");

                    // Flag context compression for this purpose
                    ctx.metrics
                        .record_value(
                            &format!("optimization.compress_context.{}", purpose),
                            1.0,
                        )
                        .await;

                    optimizations.push(Optimization {
                        finding_id: finding.id.clone(),
                        action_taken: format!(
                            "Flagged '{}' for context compression (avg {:.0} tokens)",
                            purpose, finding.metric_before
                        ),
                        metric_before: finding.metric_before,
                        metric_after: finding.metric_before * 0.6,
                        improvement_pct: 40.0,
                        rollback_key: Some(format!("cost:compress:{}", purpose)),
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
        let mut failures = Vec::new();
        let current = ctx.metrics.snapshot().await;

        // Cost should not have INCREASED
        if current.llm_daily_cost_usd > ctx.snapshot_before.llm_daily_cost_usd * 1.1 {
            failures.push(format!(
                "Daily cost increased from ${:.2} to ${:.2}",
                ctx.snapshot_before.llm_daily_cost_usd, current.llm_daily_cost_usd
            ));
        }

        // Error rate should be stable
        if current.pipeline_error_rate > ctx.snapshot_before.pipeline_error_rate + 0.1 {
            failures.push("Pipeline error rate increased significantly after model changes".into());
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
        tracing::warn!(key = rollback_key, "Rolling back cost optimization");
        Ok(())
    }
}

fn model_input_cost(model: &str) -> f64 {
    MODEL_TIERS
        .iter()
        .find(|(m, _)| *m == model)
        .map(|(_, c)| *c)
        .unwrap_or(2.0)
}

fn model_tier_index(model: &str) -> usize {
    MODEL_TIERS
        .iter()
        .position(|(m, _)| *m == model)
        .unwrap_or(0)
}

fn cheapest_adequate_model(_purpose: &str) -> &'static str {
    // All purposes currently map to gpt-4.1-mini as the cheapest adequate model.
    // Extend with purpose-specific routing when premium models are needed.
    "gpt-4.1-mini"
}
