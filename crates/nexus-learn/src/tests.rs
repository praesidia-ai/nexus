use chrono::Utc;

use crate::eval::{self, PromotionCriteria, SkillStatus};
use crate::outcome::{FeedbackType, Outcome, OutcomeStore};
use crate::pattern;

fn make_outcome(id: &str, action: &str, success: bool, duration_ms: u64) -> Outcome {
    Outcome {
        id: id.to_string(),
        agent_id: "agent-1".to_string(),
        project_id: "proj-1".to_string(),
        action: action.to_string(),
        tool_used: Some("file_write".to_string()),
        input_summary: "test input".to_string(),
        output_summary: "test output".to_string(),
        success,
        quality_score: Some(0.9),
        user_feedback: None,
        duration_ms,
        tokens_used: 500,
        timestamp: Utc::now(),
        context: serde_json::json!({"key": "value"}),
    }
}

// ---------------------------------------------------------------------------
// OutcomeStore tests
// ---------------------------------------------------------------------------

#[test]
fn outcome_store_records_and_retrieves() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("learn.db");
    let store = OutcomeStore::new(&db_path).unwrap();

    let outcome = make_outcome("o1", "code_gen", true, 1000);
    store.record(&outcome).unwrap();

    let recent = store.outcomes_by_agent("agent-1", 10);
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].id, "o1");
    assert_eq!(recent[0].action, "code_gen");
    assert!(recent[0].success);
}

#[test]
fn outcome_store_records_with_feedback() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("learn.db");
    let store = OutcomeStore::new(&db_path).unwrap();

    let mut outcome = make_outcome("o2", "review", true, 500);
    outcome.user_feedback = Some(FeedbackType::Positive);
    store.record(&outcome).unwrap();

    let mut corrected = make_outcome("o3", "review", false, 800);
    corrected.user_feedback = Some(FeedbackType::Corrected {
        correction: "Should have used a different approach".to_string(),
    });
    store.record(&corrected).unwrap();
}

#[test]
fn outcome_store_success_rate() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("learn.db");
    let store = OutcomeStore::new(&db_path).unwrap();

    store
        .record(&make_outcome("s1", "deploy", true, 100))
        .unwrap();
    store
        .record(&make_outcome("s2", "deploy", true, 100))
        .unwrap();
    store
        .record(&make_outcome("s3", "deploy", false, 100))
        .unwrap();

    let rate = store.success_rate("agent-1", "deploy");
    assert!((rate - 2.0 / 3.0).abs() < 0.01);
}

#[test]
fn outcome_store_success_rate_zero_when_no_data() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("learn.db");
    let store = OutcomeStore::new(&db_path).unwrap();

    let rate = store.success_rate("nonexistent", "nope");
    assert!((rate - 0.0).abs() < f64::EPSILON);
}

#[test]
fn outcome_store_recent_failures() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("learn.db");
    let store = OutcomeStore::new(&db_path).unwrap();

    store
        .record(&make_outcome("f1", "test", true, 100))
        .unwrap();
    store
        .record(&make_outcome("f2", "test", false, 200))
        .unwrap();
    store
        .record(&make_outcome("f3", "build", false, 300))
        .unwrap();

    let failures = store.recent_failures("proj-1", 10);
    assert_eq!(failures.len(), 2);
    assert!(failures.iter().all(|o| !o.success));
}

// ---------------------------------------------------------------------------
// Pattern extraction tests
// ---------------------------------------------------------------------------

#[test]
fn pattern_extraction_requires_minimum_occurrences() {
    let outcomes: Vec<Outcome> = (0..2)
        .map(|i| make_outcome(&format!("pe{i}"), "rare_action", true, 100))
        .collect();

    let result = pattern::extract_patterns(&outcomes);
    assert_eq!(result.patterns_found.len(), 0);
    assert_eq!(result.outcomes_analyzed, 2);
}

#[test]
fn pattern_extraction_finds_recurring_successes() {
    let outcomes: Vec<Outcome> = (0..5)
        .map(|i| make_outcome(&format!("pe{i}"), "code_gen", true, 1000))
        .collect();

    let result = pattern::extract_patterns(&outcomes);
    assert_eq!(result.patterns_found.len(), 1);
    assert_eq!(result.outcomes_analyzed, 5);
    assert_eq!(result.new_patterns, 1);

    let p = &result.patterns_found[0];
    assert_eq!(p.trigger, "code_gen");
    assert!((p.success_rate - 1.0).abs() < f64::EPSILON);
    assert_eq!(p.occurrences, 5);
    assert_eq!(p.avg_duration_ms, 1000);
}

#[test]
fn pattern_extraction_skips_low_success_rate() {
    let mut outcomes: Vec<Outcome> = (0..3)
        .map(|i| make_outcome(&format!("lo{i}"), "flaky_action", true, 100))
        .collect();
    for i in 3..10 {
        outcomes.push(make_outcome(&format!("lo{i}"), "flaky_action", false, 100));
    }

    let result = pattern::extract_patterns(&outcomes);
    assert_eq!(
        result.patterns_found.len(),
        0,
        "Actions with <70% success should not produce patterns"
    );
}

#[test]
fn pattern_extraction_multiple_actions() {
    let mut outcomes = Vec::new();
    for i in 0..4 {
        outcomes.push(make_outcome(&format!("a{i}"), "code_gen", true, 500));
    }
    for i in 0..4 {
        outcomes.push(make_outcome(&format!("b{i}"), "test_run", true, 300));
    }

    let result = pattern::extract_patterns(&outcomes);
    assert_eq!(result.patterns_found.len(), 2);
    let triggers: Vec<&str> = result.patterns_found.iter().map(|p| p.trigger.as_str()).collect();
    assert!(triggers.contains(&"code_gen"));
    assert!(triggers.contains(&"test_run"));
}

