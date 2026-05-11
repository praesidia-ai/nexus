//! Default in-memory implementation of [`LearningStore`].
//!
//! Stores learning signals in memory for the lifetime of the process.
//! Suitable for development and testing. Production deployments should
//! use a persistent backing store (e.g., SQLite via `nexus-store`).

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use super::learning::*;

/// In-memory learning store backed by a `Mutex<Vec<LearningSignal>>`.
///
/// Provides basic aggregation queries (best_choice, insights) by
/// scanning the full signal list. Not optimized for large datasets.
pub struct InMemoryLearningStore {
    signals: Mutex<Vec<LearningSignal>>,
}

impl InMemoryLearningStore {
    pub fn new() -> Self {
        Self {
            signals: Mutex::new(Vec::new()),
        }
    }
}

impl Default for InMemoryLearningStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LearningStore for InMemoryLearningStore {
    async fn record(
        &self,
        signal: &LearningSignal,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut signals = self
            .signals
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        signals.push(signal.clone());
        Ok(())
    }

    async fn best_choice(
        &self,
        area: &str,
        _app_type: &str,
        tenant_id: &str,
    ) -> Result<Option<LearnedPreference>, Box<dyn std::error::Error + Send + Sync>> {
        let signals = self
            .signals
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;

        // Collect stack decisions for this area and tenant
        let mut choice_stats: HashMap<String, (usize, f32)> = HashMap::new();

        for signal in signals.iter() {
            if signal.tenant_id != tenant_id {
                continue;
            }
            if let SignalType::StackDecision {
                area: ref sig_area,
                ref choice,
            } = signal.signal_type
            {
                if sig_area == area {
                    let entry = choice_stats.entry(choice.clone()).or_insert((0, 0.0));
                    entry.0 += 1;
                    entry.1 += signal.quality;
                }
            }
        }

        if choice_stats.is_empty() {
            return Ok(None);
        }

        // Find the choice with the highest average quality
        let best = choice_stats
            .into_iter()
            .map(|(choice, (count, total_quality))| {
                let avg = total_quality / count as f32;
                (choice, count, avg)
            })
            .max_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

        match best {
            Some((choice, count, avg_quality)) => Ok(Some(LearnedPreference {
                choice: choice.clone(),
                confidence: (count as f32 / 10.0).min(1.0), // caps at 10 samples
                sample_size: count,
                avg_quality,
                explanation: format!(
                    "'{choice}' has avg quality {avg_quality:.2} across {count} projects for area '{area}'"
                ),
            })),
            None => Ok(None),
        }
    }

    async fn insights(
        &self,
        tenant_id: &str,
    ) -> Result<LearningInsights, Box<dyn std::error::Error + Send + Sync>> {
        let signals = self
            .signals
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;

        let tenant_signals: Vec<&LearningSignal> =
            signals.iter().filter(|s| s.tenant_id == tenant_id).collect();

        let total_signals = tenant_signals.len();

        // Aggregate stack insights
        let mut stack_map: HashMap<(String, String), (usize, usize)> = HashMap::new();
        let mut failure_map: HashMap<String, usize> = HashMap::new();

        for signal in &tenant_signals {
            match &signal.signal_type {
                SignalType::StackDecision { area, choice } => {
                    let entry = stack_map
                        .entry((area.clone(), choice.clone()))
                        .or_insert((0, 0));
                    entry.0 += 1; // total
                    if signal.quality >= 0.7 {
                        entry.1 += 1; // successes
                    }
                }
                SignalType::GuaranteeResult { passed, .. } if !passed => {
                    *failure_map.entry("guarantee_failure".to_string()).or_insert(0) += 1;
                }
                SignalType::BuildSuccess { first_try } if !first_try => {
                    *failure_map.entry("build_retry".to_string()).or_insert(0) += 1;
                }
                _ => {}
            }
        }

        let top_stacks = stack_map
            .into_iter()
            .map(|((area, choice), (total, successes))| StackInsight {
                area,
                choice,
                success_rate: if total > 0 {
                    successes as f32 / total as f32
                } else {
                    0.0
                },
                usage_count: total,
            })
            .collect();

        let common_failures = failure_map
            .into_iter()
            .map(|(pattern, frequency)| FailurePattern { pattern, frequency })
            .collect();

        Ok(LearningInsights {
            top_stacks,
            common_failures,
            total_signals,
        })
    }
}
