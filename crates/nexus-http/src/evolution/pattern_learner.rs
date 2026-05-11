//! Pattern Learning — detects recurring patterns in execution history.
//!
//! Analyzes execution records to find:
//! - Tool sequences that lead to success
//! - Common error patterns and their fixes
//! - Optimal iteration counts per task type
//! - Project conventions that should be followed

use std::collections::HashMap;

use tracing::info;

use super::tracker::ImprovementTracker;
use super::types::*;

/// Analyze execution history and detect patterns.
pub fn detect_patterns(tracker: &ImprovementTracker) -> Vec<DetectedPattern> {
    let records = tracker.load_records();
    if records.len() < 3 {
        return Vec::new();
    }

    let mut patterns = Vec::new();

    patterns.extend(detect_success_patterns(&records));
    patterns.extend(detect_failure_patterns(&records));
    patterns.extend(detect_optimal_iterations(&records));
    patterns.extend(detect_tool_sequences(&records));

    info!(
        patterns_found = patterns.len(),
        records_analyzed = records.len(),
        "Pattern detection completed"
    );

    patterns
}

/// Find strategies that consistently lead to success.
fn detect_success_patterns(records: &[ExecutionRecord]) -> Vec<DetectedPattern> {
    let mut patterns = Vec::new();
    let successes: Vec<_> = records
        .iter()
        .filter(|r| r.outcome == ExecutionOutcome::Success)
        .collect();

    if successes.len() < 2 {
        return patterns;
    }

    let mut phase_counts: HashMap<String, u32> = HashMap::new();
    for record in &successes {
        for phase in &record.agent_phases {
            *phase_counts.entry(phase.agent.clone()).or_default() += 1;
        }
    }

    let total_success = successes.len() as f64;
    for (agent, count) in &phase_counts {
        let rate = *count as f64 / total_success;
        if rate > 0.8 {
            patterns.push(DetectedPattern {
                id: format!("success_agent_{}", agent),
                pattern_type: PatternType::SuccessfulStrategy,
                description: format!(
                    "Agent '{}' participates in {:.0}% of successful tasks",
                    agent,
                    rate * 100.0,
                ),
                confidence: rate,
                occurrences: *count,
                context: HashMap::new(),
                first_seen: successes.first().map(|r| r.timestamp.clone()).unwrap_or_default(),
                last_seen: successes.last().map(|r| r.timestamp.clone()).unwrap_or_default(),
            });
        }
    }

    let avg_success_iterations = successes
        .iter()
        .map(|r| r.metrics.total_llm_calls as f64)
        .sum::<f64>()
        / total_success;

    if avg_success_iterations > 0.0 {
        patterns.push(DetectedPattern {
            id: "optimal_iteration_count".to_string(),
            pattern_type: PatternType::SuccessfulStrategy,
            description: format!(
                "Successful tasks average {:.1} iterations",
                avg_success_iterations,
            ),
            confidence: 0.7,
            occurrences: successes.len() as u32,
            context: {
                let mut ctx = HashMap::new();
                ctx.insert(
                    "avg_iterations".to_string(),
                    format!("{:.1}", avg_success_iterations),
                );
                ctx
            },
            first_seen: successes.first().map(|r| r.timestamp.clone()).unwrap_or_default(),
            last_seen: successes.last().map(|r| r.timestamp.clone()).unwrap_or_default(),
        });
    }

    patterns
}

/// Find patterns that commonly lead to failure.
fn detect_failure_patterns(records: &[ExecutionRecord]) -> Vec<DetectedPattern> {
    let mut patterns = Vec::new();
    let failures: Vec<_> = records
        .iter()
        .filter(|r| r.outcome == ExecutionOutcome::Failure)
        .collect();

    if failures.len() < 2 {
        return patterns;
    }

    let mut error_counts: HashMap<String, u32> = HashMap::new();
    for record in &failures {
        for phase in &record.agent_phases {
            for error in &phase.errors {
                let normalized = normalize_error(error);
                *error_counts.entry(normalized).or_default() += 1;
            }
        }
    }

    for (error_pattern, count) in &error_counts {
        if *count >= 2 {
            patterns.push(DetectedPattern {
                id: format!("failure_{}", hash_string(error_pattern)),
                pattern_type: PatternType::FailurePattern,
                description: format!("Recurring error: {} (seen {} times)", error_pattern, count),
                confidence: (*count as f64) / (failures.len() as f64),
                occurrences: *count,
                context: HashMap::new(),
                first_seen: failures.first().map(|r| r.timestamp.clone()).unwrap_or_default(),
                last_seen: failures.last().map(|r| r.timestamp.clone()).unwrap_or_default(),
            });
        }
    }

    let high_retry_failures = failures
        .iter()
        .filter(|r| r.metrics.total_retries > 2)
        .count();

    if high_retry_failures > 1 {
        patterns.push(DetectedPattern {
            id: "high_retry_failure".to_string(),
            pattern_type: PatternType::FailurePattern,
            description: format!(
                "{} failures had >2 retries — debugger loop may be ineffective",
                high_retry_failures,
            ),
            confidence: 0.6,
            occurrences: high_retry_failures as u32,
            context: HashMap::new(),
            first_seen: failures.first().map(|r| r.timestamp.clone()).unwrap_or_default(),
            last_seen: failures.last().map(|r| r.timestamp.clone()).unwrap_or_default(),
        });
    }

    patterns
}

