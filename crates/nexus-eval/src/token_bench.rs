//! Token-efficiency benchmark — the "5× fewer tokens than Claude Code
//! on the same tasks" moonshot from NEXUS_MASTER_PLAN §11.
//!
//! The suite lives at `crates/nexus-eval/benchmarks/v1/*.json`. Each
//! file is a single task with a prompt + an expected-shape oracle.
//! The runner produces a [`ScoreboardRun`] which is serialised into
//! `web/public/bench/scoreboard.json` for the frontend to render.
//!
//! Why live in-tree rather than in a separate repo: having the suite
//! under the same CI means every commit generates a fresh scoreboard
//! without manual ceremony, and the "published" number stays
//! reproducible at any git SHA. The runner is intentionally
//! model-client-agnostic — the caller injects a closure that actually
//! talks to Claude / Nexus / Codex / anything else.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A single task loaded off disk. Tasks are intentionally narrow —
/// one prompt, a short expected output shape, and the eval-rubric
/// language telling a judge what "good" looks like.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchTask {
    pub id: String,
    /// Optional category tag — "crud" | "bugfix" | "refactor" | "test"
    pub category: String,
    pub prompt: String,
    /// Free-form rubric a human (or LLM judge) applies when grading.
    pub rubric: String,
}

/// Raw result of running a single task through one candidate system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub candidate: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    #[serde(with = "duration_ms")]
    pub duration: Duration,
    /// Judge score on 0..=5 — `None` if not yet graded.
    pub quality: Option<u8>,
    /// Optional freeform note from the judge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub judge_note: Option<String>,
}

impl TaskResult {
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

/// One row per candidate system on the scoreboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateRow {
    pub candidate: String,
    pub tasks_attempted: usize,
    pub median_total_tokens: u64,
    pub p95_total_tokens: u64,
    pub median_cost_usd: f64,
    /// Mean judge score on 0..=5 across attempted tasks. `None` when
    /// at least one task is ungraded.
    pub mean_quality: Option<f64>,
}

/// The shape written to `web/public/bench/scoreboard.json`. The UI
/// parses it straight — no server call required.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreboardRun {
    /// ISO-8601 timestamp the run was finalised.
    pub ran_at: String,
    /// Version of the benchmark suite (bumped whenever tasks change).
    pub suite_version: String,
    pub tasks_total: usize,
    pub rows: Vec<CandidateRow>,
    /// Per-task raw samples so reviewers can drill in.
    #[serde(default)]
    pub samples: Vec<TaskResult>,
}

#[derive(Debug, Error)]
pub enum BenchError {
    #[error("failed to read suite dir {0}: {1}")]
    SuiteDir(PathBuf, std::io::Error),
    #[error("failed to parse task {0}: {1}")]
    BadTask(PathBuf, serde_json::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// Load every `*.json` task under `dir` (non-recursive).
pub fn load_suite(dir: &Path) -> Result<Vec<BenchTask>, BenchError> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| BenchError::SuiteDir(dir.to_path_buf(), e))?;
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let body = std::fs::read_to_string(&path)?;
        let task: BenchTask = serde_json::from_str(&body)
            .map_err(|e| BenchError::BadTask(path.clone(), e))?;
        out.push(task);
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

/// Aggregate raw samples into one row per candidate. Medians use the
/// simple middle-of-sorted rule (no interpolation for tied sizes —
/// statistics prefer a reproducible tie-break).
pub fn aggregate(samples: &[TaskResult], suite_version: impl Into<String>) -> ScoreboardRun {
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<String, Vec<&TaskResult>> = BTreeMap::new();
    for s in samples {
        groups.entry(s.candidate.clone()).or_default().push(s);
    }
    let tasks_total = samples
        .iter()
        .map(|s| s.task_id.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .len();

    let mut rows = Vec::new();
    for (candidate, items) in groups {
        let mut tokens: Vec<u64> = items.iter().map(|s| s.total_tokens()).collect();
        let mut costs: Vec<f64> = items.iter().map(|s| s.cost_usd).collect();
        tokens.sort();
        costs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median_total_tokens = median_u64(&tokens);
        let p95_total_tokens = percentile_u64(&tokens, 0.95);
        let median_cost_usd = median_f64(&costs);
        let graded: Vec<u8> = items.iter().filter_map(|s| s.quality).collect();
        let mean_quality = if graded.len() == items.len() && !graded.is_empty() {
            Some(graded.iter().map(|q| *q as f64).sum::<f64>() / graded.len() as f64)
        } else {
            None
        };
        rows.push(CandidateRow {
            candidate,
            tasks_attempted: items.len(),
            median_total_tokens,
            p95_total_tokens,
            median_cost_usd,
            mean_quality,
        });
    }
    rows.sort_by_key(|r| r.median_total_tokens);
    ScoreboardRun {
        ran_at: chrono::Utc::now().to_rfc3339(),
        suite_version: suite_version.into(),
        tasks_total,
        rows,
        samples: samples.to_vec(),
    }
}

fn median_u64(sorted: &[u64]) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    sorted[sorted.len() / 2]
}

fn median_f64(sorted: &[f64]) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[sorted.len() / 2]
}

