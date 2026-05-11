//! Self-improvement loop: metrics collection, feedback storage, and run analytics.
//!
//! This module powers the self-improvement cycle:
//! ```text
//! Workflow Run → Metrics Collected → Feedback Stored → Patterns Detected →
//! IR Updated → System Regenerated
//! ```
//!
//! Three services:
//! - `MetricsService` — records per-step and per-run execution metrics
//! - `FeedbackService` — stores user satisfaction signals and corrections
//! - `AnalyticsService` — queries stored metrics for improvement signals

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;

// ---------------------------------------------------------------------------
// Step Metric
// ---------------------------------------------------------------------------

/// Execution metrics for a single workflow step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepMetric {
    pub id: String,
    pub run_id: String,
    pub step_id: String,
    pub agent: String,
    pub duration_ms: i64,
    pub retries_used: i32,
    pub skipped: bool,
    pub output_length: i64,
    /// "success" | "failure" | "skipped"
    pub status: String,
    pub error: Option<String>,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// Run Metric (aggregate)
// ---------------------------------------------------------------------------

/// Aggregate metrics for a complete workflow run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMetric {
    pub id: String,
    pub project_id: Option<String>,
    pub pipeline: String,
    pub total_duration_ms: i64,
    pub steps_total: i32,
    pub steps_succeeded: i32,
    pub steps_failed: i32,
    pub steps_skipped: i32,
    pub total_retries: i32,
    /// "success" | "partial" | "failure"
    pub status: String,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// Feedback
// ---------------------------------------------------------------------------

/// User feedback on a workflow run or generated output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    pub id: String,
    pub project_id: Option<String>,
    pub run_id: Option<String>,
    pub step_id: Option<String>,
    /// 1–5 star rating (0 = not rated).
    pub rating: i32,
    /// Free-text correction or comment.
    pub comment: Option<String>,
    /// "positive" | "negative" | "correction" | "neutral"
    pub signal: String,
    pub created_at: String,
}

/// Input for creating new feedback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewFeedback {
    pub project_id: Option<String>,
    pub run_id: Option<String>,
    pub step_id: Option<String>,
    pub rating: i32,
    pub comment: Option<String>,
    pub signal: String,
}

// ---------------------------------------------------------------------------
// Improvement Signal (computed)
// ---------------------------------------------------------------------------

/// A computed improvement signal derived from metrics and feedback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImprovementSignal {
    /// Which pipeline this signal applies to.
    pub pipeline: String,
    /// Which step is problematic (if step-specific).
    pub step_id: Option<String>,
    /// Which agent is underperforming.
    pub agent: Option<String>,
    /// Type of issue: "slow" | "failing" | "low_rating" | "high_retry"
    pub signal_type: String,
    /// Human-readable description.
    pub description: String,
    /// Severity: 1 (info) to 5 (critical).
    pub severity: i32,
}

// ---------------------------------------------------------------------------
// MetricsService
// ---------------------------------------------------------------------------

pub struct MetricsService<'c> {
    conn: &'c Connection,
}

