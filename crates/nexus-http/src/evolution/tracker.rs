//! Improvement Tracker — collects and stores execution metrics.
//!
//! Every coding task that runs through the orchestrator produces an
//! ExecutionRecord which is stored and analyzed for patterns.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::coding_agents::types::TaskSummary;

use super::types::*;

/// Stores execution records in a JSON Lines file under `.nexus/evolution/`.
pub struct ImprovementTracker {
    data_dir: PathBuf,
}

impl ImprovementTracker {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    fn evolution_dir(&self) -> PathBuf {
        self.data_dir.join(".nexus").join("evolution")
    }

    fn records_path(&self) -> PathBuf {
        self.evolution_dir().join("execution_records.jsonl")
    }

    fn state_path(&self) -> PathBuf {
        self.evolution_dir().join("evolution_state.json")
    }

    /// Record a completed coding task execution.
    pub fn record_execution(
        &self,
        summary: &TaskSummary,
        task_description: &str,
        project_id: &str,
    ) -> anyhow::Result<ExecutionRecord> {
        let _ = std::fs::create_dir_all(self.evolution_dir());

        let outcome = match summary.status.as_str() {
            "completed" => ExecutionOutcome::Success,
            "completed_with_errors" => ExecutionOutcome::PartialSuccess,
            "failed" => ExecutionOutcome::Failure,
            _ => ExecutionOutcome::Failure,
        };

        let agent_phases: Vec<PhaseRecord> = summary
            .agent_stats
            .iter()
            .map(|(agent, stats)| PhaseRecord {
                phase: agent.clone(),
                agent: agent.clone(),
                iterations: stats.iterations,
                tools_used: stats.tools_used.clone(),
                files_touched: stats.files_touched.clone(),
                duration_ms: stats.duration_ms,
                errors: Vec::new(),
                success: true,
            })
            .collect();

        let record = ExecutionRecord {
            id: uuid::Uuid::new_v4().to_string(),
            task_description: task_description.to_string(),
            project_id: project_id.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            duration_ms: summary.duration_ms,
            outcome,
            agent_phases,
            metrics: ExecutionMetrics {
                total_llm_calls: summary.total_iterations,
                total_tokens_used: 0,
                total_tool_calls: 0,
                total_files_changed: (summary.files_created.len()
                    + summary.files_modified.len()
                    + summary.files_deleted.len())
                    as u32,
                total_retries: summary.total_retries,
                verification_attempts: summary.total_retries + 1,
                verification_passed: summary.verification_passed,
                estimated_cost_usd: 0.0,
            },
            patterns_detected: Vec::new(),
        };

        if let Ok(line) = serde_json::to_string(&record) {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.records_path())
            {
                let _ = writeln!(f, "{}", line);
            }
        }

        info!(
            record_id = %record.id,
            outcome = ?outcome,
            duration_ms = record.duration_ms,
            "Execution record saved"
        );

        Ok(record)
    }

    /// Load all execution records.
    pub fn load_records(&self) -> Vec<ExecutionRecord> {
        let path = self.records_path();
        if !path.exists() {
            return Vec::new();
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        content
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    /// Load the evolution state.
    pub fn load_state(&self) -> EvolutionState {
        let path = self.state_path();
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(state) = serde_json::from_str(&content) {
                    return state;
                }
            }
        }
        EvolutionState::default()
    }

    /// Save the evolution state.
    pub fn save_state(&self, state: &EvolutionState) -> anyhow::Result<()> {
        let _ = std::fs::create_dir_all(self.evolution_dir());
        let json = serde_json::to_string_pretty(state)?;
        std::fs::write(self.state_path(), json)?;
        Ok(())
    }

    /// Compute aggregate metrics from all records.
    pub fn compute_metrics(&self) -> AggregateMetrics {
        let records = self.load_records();
        if records.is_empty() {
            return AggregateMetrics::default();
        }

        let total = records.len() as f64;
        let successes = records
            .iter()
            .filter(|r| r.outcome == ExecutionOutcome::Success)
            .count() as f64;
        let partial = records
            .iter()
            .filter(|r| r.outcome == ExecutionOutcome::PartialSuccess)
            .count() as f64;

        let avg_duration =
            records.iter().map(|r| r.duration_ms as f64).sum::<f64>() / total;
        let avg_iterations = records
            .iter()
            .map(|r| r.metrics.total_llm_calls as f64)
            .sum::<f64>()
            / total;
        let avg_files = records
            .iter()
            .map(|r| r.metrics.total_files_changed as f64)
            .sum::<f64>()
            / total;
        let avg_retries = records
            .iter()
            .map(|r| r.metrics.total_retries as f64)
            .sum::<f64>()
            / total;

        AggregateMetrics {
            total_executions: records.len() as u64,
            success_rate: (successes + partial * 0.5) / total,
            avg_duration_ms: avg_duration,
            avg_iterations,
            avg_files_changed: avg_files,
            avg_retries,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AggregateMetrics {
    pub total_executions: u64,
    pub success_rate: f64,
    pub avg_duration_ms: f64,
    pub avg_iterations: f64,
    pub avg_files_changed: f64,
    pub avg_retries: f64,
}
