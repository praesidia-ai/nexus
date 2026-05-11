//! Tests for the evaluation framework.

use chrono::Utc;

use crate::judge::LlmJudge;
use crate::regression::{RegressionDetector, Trend};
use crate::runner::EvalRunner;
use crate::suite::*;

// ── Suite serialization ─────────────────────────────────────────────────────

#[test]
fn suite_round_trip_json() {
    let suite = make_test_suite();
    let json = serde_json::to_string_pretty(&suite).unwrap();
    let parsed: EvalSuite = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.id, suite.id);
    assert_eq!(parsed.name, suite.name);
    assert_eq!(parsed.cases.len(), suite.cases.len());
    assert_eq!(parsed.category, suite.category);
}

#[test]
fn case_result_serialization() {
    let cr = CaseResult {
        case_id: "case-1".into(),
        passed: true,
        score: 0.85,
        duration_ms: 120,
        cost_usd: 0.003,
        error: None,
    };
    let json = serde_json::to_string(&cr).unwrap();
    let parsed: CaseResult = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.case_id, "case-1");
    assert!((parsed.score - 0.85).abs() < f32::EPSILON);
}

#[test]
fn eval_input_variants_serialize() {
    let inputs = vec![
        EvalInput::Description {
            text: "Build a todo app".into(),
        },
        EvalInput::IntentPair {
            utterance: "I want a landing page".into(),
            expected_intent: "create_landing".into(),
        },
        EvalInput::AgentTask {
            agent: "coder".into(),
            task: "implement auth".into(),
        },
        EvalInput::TeamTask {
            team: "full-stack".into(),
            task: "build dashboard".into(),
        },
        EvalInput::TasteJudgment {
            content: "<div class='hero'>Hello</div>".into(),
            expected_score: 0.7,
        },
    ];

    for input in &inputs {
        let json = serde_json::to_string(input).unwrap();
        let parsed: EvalInput = serde_json::from_str(&json).unwrap();
        // Just ensure it round-trips without error.
        let _ = serde_json::to_string(&parsed).unwrap();
    }
}

// ── Runner ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn runner_produces_results_for_all_cases() {
    let suite = make_test_suite();
    let runner = EvalRunner::new();
    let result = runner.run_suite(&suite).await;

    assert_eq!(result.suite_id, suite.id);
    assert_eq!(result.case_results.len(), suite.cases.len());
    assert!(result.overall_score >= 0.0 && result.overall_score <= 1.0);
    assert!(result.duration_total_ms < 10_000); // should be fast in heuristic mode

    for cr in &result.case_results {
        assert!(cr.score >= 0.0 && cr.score <= 1.0);
    }
}

#[tokio::test]
async fn runner_description_empty_fails() {
    let suite = EvalSuite {
        id: "empty-test".into(),
        name: "Empty Test".into(),
        description: "Test empty input".into(),
        category: EvalCategory::Generation,
        cases: vec![EvalCase {
            id: "empty".into(),
            name: "Empty description".into(),
            input: EvalInput::Description {
                text: "".into(),
            },
            expected: EvalExpectation::Builds,
            weight: 1.0,
            timeout_secs: 5,
        }],
    };

    let runner = EvalRunner::new();
    let result = runner.run_suite(&suite).await;

    assert!(!result.case_results[0].passed);
    assert!(result.case_results[0].error.is_some());
}

#[test]
fn weighted_score_calculation() {
    let cases = vec![
        EvalCase {
            id: "a".into(),
            name: "A".into(),
            input: EvalInput::Description { text: "x".into() },
            expected: EvalExpectation::Builds,
            weight: 2.0,
            timeout_secs: 10,
        },
        EvalCase {
            id: "b".into(),
            name: "B".into(),
            input: EvalInput::Description { text: "y".into() },
            expected: EvalExpectation::Builds,
            weight: 1.0,
            timeout_secs: 10,
        },
    ];

    let results = vec![
        CaseResult {
            case_id: "a".into(),
            passed: true,
            score: 0.9,
            duration_ms: 10,
            cost_usd: 0.0,
            error: None,
        },
        CaseResult {
            case_id: "b".into(),
            passed: true,
            score: 0.3,
            duration_ms: 10,
            cost_usd: 0.0,
            error: None,
        },
    ];

    let ws = EvalRunner::compute_weighted_score(&results, &cases);
    // (0.9*2 + 0.3*1) / (2+1) = 2.1/3 = 0.7
    assert!((ws - 0.7).abs() < 0.01);
}