fn percentile_u64(sorted: &[u64], pct: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * pct).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

mod duration_ms {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;
    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        (d.as_millis() as u64).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        Ok(Duration::from_millis(u64::deserialize(d)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample(candidate: &str, task: &str, tokens: u64, cost: f64, quality: u8) -> TaskResult {
        TaskResult {
            task_id: task.into(),
            candidate: candidate.into(),
            input_tokens: tokens / 2,
            output_tokens: tokens - tokens / 2,
            cost_usd: cost,
            duration: Duration::from_millis(100),
            quality: Some(quality),
            judge_note: None,
        }
    }

    #[test]
    fn aggregate_produces_one_row_per_candidate_sorted_by_median_tokens() {
        let samples = vec![
            sample("nexus", "t1", 1_000, 0.01, 4),
            sample("nexus", "t2", 2_000, 0.02, 5),
            sample("claude-code", "t1", 4_000, 0.04, 5),
            sample("claude-code", "t2", 8_000, 0.08, 4),
        ];
        let run = aggregate(&samples, "v1");
        assert_eq!(run.rows.len(), 2);
        assert_eq!(run.tasks_total, 2);
        assert_eq!(run.rows[0].candidate, "nexus");
        assert_eq!(run.rows[1].candidate, "claude-code");
        assert!(run.rows[0].median_total_tokens < run.rows[1].median_total_tokens);
    }

    #[test]
    fn mean_quality_is_none_when_any_task_ungraded() {
        let mut s1 = sample("nexus", "t1", 1_000, 0.01, 4);
        s1.quality = None;
        let run = aggregate(&[s1, sample("nexus", "t2", 2_000, 0.02, 5)], "v1");
        assert_eq!(run.rows[0].mean_quality, None);
    }

    #[test]
    fn load_suite_reads_json_tasks_in_sorted_id_order() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("b.json"),
            r#"{"id":"t-02","category":"crud","prompt":"x","rubric":"y"}"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("a.json"),
            r#"{"id":"t-01","category":"crud","prompt":"x","rubric":"y"}"#,
        )
        .unwrap();
        let tasks = load_suite(dir.path()).unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, "t-01");
        assert_eq!(tasks[1].id, "t-02");
    }

    #[test]
    fn percentile_handles_small_samples() {
        assert_eq!(percentile_u64(&[100], 0.95), 100);
        assert_eq!(percentile_u64(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 0.95), 10);
    }

    #[test]
    fn median_of_empty_is_zero() {
        assert_eq!(median_u64(&[]), 0);
        assert_eq!(median_f64(&[]), 0.0);
    }
}