// ---------------------------------------------------------------------------
// Eval-gated promotion tests
// ---------------------------------------------------------------------------

#[test]
fn eval_promotes_qualifying_pattern() {
    let p = pattern::Pattern {
        id: "p1".to_string(),
        name: "pattern_deploy".to_string(),
        description: "Deploy pattern".to_string(),
        trigger: "deploy".to_string(),
        action_sequence: vec!["deploy".to_string()],
        success_rate: 0.95,
        occurrences: 20,
        avg_duration_ms: 5000,
        avg_tokens: 1000,
        first_seen: Utc::now().to_rfc3339(),
        last_seen: Utc::now().to_rfc3339(),
    };

    let criteria = PromotionCriteria::default();
    let result = eval::evaluate_for_promotion(&p, &criteria);

    assert!(result.promoted);
    assert!((result.score - 1.0).abs() < f64::EPSILON);
    assert_eq!(result.reason, "Pattern meets all promotion criteria");
}

#[test]
fn eval_rejects_low_success_rate() {
    let p = pattern::Pattern {
        id: "p2".to_string(),
        name: "pattern_flaky".to_string(),
        description: "Flaky pattern".to_string(),
        trigger: "flaky".to_string(),
        action_sequence: vec!["flaky".to_string()],
        success_rate: 0.60,
        occurrences: 50,
        avg_duration_ms: 1000,
        avg_tokens: 200,
        first_seen: Utc::now().to_rfc3339(),
        last_seen: Utc::now().to_rfc3339(),
    };

    let criteria = PromotionCriteria::default();
    let result = eval::evaluate_for_promotion(&p, &criteria);

    assert!(!result.promoted);
    assert!(result.reason.contains("Success rate"));
}

#[test]
fn eval_rejects_insufficient_occurrences() {
    let p = pattern::Pattern {
        id: "p3".to_string(),
        name: "pattern_rare".to_string(),
        description: "Rare pattern".to_string(),
        trigger: "rare".to_string(),
        action_sequence: vec!["rare".to_string()],
        success_rate: 0.95,
        occurrences: 3,
        avg_duration_ms: 1000,
        avg_tokens: 200,
        first_seen: Utc::now().to_rfc3339(),
        last_seen: Utc::now().to_rfc3339(),
    };

    let criteria = PromotionCriteria::default();
    let result = eval::evaluate_for_promotion(&p, &criteria);

    assert!(!result.promoted);
    assert!(result.reason.contains("occurrences"));
}

#[test]
fn eval_rejects_slow_pattern() {
    let p = pattern::Pattern {
        id: "p4".to_string(),
        name: "pattern_slow".to_string(),
        description: "Slow pattern".to_string(),
        trigger: "slow".to_string(),
        action_sequence: vec!["slow".to_string()],
        success_rate: 0.95,
        occurrences: 20,
        avg_duration_ms: 60_000,
        avg_tokens: 200,
        first_seen: Utc::now().to_rfc3339(),
        last_seen: Utc::now().to_rfc3339(),
    };

    let criteria = PromotionCriteria::default();
    let result = eval::evaluate_for_promotion(&p, &criteria);

    assert!(!result.promoted);
    assert!(result.reason.contains("duration"));
}

#[test]
fn promote_pattern_creates_skill() {
    let p = pattern::Pattern {
        id: "p5".to_string(),
        name: "pattern_code_gen".to_string(),
        description: "Code generation pattern".to_string(),
        trigger: "code_gen".to_string(),
        action_sequence: vec!["plan".to_string(), "generate".to_string(), "test".to_string()],
        success_rate: 0.92,
        occurrences: 25,
        avg_duration_ms: 3000,
        avg_tokens: 800,
        first_seen: Utc::now().to_rfc3339(),
        last_seen: Utc::now().to_rfc3339(),
    };

    let skill = eval::promote_pattern(&p);
    assert_eq!(skill.name, "pattern_code_gen");
    assert_eq!(skill.promoted_from, "p5");
    assert_eq!(skill.status, SkillStatus::Promoted);
    assert_eq!(skill.action_template, "plan -> generate -> test");
    assert!(skill.promoted_at.is_some());
}

// ---------------------------------------------------------------------------
// Full pipeline: outcomes -> patterns -> eval -> promotion
// ---------------------------------------------------------------------------

#[test]
fn full_pipeline_outcomes_to_skill() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("pipeline.db");
    let store = OutcomeStore::new(&db_path).unwrap();

    for i in 0..15 {
        let success = i % 8 != 0; // 13/15 = 86.7% success
        let outcome = make_outcome(&format!("pipe{i}"), "code_review", success, 2000);
        store.record(&outcome).unwrap();
    }

    let outcomes = store.outcomes_by_agent("agent-1", 100);
    let extraction = pattern::extract_patterns(&outcomes);
    assert_eq!(extraction.patterns_found.len(), 1);

    let p = &extraction.patterns_found[0];
    let criteria = PromotionCriteria::default();
    let eval_result = eval::evaluate_for_promotion(p, &criteria);

    assert!(
        eval_result.promoted,
        "15 outcomes with ~86% success should promote: {}",
        eval_result.reason
    );

    let skill = eval::promote_pattern(p);
    assert_eq!(skill.status, SkillStatus::Promoted);
    assert!(skill.eval_score > criteria.min_success_rate);
}