#[test]
fn weighted_score_empty() {
    assert_eq!(EvalRunner::compute_weighted_score(&[], &[]), 0.0);
}

// ── Judge ───────────────────────────────────────────────────────────────────

#[test]
fn judge_scores_text() {
    let judge = LlmJudge::new();
    let text = "This function implements authentication using JWT tokens. \
                It validates the token, extracts the user ID, and returns \
                the authenticated user object.";
    let criteria = vec![
        "clarity".to_string(),
        "accuracy".to_string(),
        "completeness".to_string(),
    ];

    let score = judge.judge_text(text, &criteria);
    assert_eq!(score.scores.len(), 3);
    assert!(score.overall > 0.0);
    assert!(score.overall <= 1.0);

    for cs in &score.scores {
        assert!(!cs.explanation.is_empty());
        assert!(cs.score >= 0.0 && cs.score <= 1.0);
    }
}

#[test]
fn judge_empty_criteria() {
    let judge = LlmJudge::new();
    let score = judge.judge_text("hello", &[]);
    assert_eq!(score.scores.len(), 0);
    assert_eq!(score.overall, 0.0);
}

#[test]
fn judge_comparison() {
    let judge = LlmJudge::new();
    let a = "Short.";
    let b = "This is a much longer and more detailed explanation that covers \
             multiple aspects of the topic with clarity and structure. \
             It includes examples and technical terms like function, class, and module.";

    let criteria = vec!["completeness".to_string(), "clarity".to_string()];
    let result = judge.compare(a, b, &criteria);

    assert_eq!(result.winner, "b");
    assert!(result.scores_b > result.scores_a);
    assert!(!result.reasoning.is_empty());
}

// ── Regression detection ────────────────────────────────────────────────────

#[test]
fn regression_detects_drops() {
    let baseline = SuiteResult {
        suite_id: "s1".into(),
        timestamp: Utc::now(),
        overall_score: 0.8,
        case_results: vec![
            CaseResult {
                case_id: "c1".into(),
                passed: true,
                score: 0.9,
                duration_ms: 100,
                cost_usd: 0.01,
                error: None,
            },
            CaseResult {
                case_id: "c2".into(),
                passed: true,
                score: 0.8,
                duration_ms: 100,
                cost_usd: 0.01,
                error: None,
            },
        ],
        cost_total: 0.02,
        duration_total_ms: 200,
    };

    let current = SuiteResult {
        suite_id: "s1".into(),
        timestamp: Utc::now(),
        overall_score: 0.6,
        case_results: vec![
            CaseResult {
                case_id: "c1".into(),
                passed: false,
                score: 0.5, // 44% drop from 0.9
                duration_ms: 100,
                cost_usd: 0.01,
                error: None,
            },
            CaseResult {
                case_id: "c2".into(),
                passed: true,
                score: 0.85, // 6.25% improvement
                duration_ms: 100,
                cost_usd: 0.01,
                error: None,
            },
        ],
        cost_total: 0.02,
        duration_total_ms: 200,
    };

    let report = RegressionDetector::detect(&baseline, &current);

    assert!(report.has_regressions);
    assert_eq!(report.regressions.len(), 1);
    assert_eq!(report.regressions[0].case_id, "c1");
    assert!(report.regressions[0].drop_pct > 10.0);

    assert_eq!(report.improvements.len(), 1);
    assert_eq!(report.improvements[0].case_id, "c2");
}

#[test]
fn regression_stable_when_no_change() {
    let result = SuiteResult {
        suite_id: "s1".into(),
        timestamp: Utc::now(),
        overall_score: 0.8,
        case_results: vec![CaseResult {
            case_id: "c1".into(),
            passed: true,
            score: 0.8,
            duration_ms: 100,
            cost_usd: 0.01,
            error: None,
        }],
        cost_total: 0.01,
        duration_total_ms: 100,
    };

    let report = RegressionDetector::detect(&result, &result);
    assert!(!report.has_regressions);
    assert_eq!(report.overall_trend, Trend::Stable);
}

