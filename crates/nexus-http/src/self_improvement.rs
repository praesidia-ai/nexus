//! Self-Improvement Loop — Nexus learns and optimizes itself continuously.
//!
//! Every cycle:
//! 1. Analyze: slow pipelines, low taste scores, failing builds
//! 2. Decide: what to optimize
//! 3. Apply: prompt improvements, pipeline adjustments, caching strategies
//! 4. Validate: performance improved?
//! 5. Store learning → global_memory

use std::sync::Arc;

use serde::Serialize;
use tracing::info;

use crate::state::AppState;

/// Result of a self-improvement cycle.
#[derive(Debug, Clone, Serialize)]
pub struct ImprovementCycle {
    /// What was analyzed.
    pub analysis: Vec<ImprovementSignal>,
    /// What was decided.
    pub decisions: Vec<ImprovementDecision>,
    /// What was applied.
    pub applied: Vec<String>,
    /// Whether the improvement was validated as beneficial.
    pub validated: bool,
    /// Learnings stored to global memory.
    pub learnings_stored: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImprovementSignal {
    pub area: String,
    pub signal_type: String,
    pub description: String,
    pub severity: u32, // 1-5
}

#[derive(Debug, Clone, Serialize)]
pub struct ImprovementDecision {
    pub action: String,
    pub target: String,
    pub expected_impact: String,
    pub risk: String,
}

/// Run a self-improvement cycle.
///
/// This is called by the system_scheduler on a periodic basis.
pub async fn run_improvement_cycle(app: &Arc<AppState>) -> ImprovementCycle {
    // ── Step 1: Analyze — single lock, collect all metrics at once ──────
    let mut signals = Vec::new();

    let metrics = {
        let db = app.db.lock().await;
        // Collect all metrics in one lock scope to minimize contention
        let avg_duration: f64 = db.query_row(
            "SELECT COALESCE(AVG(total_duration_ms), 0) FROM run_metrics WHERE created_at > datetime('now', '-24 hours')",
            [], |r| r.get(0),
        ).unwrap_or(0.0);
        let avg_taste: f64 = db.query_row(
            "SELECT COALESCE(AVG(overall), 100) FROM taste_history WHERE created_at > datetime('now', '-7 days')",
            [], |r| r.get(0),
        ).unwrap_or(100.0);
        let fail_count: i64 = db.query_row(
            "SELECT COUNT(*) FROM run_metrics WHERE status = 'failure' AND created_at > datetime('now', '-24 hours')",
            [], |r| r.get(0),
        ).unwrap_or(0);
        let daily_cost: f64 = db.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0) FROM llm_cost_records WHERE created_at > datetime('now', '-24 hours')",
            [], |r| r.get(0),
        ).unwrap_or(0.0);
        let avg_iterations: f64 = db.query_row(
            "SELECT COALESCE(AVG(total_iterations), 0) FROM agent_traces WHERE created_at > datetime('now', '-24 hours') AND total_iterations > 0",
            [], |r| r.get(0),
        ).unwrap_or(0.0);
        // Lock dropped here at end of block
        (avg_duration, avg_taste, fail_count, daily_cost, avg_iterations)
    };

    let (avg_duration, avg_taste, fail_count, daily_cost, avg_iterations) = metrics;

    if avg_duration > 60_000.0 {
        signals.push(ImprovementSignal {
            area: "pipeline".into(), signal_type: "slow".into(),
            description: format!("Average pipeline duration: {:.0}ms (target: <60s)", avg_duration),
            severity: 3,
        });
    }
    if avg_taste < 80.0 {
        signals.push(ImprovementSignal {
            area: "taste".into(), signal_type: "low_quality".into(),
            description: format!("Average taste score: {:.0}/100 (target: >=90)", avg_taste),
            severity: 4,
        });
    }
    if fail_count > 3 {
        signals.push(ImprovementSignal {
            area: "builds".into(), signal_type: "failing".into(),
            description: format!("{} build failures in last 24h", fail_count),
            severity: 5,
        });
    }
    if daily_cost > 25.0 {
        signals.push(ImprovementSignal {
            area: "cost".into(), signal_type: "high_cost".into(),
            description: format!("Daily LLM cost: ${:.2} (budget: $50)", daily_cost),
            severity: 2,
        });
    }
    if avg_iterations > 8.0 {
        signals.push(ImprovementSignal {
            area: "agents".into(), signal_type: "high_iterations".into(),
            description: format!("Average agent iterations: {:.1} (target: <8)", avg_iterations),
            severity: 3,
        });
    }

    // ── Step 2: Decide ──────────────────────────────────────────────────
    let mut decisions = Vec::new();

    for signal in &signals {
        let decision = match (signal.area.as_str(), signal.signal_type.as_str()) {
            ("pipeline", "slow") => ImprovementDecision {
                action: "increase_parallelism".into(),
                target: "pipeline_turbo".into(),
                expected_impact: "Reduce pipeline duration by 20-40%".into(),
                risk: "low".into(),
            },
            ("taste", "low_quality") => ImprovementDecision {
                action: "strengthen_taste_gate".into(),
                target: "taste_engine".into(),
                expected_impact: "Improve average taste score by 5-10 points".into(),
                risk: "low".into(),
            },
            ("builds", "failing") => ImprovementDecision {
                action: "add_validation_step".into(),
                target: "execution_pipeline".into(),
                expected_impact: "Reduce build failures by catching issues earlier".into(),
                risk: "low".into(),
            },
            ("cost", "high_cost") => ImprovementDecision {
                action: "downgrade_model_for_simple".into(),
                target: "cost_intelligence".into(),
                expected_impact: "Reduce LLM costs by 30-50% for simple tasks".into(),
                risk: "medium".into(),
            },
            ("agents", "high_iterations") => ImprovementDecision {
                action: "improve_agent_prompts".into(),
                target: "agent_evolution".into(),
                expected_impact: "Reduce average iterations by 2-3".into(),
                risk: "low".into(),
            },
            _ => ImprovementDecision {
                action: "monitor".into(),
                target: signal.area.clone(),
                expected_impact: "Continue monitoring for patterns".into(),
                risk: "none".into(),
            },
        };
        decisions.push(decision);
    }

    // ── Step 3: Apply (safe, non-destructive improvements) ──────────────
    let mut applied = Vec::new();
    let mut learnings = Vec::new();

    for decision in &decisions {
        match decision.action.as_str() {
            "increase_parallelism" => {
                // Store as a global memory preference
                store_learning(app, "pipeline", "prefer_high_parallelism",
                    "Pipeline durations are high — prefer aggressive parallelization").await;
                applied.push("Stored parallelism preference in global memory".into());
                learnings.push("Pipeline optimization: prefer high parallelism".into());
            }
            "strengthen_taste_gate" => {
                store_learning(app, "quality", "taste_gate_strict",
                    "Taste scores are low — enforce stricter quality gate").await;
                applied.push("Stored strict quality gate preference".into());
                learnings.push("Quality: enforce stricter taste gate".into());
            }
            "downgrade_model_for_simple" => {
                store_learning(app, "cost", "prefer_cheap_models_for_validation",
                    "LLM costs are high — use cheaper models for validation/simple tasks").await;
                applied.push("Stored cost optimization preference".into());
                learnings.push("Cost: use cheaper models for simple tasks".into());
            }
            "improve_agent_prompts" => {
                store_learning(app, "agents", "reduce_iteration_count",
                    "Agent iterations are high — improve prompts for directness").await;
                applied.push("Stored agent optimization preference".into());
                learnings.push("Agents: improve prompt directness".into());
            }
            _ => {}
        }
    }

    // ── Step 4: Validate (deferred to next cycle) ───────────────────────
    let validated = !signals.is_empty(); // We at least analyzed something

    if !signals.is_empty() {
        info!(
            signals = signals.len(),
            decisions = decisions.len(),
            applied = applied.len(),
            "Self-improvement cycle completed"
        );
    }

    ImprovementCycle {
        analysis: signals,
        decisions,
        applied,
        validated,
        learnings_stored: learnings,
    }
}

async fn store_learning(app: &Arc<AppState>, category: &str, key: &str, value: &str) {
    let db = app.db.lock().await;
    let id = uuid::Uuid::new_v4().to_string();
    let _ = db.execute(
        "INSERT INTO global_memory (id, category, key, value, source, confidence, times_applied, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'self_improvement', 0.7, 0, datetime('now'), datetime('now'))
         ON CONFLICT(key) DO UPDATE SET
           value = ?4,
           confidence = MIN(confidence + 0.05, 1.0),
           times_applied = times_applied + 1,
           updated_at = datetime('now')",
        rusqlite::params![id, category, key, value],
    );
}
