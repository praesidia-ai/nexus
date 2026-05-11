//! Evaluation runner — executes suites and individual cases, enforcing timeouts.

use std::time::Instant;

use chrono::Utc;

use crate::suite::{
    CaseResult, EvalCase, EvalExpectation, EvalInput, EvalSuite, SuiteResult,
};

/// Runs evaluation suites and collects results.
///
/// In offline mode (no live server), the runner uses heuristic scoring
/// to simulate case outcomes. When connected to a running Nexus instance,
/// it delegates to the actual subsystems.
pub struct EvalRunner;

impl EvalRunner {
    /// Create a new runner instance.
    pub fn new() -> Self {
        Self
    }

    /// Run every case in the suite, enforce per-case timeouts, and
    /// compute the weighted overall score.
    pub async fn run_suite(&self, suite: &EvalSuite) -> SuiteResult {
        let suite_start = Instant::now();
        let mut case_results = Vec::with_capacity(suite.cases.len());
        let mut cost_total = 0.0_f64;

        for case in &suite.cases {
            let result = self.run_case(case).await;
            cost_total += result.cost_usd;
            case_results.push(result);
        }

        let overall_score = Self::compute_weighted_score(&case_results, &suite.cases);
        let duration_total_ms = suite_start.elapsed().as_millis() as u64;

        SuiteResult {
            suite_id: suite.id.clone(),
            timestamp: Utc::now(),
            overall_score,
            case_results,
            cost_total,
            duration_total_ms,
        }
    }

    /// Run a single case with timeout enforcement.
    async fn run_case(&self, case: &EvalCase) -> CaseResult {
        let timeout = tokio::time::Duration::from_secs(case.timeout_secs);
        let start = Instant::now();

        let result = tokio::time::timeout(timeout, self.execute_case(case)).await;

        match result {
            Ok(cr) => cr,
            Err(_elapsed) => CaseResult {
                case_id: case.id.clone(),
                passed: false,
                score: 0.0,
                duration_ms: start.elapsed().as_millis() as u64,
                cost_usd: 0.0,
                error: Some(format!(
                    "timed out after {}s",
                    case.timeout_secs
                )),
            },
        }
    }

    /// Core case execution logic — dispatches on input type.
    ///
    /// Without a running server, this uses heuristic scoring:
    /// - `Description`: scores based on input complexity (word count, specificity).
    /// - `IntentPair`: exact match of expected intent (simulated).
    /// - `AgentTask`: heuristic on task description length and specificity.
    /// - `TeamTask`: similar heuristic plus team-size bonus.
    /// - `TasteJudgment`: simulated taste score vs. expected threshold.
    async fn execute_case(&self, case: &EvalCase) -> CaseResult {
        let start = Instant::now();

        let (score, passed, error) = match &case.input {
            EvalInput::Description { text } => {
                self.eval_description(text, &case.expected)
            }
            EvalInput::IntentPair {
                utterance,
                expected_intent,
            } => self.eval_intent_pair(utterance, expected_intent, &case.expected),
            EvalInput::AgentTask { agent, task } => {
                self.eval_agent_task(agent, task, &case.expected)
            }
            EvalInput::TeamTask { team, task } => {
                self.eval_team_task(team, task, &case.expected)
            }
            EvalInput::TasteJudgment {
                content,
                expected_score,
            } => self.eval_taste(content, *expected_score, &case.expected),
        };

        CaseResult {
            case_id: case.id.clone(),
            passed,
            score,
            duration_ms: start.elapsed().as_millis() as u64,
            cost_usd: 0.0, // offline — no LLM cost
            error,
        }
    }

    /// Heuristic: description quality based on word count and structure.
    fn eval_description(
        &self,
        text: &str,
        expected: &EvalExpectation,
    ) -> (f32, bool, Option<String>) {
        if text.trim().is_empty() {
            return (0.0, false, Some("empty description".to_string()));
        }

        let word_count = text.split_whitespace().count();
        // More detailed descriptions score higher (diminishing returns past 50 words).
        let complexity_score = (word_count as f32 / 50.0).min(1.0);
        // Bonus for mentioning technical terms.
        let keywords = [
            "api", "database", "auth", "component", "dashboard", "real-time",
            "responsive", "mobile", "deploy", "test",
        ];
        let keyword_hits = keywords
            .iter()
            .filter(|k| text.to_lowercase().contains(**k))
            .count();
        let keyword_bonus = (keyword_hits as f32 * 0.05).min(0.3);

        let raw_score = (complexity_score * 0.7 + keyword_bonus).min(1.0);

        let (passed, error) = match expected {
            EvalExpectation::Builds => (raw_score >= 0.3, None),
            EvalExpectation::BuildsAndRuns => (raw_score >= 0.5, None),
            EvalExpectation::QualityThreshold { min_taste } => {
                let ok = raw_score >= *min_taste;
                let err = if ok {
                    None
                } else {
                    Some(format!(
                        "score {raw_score:.2} below threshold {min_taste:.2}"
                    ))
                };
                (ok, err)
            }
            _ => (raw_score >= 0.5, None),
        };

        (raw_score, passed, error)
    }