#[test]
fn regression_improving_trend() {
    let baseline = SuiteResult {
        suite_id: "s1".into(),
        timestamp: Utc::now(),
        overall_score: 0.5,
        case_results: vec![
            CaseResult {
                case_id: "c1".into(),
                passed: true,
                score: 0.5,
                duration_ms: 100,
                cost_usd: 0.0,
                error: None,
            },
            CaseResult {
                case_id: "c2".into(),
                passed: true,
                score: 0.5,
                duration_ms: 100,
                cost_usd: 0.0,
                error: None,
            },
        ],
        cost_total: 0.0,
        duration_total_ms: 200,
    };

    let current = SuiteResult {
        suite_id: "s1".into(),
        timestamp: Utc::now(),
        overall_score: 0.85,
        case_results: vec![
            CaseResult {
                case_id: "c1".into(),
                passed: true,
                score: 0.85,
                duration_ms: 100,
                cost_usd: 0.0,
                error: None,
            },
            CaseResult {
                case_id: "c2".into(),
                passed: true,
                score: 0.9,
                duration_ms: 100,
                cost_usd: 0.0,
                error: None,
            },
        ],
        cost_total: 0.0,
        duration_total_ms: 200,
    };

    let report = RegressionDetector::detect(&baseline, &current);
    assert!(!report.has_regressions);
    assert_eq!(report.overall_trend, Trend::Improving);
    assert_eq!(report.improvements.len(), 2);
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Build a small test suite with one case per input variant.
fn make_test_suite() -> EvalSuite {
    EvalSuite {
        id: "test-suite-001".into(),
        name: "Test Suite".into(),
        description: "Exercises all input types".into(),
        category: EvalCategory::Generation,
        cases: vec![
            EvalCase {
                id: "desc-1".into(),
                name: "Simple description".into(),
                input: EvalInput::Description {
                    text: "Build a responsive dashboard with auth, real-time charts, and a database backend".into(),
                },
                expected: EvalExpectation::Builds,
                weight: 1.0,
                timeout_secs: 10,
            },
            EvalCase {
                id: "intent-1".into(),
                name: "Intent classification".into(),
                input: EvalInput::IntentPair {
                    utterance: "I need a landing page for my startup".into(),
                    expected_intent: "create_landing".into(),
                },
                expected: EvalExpectation::FieldMatch {
                    checks: vec![FieldCheck {
                        field: "intent".into(),
                        expected: "create_landing".into(),
                    }],
                },
                weight: 1.0,
                timeout_secs: 10,
            },
            EvalCase {
                id: "agent-1".into(),
                name: "Agent task".into(),
                input: EvalInput::AgentTask {
                    agent: "coder".into(),
                    task: "Implement JWT authentication middleware".into(),
                },
                expected: EvalExpectation::AgentEfficiency {
                    max_iterations: 10,
                    max_cost: 0.50,
                },
                weight: 1.5,
                timeout_secs: 10,
            },
            EvalCase {
                id: "team-1".into(),
                name: "Team coordination".into(),
                input: EvalInput::TeamTask {
                    team: "full-stack".into(),
                    task: "Build an e-commerce checkout flow with payment integration".into(),
                },
                expected: EvalExpectation::LlmJudge {
                    criteria: vec!["completeness".into(), "structure".into()],
                },
                weight: 2.0,
                timeout_secs: 10,
            },
            EvalCase {
                id: "taste-1".into(),
                name: "Taste judgment".into(),
                input: EvalInput::TasteJudgment {
                    content: "<div class='hero'><h1>Welcome</h1><p>Modern SaaS platform</p></div>".into(),
                    expected_score: 0.5,
                },
                expected: EvalExpectation::QualityThreshold { min_taste: 0.5 },
                weight: 1.0,
                timeout_secs: 10,
            },
        ],
    }
}
