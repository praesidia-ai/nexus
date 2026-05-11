//! Auto-Optimization Engine — applies learned improvements safely.
//!
//! Takes detected patterns and generates concrete improvements to:
//! - Agent iteration limits
//! - Prompt effectiveness
//! - Model routing decisions
//! - Error recovery strategies

use tracing::info;

use super::pattern_learner;
use super::tracker::ImprovementTracker;
use super::types::*;

/// Run the evolution cycle: analyze history, detect patterns, propose improvements.
pub fn evolve(tracker: &ImprovementTracker) -> EvolutionResult {
    let mut state = tracker.load_state();
    let records = tracker.load_records();

    if records.is_empty() {
        return EvolutionResult {
            patterns_detected: Vec::new(),
            improvements_proposed: Vec::new(),
            improvements_applied: Vec::new(),
            new_version: state.version,
        };
    }

    let patterns = pattern_learner::detect_patterns(tracker);

    let improvements = generate_improvements(&state, &patterns, &records);

    let mut applied = Vec::new();
    for improvement in &improvements {
        if improvement.confidence >= 0.7
            && apply_improvement(&mut state, improvement) {
                applied.push(improvement.id.clone());
            }
    }

    state.version += 1;
    state.total_executions = records.len() as u64;
    state.patterns = patterns.clone();
    state.improvements.extend(improvements.clone());
    state.last_evolution_at = chrono::Utc::now().to_rfc3339();

    let metrics = tracker.compute_metrics();
    state.success_rate = metrics.success_rate;
    state.avg_duration_ms = metrics.avg_duration_ms;
    state.avg_iterations = metrics.avg_iterations;

    if let Err(e) = tracker.save_state(&state) {
        tracing::warn!(error = %e, "Failed to save evolution state");
    }

    info!(
        version = state.version,
        patterns = patterns.len(),
        improvements = improvements.len(),
        applied = applied.len(),
        "Evolution cycle completed"
    );

    EvolutionResult {
        patterns_detected: patterns,
        improvements_proposed: improvements,
        improvements_applied: applied,
        new_version: state.version,
    }
}

#[derive(Debug, Clone)]
pub struct EvolutionResult {
    pub patterns_detected: Vec<DetectedPattern>,
    pub improvements_proposed: Vec<Improvement>,
    pub improvements_applied: Vec<String>,
    pub new_version: u32,
}