impl<'c> MetricsService<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Record metrics for a single step execution.
    #[allow(clippy::too_many_arguments)]
    pub fn record_step_metric(
        &self,
        run_id: &str,
        step_id: &str,
        agent: &str,
        duration_ms: i64,
        retries_used: i32,
        skipped: bool,
        output_length: i64,
        status: &str,
        error: Option<&str>,
    ) -> Result<StepMetric> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        self.conn.execute(
            "INSERT INTO step_metrics (id, run_id, step_id, agent, duration_ms, retries_used,
             skipped, output_length, status, error, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                id, run_id, step_id, agent, duration_ms, retries_used,
                skipped, output_length, status, error, now
            ],
        )?;

        Ok(StepMetric {
            id,
            run_id: run_id.to_string(),
            step_id: step_id.to_string(),
            agent: agent.to_string(),
            duration_ms,
            retries_used,
            skipped,
            output_length,
            status: status.to_string(),
            error: error.map(String::from),
            created_at: now,
        })
    }

    /// Record aggregate metrics for a complete run.
    #[allow(clippy::too_many_arguments)]
    pub fn record_run_metric(
        &self,
        project_id: Option<&str>,
        pipeline: &str,
        total_duration_ms: i64,
        steps_total: i32,
        steps_succeeded: i32,
        steps_failed: i32,
        steps_skipped: i32,
        total_retries: i32,
        status: &str,
    ) -> Result<RunMetric> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        self.conn.execute(
            "INSERT INTO run_metrics (id, project_id, pipeline, total_duration_ms,
             steps_total, steps_succeeded, steps_failed, steps_skipped,
             total_retries, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                id, project_id, pipeline, total_duration_ms,
                steps_total, steps_succeeded, steps_failed, steps_skipped,
                total_retries, status, now
            ],
        )?;

        Ok(RunMetric {
            id,
            project_id: project_id.map(String::from),
            pipeline: pipeline.to_string(),
            total_duration_ms,
            steps_total,
            steps_succeeded,
            steps_failed,
            steps_skipped,
            total_retries,
            status: status.to_string(),
            created_at: now,
        })
    }

    /// List step metrics for a given run.
    pub fn list_step_metrics(&self, run_id: &str) -> Result<Vec<StepMetric>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, run_id, step_id, agent, duration_ms, retries_used,
                    skipped, output_length, status, error, created_at
             FROM step_metrics WHERE run_id = ?1 ORDER BY created_at",
        )?;
        let rows = stmt.query_map(params![run_id], |row| {
            Ok(StepMetric {
                id: row.get(0)?,
                run_id: row.get(1)?,
                step_id: row.get(2)?,
                agent: row.get(3)?,
                duration_ms: row.get(4)?,
                retries_used: row.get(5)?,
                skipped: row.get(6)?,
                output_length: row.get(7)?,
                status: row.get(8)?,
                error: row.get(9)?,
                created_at: row.get(10)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// List run metrics for a project (or all if project_id is None).
    /// Resolve a `run_id` to its owning `project_id` via the `run_metrics`
    /// table, so handlers can tenant-validate the run before exposing data.
    pub fn get_run_project_id(&self, run_id: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT project_id FROM run_metrics WHERE id = ?1 LIMIT 1")?;
        let mut rows = stmt.query([run_id])?;
        if let Some(row) = rows.next()? {
            let pid: Option<String> = row.get(0)?;
            return Ok(pid);
        }
        Ok(None)
    }

    pub fn list_run_metrics(&self, project_id: Option<&str>) -> Result<Vec<RunMetric>> {
        let (sql, p): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = match project_id {
            Some(pid) => (
                "SELECT id, project_id, pipeline, total_duration_ms,
                        steps_total, steps_succeeded, steps_failed, steps_skipped,
                        total_retries, status, created_at
                 FROM run_metrics WHERE project_id = ?1 ORDER BY created_at DESC",
                vec![Box::new(pid.to_string())],
            ),
            None => (
                "SELECT id, project_id, pipeline, total_duration_ms,
                        steps_total, steps_succeeded, steps_failed, steps_skipped,
                        total_retries, status, created_at
                 FROM run_metrics ORDER BY created_at DESC",
                vec![],
            ),
        };

        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(p.iter()), |row| {
            Ok(RunMetric {
                id: row.get(0)?,
                project_id: row.get(1)?,
                pipeline: row.get(2)?,
                total_duration_ms: row.get(3)?,
                steps_total: row.get(4)?,
                steps_succeeded: row.get(5)?,
                steps_failed: row.get(6)?,
                steps_skipped: row.get(7)?,
                total_retries: row.get(8)?,
                status: row.get(9)?,
                created_at: row.get(10)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }
}

// ---------------------------------------------------------------------------
// FeedbackService
// ---------------------------------------------------------------------------

pub struct FeedbackService<'c> {
    conn: &'c Connection,
}

impl<'c> FeedbackService<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Record user feedback.
    pub fn add_feedback(&self, input: &NewFeedback) -> Result<Feedback> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        self.conn.execute(
            "INSERT INTO feedback (id, project_id, run_id, step_id, rating, comment, signal, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                input.project_id,
                input.run_id,
                input.step_id,
                input.rating,
                input.comment,
                input.signal,
                now
            ],
        )?;

        Ok(Feedback {
            id,
            project_id: input.project_id.clone(),
            run_id: input.run_id.clone(),
            step_id: input.step_id.clone(),
            rating: input.rating,
            comment: input.comment.clone(),
            signal: input.signal.clone(),
            created_at: now,
        })
    }

    /// List feedback for a project.
    pub fn list_feedback(&self, project_id: &str) -> Result<Vec<Feedback>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, run_id, step_id, rating, comment, signal, created_at
             FROM feedback WHERE project_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![project_id], |row| {
            Ok(Feedback {
                id: row.get(0)?,
                project_id: row.get(1)?,
                run_id: row.get(2)?,
                step_id: row.get(3)?,
                rating: row.get(4)?,
                comment: row.get(5)?,
                signal: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Average rating for a project (returns 0.0 if no feedback).
    pub fn average_rating(&self, project_id: &str) -> Result<f64> {
        let avg: f64 = self.conn.query_row(
            "SELECT COALESCE(AVG(CAST(rating AS REAL)), 0.0)
             FROM feedback WHERE project_id = ?1 AND rating > 0",
            params![project_id],
            |row| row.get(0),
        )?;
        Ok(avg)
    }
}

// ---------------------------------------------------------------------------
// AnalyticsService
// ---------------------------------------------------------------------------

pub struct AnalyticsService<'c> {
    conn: &'c Connection,
}

impl<'c> AnalyticsService<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Detect improvement signals from recent metrics and feedback.
    ///
    /// Checks for:
    /// - Steps with high retry rates
    /// - Steps that are consistently slow
    /// - Steps with frequent failures
    /// - Low user ratings per pipeline
    pub fn detect_signals(&self, project_id: Option<&str>) -> Result<Vec<ImprovementSignal>> {
        let mut signals = Vec::new();

        // 1. High retry agents (> 1 avg retries in last 20 runs)
        let retry_sql = if project_id.is_some() {
            "SELECT sm.agent, sm.step_id, AVG(sm.retries_used) as avg_retries, COUNT(*) as cnt
             FROM step_metrics sm
             JOIN run_metrics rm ON sm.run_id = rm.id
             WHERE rm.project_id = ?1 AND sm.retries_used > 0
             GROUP BY sm.agent, sm.step_id
             HAVING avg_retries > 1.0
             ORDER BY avg_retries DESC
             LIMIT 10"
        } else {
            "SELECT sm.agent, sm.step_id, AVG(sm.retries_used) as avg_retries, COUNT(*) as cnt
             FROM step_metrics sm
             WHERE sm.retries_used > 0
             GROUP BY sm.agent, sm.step_id
             HAVING avg_retries > 1.0
             ORDER BY avg_retries DESC
             LIMIT 10"
        };

        let mut stmt = self.conn.prepare(retry_sql)?;
        let params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = match project_id {
            Some(pid) => vec![Box::new(pid.to_string())],
            None => vec![],
        };
        let rows = stmt.query_map(rusqlite::params_from_iter(params_vec.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;
        for row in rows {
            let (agent, step_id, avg_retries, count) = row?;
            signals.push(ImprovementSignal {
                pipeline: String::new(),
                step_id: Some(step_id.clone()),
                agent: Some(agent.clone()),
                signal_type: "high_retry".into(),
                description: format!(
                    "Agent '{}' in step '{}' averages {:.1} retries across {} executions",
                    agent, step_id, avg_retries, count
                ),
                severity: if avg_retries > 2.0 { 4 } else { 2 },
            });
        }

        // 2. Slow steps (> 30s average)
        let slow_sql = if project_id.is_some() {
            "SELECT sm.step_id, sm.agent, AVG(sm.duration_ms) as avg_ms
             FROM step_metrics sm
             JOIN run_metrics rm ON sm.run_id = rm.id
             WHERE rm.project_id = ?1 AND sm.skipped = 0
             GROUP BY sm.step_id, sm.agent
             HAVING avg_ms > 30000
             ORDER BY avg_ms DESC
             LIMIT 10"
        } else {
            "SELECT sm.step_id, sm.agent, AVG(sm.duration_ms) as avg_ms
             FROM step_metrics sm
             WHERE sm.skipped = 0
             GROUP BY sm.step_id, sm.agent
             HAVING avg_ms > 30000
             ORDER BY avg_ms DESC
             LIMIT 10"
        };

        let mut stmt = self.conn.prepare(slow_sql)?;
        let params_vec2: Vec<Box<dyn rusqlite::types::ToSql>> = match project_id {
            Some(pid) => vec![Box::new(pid.to_string())],
            None => vec![],
        };
        let rows = stmt.query_map(rusqlite::params_from_iter(params_vec2.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
            ))
        })?;
        for row in rows {
            let (step_id, agent, avg_ms) = row?;
            signals.push(ImprovementSignal {
                pipeline: String::new(),
                step_id: Some(step_id.clone()),
                agent: Some(agent.clone()),
                signal_type: "slow".into(),
                description: format!(
                    "Step '{}' by agent '{}' averages {:.0}ms ({:.1}s)",
                    step_id, agent, avg_ms, avg_ms / 1000.0
                ),
                severity: if avg_ms > 60000.0 { 3 } else { 1 },
            });
        }

        // 3. Failing pipelines (> 30% failure rate)
        let fail_sql = if project_id.is_some() {
            "SELECT pipeline,
                    COUNT(*) as total,
                    SUM(CASE WHEN status = 'failure' THEN 1 ELSE 0 END) as failures
             FROM run_metrics
             WHERE project_id = ?1
             GROUP BY pipeline
             HAVING CAST(failures AS REAL) / total > 0.3"
        } else {
            "SELECT pipeline,
                    COUNT(*) as total,
                    SUM(CASE WHEN status = 'failure' THEN 1 ELSE 0 END) as failures
             FROM run_metrics
             GROUP BY pipeline
             HAVING CAST(failures AS REAL) / total > 0.3"
        };

        let mut stmt = self.conn.prepare(fail_sql)?;
        let params_vec3: Vec<Box<dyn rusqlite::types::ToSql>> = match project_id {
            Some(pid) => vec![Box::new(pid.to_string())],
            None => vec![],
        };
        let rows = stmt.query_map(rusqlite::params_from_iter(params_vec3.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        for row in rows {
            let (pipeline, total, failures) = row?;
            let rate = failures as f64 / total as f64 * 100.0;
            signals.push(ImprovementSignal {
                pipeline: pipeline.clone(),
                step_id: None,
                agent: None,
                signal_type: "failing".into(),
                description: format!(
                    "Pipeline '{}' has {:.0}% failure rate ({}/{} runs)",
                    pipeline, rate, failures, total
                ),
                severity: if rate > 50.0 { 5 } else { 3 },
            });
        }

        Ok(signals)
    }
}
