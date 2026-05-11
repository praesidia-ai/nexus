//! Pattern extraction — discovers recurring successful action sequences from outcomes.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub id: String,
    pub name: String,
    pub description: String,
    pub trigger: String,
    pub action_sequence: Vec<String>,
    pub success_rate: f64,
    pub occurrences: u32,
    pub avg_duration_ms: u64,
    pub avg_tokens: u64,
    pub first_seen: String,
    pub last_seen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionResult {
    pub patterns_found: Vec<Pattern>,
    pub outcomes_analyzed: usize,
    pub new_patterns: usize,
    pub updated_patterns: usize,
}

/// Extract patterns from a batch of outcomes.
///
/// Groups by action, requires at least 3 occurrences, and only keeps
/// patterns with >70% success rate.
pub fn extract_patterns(outcomes: &[crate::outcome::Outcome]) -> ExtractionResult {
    let mut action_counts: HashMap<String, Vec<&crate::outcome::Outcome>> = HashMap::new();
    for outcome in outcomes {
        action_counts
            .entry(outcome.action.clone())
            .or_default()
            .push(outcome);
    }

    let mut patterns = Vec::new();
    for (action, group) in &action_counts {
        if group.len() < 3 {
            continue;
        }

        let successes = group.iter().filter(|o| o.success).count();
        let success_rate = successes as f64 / group.len() as f64;
        let avg_duration = group.iter().map(|o| o.duration_ms).sum::<u64>() / group.len() as u64;
        let avg_tokens = group.iter().map(|o| o.tokens_used).sum::<u64>() / group.len() as u64;

        if success_rate > 0.7 {
            patterns.push(Pattern {
                id: uuid::Uuid::new_v4().to_string(),
                name: format!("pattern_{action}"),
                description: format!("Recurring pattern for action: {action}"),
                trigger: action.clone(),
                action_sequence: vec![action.clone()],
                success_rate,
                occurrences: group.len() as u32,
                avg_duration_ms: avg_duration,
                avg_tokens,
                first_seen: group
                    .iter()
                    .map(|o| o.timestamp)
                    .min()
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_default(),
                last_seen: group
                    .iter()
                    .map(|o| o.timestamp)
                    .max()
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_default(),
            });
        }
    }

    let new_count = patterns.len();
    ExtractionResult {
        patterns_found: patterns,
        outcomes_analyzed: outcomes.len(),
        new_patterns: new_count,
        updated_patterns: 0,
    }
}
