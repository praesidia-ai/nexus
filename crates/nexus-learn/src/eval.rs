//! Eval-gated skill promotion — patterns graduate to skills only when they pass
//! configurable quality thresholds.

use crate::pattern::Pattern;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub trigger_pattern: String,
    pub action_template: String,
    pub promoted_from: String,
    pub eval_score: f64,
    pub status: SkillStatus,
    pub created_at: String,
    pub promoted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SkillStatus {
    Candidate,
    Evaluating,
    Promoted,
    Rejected,
    Deprecated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResult {
    pub skill_id: String,
    pub test_cases: usize,
    pub passed: usize,
    pub failed: usize,
    pub score: f64,
    pub promoted: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionCriteria {
    pub min_success_rate: f64,
    pub min_occurrences: u32,
    pub min_eval_score: f64,
    pub max_avg_duration_ms: u64,
}

impl Default for PromotionCriteria {
    fn default() -> Self {
        Self {
            min_success_rate: 0.85,
            min_occurrences: 10,
            min_eval_score: 0.80,
            max_avg_duration_ms: 30_000,
        }
    }
}

/// Check if a pattern qualifies for promotion to a skill.
pub fn evaluate_for_promotion(pattern: &Pattern, criteria: &PromotionCriteria) -> EvalResult {
    let mut reasons = Vec::new();
    let mut score = 0.0;

    if pattern.success_rate >= criteria.min_success_rate {
        score += 0.4;
    } else {
        reasons.push(format!(
            "Success rate {:.1}% below {:.1}%",
            pattern.success_rate * 100.0,
            criteria.min_success_rate * 100.0
        ));
    }

    if pattern.occurrences >= criteria.min_occurrences {
        score += 0.3;
    } else {
        reasons.push(format!(
            "Only {} occurrences (need {})",
            pattern.occurrences, criteria.min_occurrences
        ));
    }

    if pattern.avg_duration_ms <= criteria.max_avg_duration_ms {
        score += 0.3;
    } else {
        reasons.push(format!(
            "Average duration {}ms exceeds {}ms",
            pattern.avg_duration_ms, criteria.max_avg_duration_ms
        ));
    }

    let promoted = score >= criteria.min_eval_score;
    let reason = if promoted {
        "Pattern meets all promotion criteria".to_string()
    } else {
        format!("Not promoted: {}", reasons.join("; "))
    };

    let passed = (pattern.occurrences as f64 * pattern.success_rate) as usize;
    EvalResult {
        skill_id: pattern.id.clone(),
        test_cases: pattern.occurrences as usize,
        passed,
        failed: pattern.occurrences as usize - passed,
        score,
        promoted,
        reason,
    }
}

/// Promote a pattern to a skill.
pub fn promote_pattern(pattern: &Pattern) -> Skill {
    Skill {
        id: uuid::Uuid::new_v4().to_string(),
        name: pattern.name.clone(),
        description: pattern.description.clone(),
        trigger_pattern: pattern.trigger.clone(),
        action_template: pattern.action_sequence.join(" -> "),
        promoted_from: pattern.id.clone(),
        eval_score: pattern.success_rate,
        status: SkillStatus::Promoted,
        created_at: chrono::Utc::now().to_rfc3339(),
        promoted_at: Some(chrono::Utc::now().to_rfc3339()),
    }
}
