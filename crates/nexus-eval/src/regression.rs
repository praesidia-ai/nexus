//! Regression detection — compares two suite runs to find quality drops.

use serde::{Deserialize, Serialize};

use crate::suite::SuiteResult;

/// Threshold (as a fraction) below which a score drop is flagged as a regression.
const REGRESSION_THRESHOLD: f32 = 0.10;

/// Threshold above which a score increase is flagged as an improvement.
const IMPROVEMENT_THRESHOLD: f32 = 0.05;

/// Compares baseline and current suite results to detect regressions.
pub struct RegressionDetector;

/// Full regression report comparing two runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionReport {
    /// Cases that regressed beyond the threshold.
    pub regressions: Vec<Regression>,
    /// Cases that improved beyond the threshold.
    pub improvements: Vec<Improvement>,
    /// Overall trend direction.
    pub overall_trend: Trend,
    /// Convenience flag: `true` if any regressions were detected.
    pub has_regressions: bool,
}

/// A single case that regressed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Regression {
    /// The case that regressed.
    pub case_id: String,
    /// Score in the baseline run.
    pub baseline_score: f32,
    /// Score in the current run.
    pub current_score: f32,
    /// Percentage drop (positive number, e.g. 15.0 means 15% drop).
    pub drop_pct: f32,
}

/// A single case that improved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Improvement {
    /// The case that improved.
    pub case_id: String,
    /// Score in the baseline run.
    pub baseline_score: f32,
    /// Score in the current run.
    pub current_score: f32,
    /// Percentage gain (positive number).
    pub gain_pct: f32,
}

/// Overall trend direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trend {
    /// The current run is better overall.
    Improving,
    /// Scores are roughly the same.
    Stable,
    /// The current run is worse overall.
    Regressing,
}

impl RegressionDetector {
    /// Compare a baseline run against a current run and produce a report.
    ///
    /// Cases are matched by `case_id`. Cases present in only one run are
    /// silently skipped (they cannot regress or improve).
    pub fn detect(baseline: &SuiteResult, current: &SuiteResult) -> RegressionReport {
        let mut regressions = Vec::new();
        let mut improvements = Vec::new();

        // Build a lookup for baseline scores.
        let baseline_map: std::collections::HashMap<&str, f32> = baseline
            .case_results
            .iter()
            .map(|cr| (cr.case_id.as_str(), cr.score))
            .collect();

        for current_case in &current.case_results {
            let Some(&base_score) = baseline_map.get(current_case.case_id.as_str()) else {
                continue;
            };

            let diff = current_case.score - base_score;

            if base_score > 0.0 {
                let pct = (diff / base_score).abs();

                if diff < 0.0 && pct >= REGRESSION_THRESHOLD {
                    regressions.push(Regression {
                        case_id: current_case.case_id.clone(),
                        baseline_score: base_score,
                        current_score: current_case.score,
                        drop_pct: pct * 100.0,
                    });
                } else if diff > 0.0 && pct >= IMPROVEMENT_THRESHOLD {
                    improvements.push(Improvement {
                        case_id: current_case.case_id.clone(),
                        baseline_score: base_score,
                        current_score: current_case.score,
                        gain_pct: pct * 100.0,
                    });
                }
            } else if current_case.score > 0.0 {
                // Baseline was zero, any positive score is an improvement.
                improvements.push(Improvement {
                    case_id: current_case.case_id.clone(),
                    baseline_score: 0.0,
                    current_score: current_case.score,
                    gain_pct: 100.0,
                });
            }
        }

        let overall_trend = if !regressions.is_empty()
            && regressions.len() > improvements.len()
        {
            Trend::Regressing
        } else if !improvements.is_empty()
            && improvements.len() > regressions.len()
        {
            Trend::Improving
        } else {
            Trend::Stable
        };

        let has_regressions = !regressions.is_empty();

        RegressionReport {
            regressions,
            improvements,
            overall_trend,
            has_regressions,
        }
    }
}
