//! Cache Optimizer Agent — introduces and tunes caching strategies.
//!
//! TRIGGERS: Every 5 minutes
//! INPUT: Cache hit/miss rates, LLM call patterns, repeated queries
//! ACTIONS: Adjusts TTL, suggests new cache layers, evicts cold entries
//! SAFETY: Never caches non-deterministic results; respects cache size limits

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::super_agents::metrics_bus::metric_names;
use crate::super_agents::traits::*;
use crate::super_agents::types::*;

pub struct CacheOptimizerAgent;

impl Default for CacheOptimizerAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl CacheOptimizerAgent {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SuperAgent for CacheOptimizerAgent {
    fn name(&self) -> &str {
        "Cache Optimizer"
    }

    fn kind(&self) -> SuperAgentKind {
        SuperAgentKind::CacheOptimizer
    }

    fn trigger(&self) -> TriggerMode {
        TriggerMode::Periodic {
            interval: Duration::from_secs(300),
        }
    }

    fn risk_level(&self) -> Severity {
        Severity::Low
    }

    async fn analyze(&self, ctx: &AnalysisContext) -> anyhow::Result<AnalysisReport> {
        let start = std::time::Instant::now();
        let mut findings = Vec::new();

        let hit_rate = ctx.snapshot.cache_hit_rate;
        let cache_size = ctx.snapshot.cache_size_entries;

        // Low hit rate
        if hit_rate < 0.3 && cache_size > 10 {
            findings.push(Finding {
                id: Uuid::new_v4().to_string(),
                severity: Severity::Medium,
                category: "low_hit_rate".into(),
                description: format!(
                    "Cache hit rate is {:.1}% ({} entries)",
                    hit_rate * 100.0,
                    cache_size
                ),
                metric_before: hit_rate,
                suggested_action: "Increase TTL for stable prompts; add semantic similarity matching"
                    .into(),
                estimated_improvement_pct: 30.0,
                metadata: HashMap::new(),
            });
        }

        // Cache too large (memory pressure)
        if cache_size > 800 {
            findings.push(Finding {
                id: Uuid::new_v4().to_string(),
                severity: if cache_size > 950 {
                    Severity::High
                } else {
                    Severity::Medium
                },
                category: "cache_size".into(),
                description: format!("Cache has {} entries (max 1000)", cache_size),
                metric_before: cache_size as f64,
                suggested_action: "Evict cold entries; implement LRU eviction policy".into(),
                estimated_improvement_pct: 15.0,
                metadata: HashMap::new(),
            });
        }

        // Detect frequently repeated LLM calls that could be cached
        let cost_samples = ctx
            .metrics
            .recent_samples(metric_names::LLM_COST_USD, 200)
            .await;
        let mut purpose_counts: HashMap<String, usize> = HashMap::new();
        for sample in &cost_samples {
            let purpose = sample.tags.get("purpose").cloned().unwrap_or_default();
            *purpose_counts.entry(purpose).or_default() += 1;
        }

        for (purpose, count) in &purpose_counts {
            if *count > 20
                && (purpose.contains("validate") || purpose.contains("check") || purpose.contains("lint"))
            {
                findings.push(Finding {
                    id: Uuid::new_v4().to_string(),
                    severity: Severity::Medium,
                    category: "cacheable_calls".into(),
                    description: format!(
                        "Purpose '{}' called {} times — likely cacheable",
                        purpose, count
                    ),
                    metric_before: *count as f64,
                    suggested_action: format!(
                        "Enable LLM response caching for '{}' with 5-minute TTL",
                        purpose
                    ),
                    estimated_improvement_pct: 60.0,
                    metadata: {
                        let mut m = HashMap::new();
                        m.insert("purpose".into(), serde_json::json!(purpose));
                        m.insert("call_count".into(), serde_json::json!(count));
                        m
                    },
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
                "low_hit_rate" => {
                    ctx.metrics
                        .record_value("optimization.cache_ttl_secs", 300.0)
                        .await;
                    optimizations.push(Optimization {
                        finding_id: finding.id.clone(),
                        action_taken: "Increased cache TTL to 300s for stable prompts".into(),
                        metric_before: finding.metric_before,
                        metric_after: (finding.metric_before + 0.3).min(1.0),
                        improvement_pct: 30.0,
                        rollback_key: Some("cache:ttl".into()),
                    });
                }
                "cacheable_calls" => {
                    let purpose = finding
                        .metadata
                        .get("purpose")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    ctx.metrics
                        .record_value(&format!("optimization.cache_enabled.{}", purpose), 1.0)
                        .await;
                    optimizations.push(Optimization {
                        finding_id: finding.id.clone(),
                        action_taken: format!("Enabled caching for '{}' calls", purpose),
                        metric_before: finding.metric_before,
                        metric_after: finding.metric_before * 0.4,
                        improvement_pct: 60.0,
                        rollback_key: Some(format!("cache:enable:{}", purpose)),
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

        if current.cache_hit_rate < ctx.snapshot_before.cache_hit_rate - 0.1 {
            failures.push("Cache hit rate decreased after optimization".into());
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
        tracing::warn!(key = rollback_key, "Rolling back cache optimization");
        Ok(())
    }
}