/// Generate concrete improvements from detected patterns.
fn generate_improvements(
    state: &EvolutionState,
    patterns: &[DetectedPattern],
    records: &[ExecutionRecord],
) -> Vec<Improvement> {
    let mut improvements = Vec::new();
    let timestamp = chrono::Utc::now().to_rfc3339();

    for pattern in patterns {
        match pattern.pattern_type {
            PatternType::OptimalToolSequence => {
                if let Some(recommended_max) = pattern.context.get("recommended_max") {
                    if let Some(agent) = pattern.id.strip_prefix("optimal_iter_") {
                        let existing = state.config_overrides.get(&format!(
                            "agent.{}.max_iterations",
                            agent
                        ));
                        if existing.is_none() {
                            improvements.push(Improvement {
                                id: format!("iter_limit_{}_{}", agent, state.version + 1),
                                improvement_type: ImprovementType::IterationLimitTuning,
                                description: format!(
                                    "Set {} agent max iterations to {} based on {} successful runs",
                                    agent, recommended_max, pattern.occurrences,
                                ),
                                version: state.version + 1,
                                confidence: pattern.confidence,
                                impact_estimate: 0.1,
                                applied: false,
                                created_at: timestamp.clone(),
                                evidence: vec![pattern.description.clone()],
                                rollback_data: existing.map(|v| v.to_string()),
                            });
                        }
                    }
                }
            }
            PatternType::FailurePattern => {
                if pattern.id == "high_retry_failure" && pattern.confidence > 0.5 {
                    improvements.push(Improvement {
                        id: format!("retry_strategy_{}", state.version + 1),
                        improvement_type: ImprovementType::ErrorRecoveryStrategy,
                        description: "Increase debugger max iterations for retry-heavy failures"
                            .to_string(),
                        version: state.version + 1,
                        confidence: pattern.confidence,
                        impact_estimate: 0.15,
                        applied: false,
                        created_at: timestamp.clone(),
                        evidence: vec![pattern.description.clone()],
                        rollback_data: None,
                    });
                }
            }
            PatternType::SuccessfulStrategy => {
                if let Some(avg) = pattern.context.get("avg_iterations") {
                    if let Ok(avg_val) = avg.parse::<f64>() {
                        if avg_val < 10.0 {
                            improvements.push(Improvement {
                                id: format!("fast_path_{}", state.version + 1),
                                improvement_type: ImprovementType::IterationLimitTuning,
                                description: format!(
                                    "Tasks succeed quickly ({:.1} avg iterations) — \
                                     consider reducing timeouts to save cost",
                                    avg_val,
                                ),
                                version: state.version + 1,
                                confidence: pattern.confidence * 0.8,
                                impact_estimate: 0.05,
                                applied: false,
                                created_at: timestamp.clone(),
                                evidence: vec![pattern.description.clone()],
                                rollback_data: None,
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let success_count = records
        .iter()
        .filter(|r| r.outcome == ExecutionOutcome::Success)
        .count();
    let total = records.len();
    if total >= 10 && success_count as f64 / total as f64 > 0.8 {
        let has_model_route = state.improvements.iter().any(|i| {
            i.improvement_type == ImprovementType::ModelRouting && i.applied
        });
        if !has_model_route {
            let avg_cost: f64 = records
                .iter()
                .map(|r| r.metrics.estimated_cost_usd)
                .sum::<f64>()
                / total as f64;

            if avg_cost > 0.5 {
                improvements.push(Improvement {
                    id: format!("model_route_{}", state.version + 1),
                    improvement_type: ImprovementType::ModelRouting,
                    description: format!(
                        "High success rate ({:.0}%) with avg cost ${:.2} — \
                         consider routing simpler tasks to a cheaper model",
                        (success_count as f64 / total as f64) * 100.0,
                        avg_cost,
                    ),
                    version: state.version + 1,
                    confidence: 0.6,
                    impact_estimate: 0.2,
                    applied: false,
                    created_at: timestamp.clone(),
                    evidence: vec![format!(
                        "{}/{} tasks succeeded, avg cost ${:.2}",
                        success_count, total, avg_cost
                    )],
                    rollback_data: None,
                });
            }
        }
    }

    improvements
}

/// Apply a single improvement to the evolution state.
fn apply_improvement(state: &mut EvolutionState, improvement: &Improvement) -> bool {
    match improvement.improvement_type {
        ImprovementType::IterationLimitTuning => {
            if let Some(agent) = improvement.id.split('_').nth(2) {
                let key = format!("agent.{}.max_iterations", agent);
                if let Some(recommended) = improvement
                    .description
                    .split("to ")
                    .nth(1)
                    .and_then(|s| s.split(' ').next())
                    .and_then(|s| s.parse::<u32>().ok())
                {
                    state
                        .config_overrides
                        .insert(key, serde_json::json!(recommended));
                    info!(
                        agent = agent,
                        max_iterations = recommended,
                        "Applied iteration limit tuning"
                    );
                    return true;
                }
            }
            false
        }
        ImprovementType::ErrorRecoveryStrategy => {
            state.config_overrides.insert(
                "debugger.max_iterations".to_string(),
                serde_json::json!(30),
            );
            info!("Applied error recovery strategy improvement");
            true
        }
        _ => {
            info!(
                improvement_type = ?improvement.improvement_type,
                "Improvement type not auto-applicable — stored for manual review"
            );
            false
        }
    }
}