/// Detect optimal iteration counts per agent phase.
fn detect_optimal_iterations(records: &[ExecutionRecord]) -> Vec<DetectedPattern> {
    let mut patterns = Vec::new();
    let successes: Vec<_> = records
        .iter()
        .filter(|r| r.outcome == ExecutionOutcome::Success)
        .collect();

    if successes.len() < 3 {
        return patterns;
    }

    let mut agent_iterations: HashMap<String, Vec<u32>> = HashMap::new();
    for record in &successes {
        for phase in &record.agent_phases {
            agent_iterations
                .entry(phase.agent.clone())
                .or_default()
                .push(phase.iterations);
        }
    }

    for (agent, iters) in &agent_iterations {
        if iters.len() >= 3 {
            let avg = iters.iter().map(|i| *i as f64).sum::<f64>() / iters.len() as f64;
            let max = *iters.iter().max().unwrap_or(&0);

            let optimal_limit = ((avg * 1.5) as u32).max(max).min(50);

            patterns.push(DetectedPattern {
                id: format!("optimal_iter_{}", agent),
                pattern_type: PatternType::OptimalToolSequence,
                description: format!(
                    "Agent '{}' succeeds with avg {:.1} iterations (recommend max: {})",
                    agent, avg, optimal_limit,
                ),
                confidence: 0.75,
                occurrences: iters.len() as u32,
                context: {
                    let mut ctx = HashMap::new();
                    ctx.insert("avg_iterations".to_string(), format!("{:.1}", avg));
                    ctx.insert("recommended_max".to_string(), optimal_limit.to_string());
                    ctx
                },
                first_seen: successes.first().map(|r| r.timestamp.clone()).unwrap_or_default(),
                last_seen: successes.last().map(|r| r.timestamp.clone()).unwrap_or_default(),
            });
        }
    }

    patterns
}

/// Detect commonly used tool sequences in successful executions.
fn detect_tool_sequences(records: &[ExecutionRecord]) -> Vec<DetectedPattern> {
    let mut patterns = Vec::new();
    let successes: Vec<_> = records
        .iter()
        .filter(|r| r.outcome == ExecutionOutcome::Success)
        .collect();

    if successes.len() < 3 {
        return patterns;
    }

    let mut tool_freq: HashMap<String, u32> = HashMap::new();
    for record in &successes {
        for phase in &record.agent_phases {
            for tool in &phase.tools_used {
                *tool_freq.entry(tool.clone()).or_default() += 1;
            }
        }
    }

    let mut sorted_tools: Vec<_> = tool_freq.into_iter().collect();
    sorted_tools.sort_by(|a, b| b.1.cmp(&a.1));

    if sorted_tools.len() >= 3 {
        let top_tools: Vec<String> = sorted_tools
            .iter()
            .take(5)
            .map(|(t, c)| format!("{}(x{})", t, c))
            .collect();

        patterns.push(DetectedPattern {
            id: "top_tools_in_success".to_string(),
            pattern_type: PatternType::OptimalToolSequence,
            description: format!("Most used tools in successful tasks: {}", top_tools.join(", ")),
            confidence: 0.8,
            occurrences: successes.len() as u32,
            context: HashMap::new(),
            first_seen: successes.first().map(|r| r.timestamp.clone()).unwrap_or_default(),
            last_seen: successes.last().map(|r| r.timestamp.clone()).unwrap_or_default(),
        });
    }

    patterns
}

fn normalize_error(error: &str) -> String {
    let lower = error.to_lowercase();
    if lower.contains("type") && lower.contains("error") {
        "type_error".to_string()
    } else if lower.contains("import") || lower.contains("module not found") {
        "import_error".to_string()
    } else if lower.contains("syntax") {
        "syntax_error".to_string()
    } else if lower.contains("timeout") {
        "timeout_error".to_string()
    } else if lower.contains("not found") {
        "not_found_error".to_string()
    } else {
        let truncated = if error.len() > 50 {
            &error[..50]
        } else {
            error
        };
        truncated.to_string()
    }
}

fn hash_string(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}