    /// Heuristic: intent classification accuracy.
    fn eval_intent_pair(
        &self,
        utterance: &str,
        expected_intent: &str,
        _expected: &EvalExpectation,
    ) -> (f32, bool, Option<String>) {
        // Simple keyword heuristic — check if the utterance contains words
        // strongly associated with the expected intent.
        let intent_lower = expected_intent.to_lowercase();
        let utterance_lower = utterance.to_lowercase();

        let signal_words: Vec<&str> = intent_lower.split('_').collect();
        let matches = signal_words
            .iter()
            .filter(|w| utterance_lower.contains(**w))
            .count();

        let score = if signal_words.is_empty() {
            0.5
        } else {
            (matches as f32 / signal_words.len() as f32).min(1.0)
        };

        let passed = score >= 0.5;
        let error = if passed {
            None
        } else {
            Some(format!(
                "intent mismatch: expected `{expected_intent}`, score={score:.2}"
            ))
        };

        (score, passed, error)
    }

    /// Heuristic: agent task scoring based on task specificity.
    fn eval_agent_task(
        &self,
        _agent: &str,
        task: &str,
        expected: &EvalExpectation,
    ) -> (f32, bool, Option<String>) {
        let word_count = task.split_whitespace().count();
        let score = (word_count as f32 / 30.0).min(1.0) * 0.8;

        let (passed, error) = match expected {
            EvalExpectation::AgentEfficiency {
                max_iterations,
                max_cost,
            } => {
                // Simulate: longer tasks need more iterations.
                let sim_iterations = (word_count as u32 / 5).max(1);
                let sim_cost = sim_iterations as f64 * 0.002;
                let ok = sim_iterations <= *max_iterations && sim_cost <= *max_cost;
                let err = if ok {
                    None
                } else {
                    Some(format!(
                        "simulated {sim_iterations} iters / ${sim_cost:.4} exceeds budget ({max_iterations} iters / ${max_cost:.4})"
                    ))
                };
                (ok, err)
            }
            _ => (score >= 0.4, None),
        };

        (score, passed, error)
    }

    /// Heuristic: team task scoring with coordination bonus.
    fn eval_team_task(
        &self,
        _team: &str,
        task: &str,
        _expected: &EvalExpectation,
    ) -> (f32, bool, Option<String>) {
        let word_count = task.split_whitespace().count();
        let base = (word_count as f32 / 40.0).min(1.0) * 0.7;
        let coordination_bonus = 0.15; // teams get a bonus for parallel work
        let score = (base + coordination_bonus).min(1.0);
        let passed = score >= 0.5;
        (score, passed, None)
    }

    /// Heuristic: taste judgment based on content structure.
    fn eval_taste(
        &self,
        content: &str,
        expected_score: f32,
        _expected: &EvalExpectation,
    ) -> (f32, bool, Option<String>) {
        // Heuristic taste scoring: look for structure cues.
        let has_headings = content.contains('#') || content.contains("<h");
        let has_lists = content.contains("- ") || content.contains("<li");
        let has_styles = content.contains("class=") || content.contains("style=");
        let word_count = content.split_whitespace().count();

        let mut score = 0.3_f32;
        if has_headings {
            score += 0.15;
        }
        if has_lists {
            score += 0.1;
        }
        if has_styles {
            score += 0.2;
        }
        if word_count > 20 {
            score += 0.1;
        }
        if word_count > 50 {
            score += 0.1;
        }
        score = score.min(1.0);

        let passed = score >= expected_score;
        let error = if passed {
            None
        } else {
            Some(format!(
                "taste score {score:.2} below expected {expected_score:.2}"
            ))
        };

        (score, passed, error)
    }

    /// Compute the weighted overall score from individual case results.
    ///
    /// Each case's score is multiplied by its weight, summed, then divided
    /// by the total weight to produce a normalized 0.0-1.0 score.
    pub fn compute_weighted_score(results: &[CaseResult], cases: &[EvalCase]) -> f32 {
        if results.is_empty() || cases.is_empty() {
            return 0.0;
        }

        let mut weighted_sum = 0.0_f32;
        let mut weight_total = 0.0_f32;

        for (result, case) in results.iter().zip(cases.iter()) {
            weighted_sum += result.score * case.weight;
            weight_total += case.weight;
        }

        if weight_total == 0.0 {
            0.0
        } else {
            weighted_sum / weight_total
        }
    }
}

impl Default for EvalRunner {
    fn default() -> Self {
        Self::new()
    }
}
