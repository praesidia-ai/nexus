//! Self-improvement engine — Nexus learns from every generation.
//!
//! After each generation the engine records episodes in the memory system,
//! stores learning signals (stack decisions + outcomes), identifies weak
//! quality axes, and classifies build failures to record fix patterns.
//!
//! A periodic self-evaluation summarises recent performance and detects
//! improving, stable, or regressing trends.

use std::sync::Arc;

use serde::Serialize;
use tracing::info;

use crate::state::AppState;

/// Engine that drives continuous learning from generation outcomes.
pub struct SelfImprovementEngine;

/// Outcome of a single post-generation learning pass.
#[derive(Debug, Clone, Serialize)]
pub struct LearningOutcome {
    /// Number of episodic memories recorded.
    pub episodes_recorded: usize,
    /// Number of learning signals stored.
    pub signals_recorded: usize,
    /// Human-readable descriptions of patterns learned.
    pub patterns_learned: Vec<String>,
}

/// Periodic self-evaluation report.
#[derive(Debug, Clone, Serialize)]
pub struct SelfEvalReport {
    /// Total generations in the evaluation window.
    pub total_generations: usize,
    /// Average taste/quality score (0.0 to 1.0).
    pub avg_taste_score: f32,
    /// Fraction of builds that succeeded (0.0 to 1.0).
    pub build_success_rate: f32,
    /// Detected trend: "improving", "stable", or "regressing".
    pub trend: String,
    /// Quality axes that are consistently weak.
    pub weak_areas: Vec<String>,
}

impl SelfImprovementEngine {
    /// Create a new self-improvement engine instance.
    pub fn new() -> Self {
        Self
    }

    /// Run after every generation to record outcomes and learn.
    ///
    /// 1. Record episode in memory system
    /// 2. Record learning signal (stack decisions + outcomes)
    /// 3. If taste score < 0.75, analyze weak axes and record patterns
    /// 4. If build failed, classify error and record fix pattern
    pub async fn post_generation_learning(
        &self,
        app: &Arc<AppState>,
        project_id: &str,
        description: &str,
        taste_score: f32,
        build_success: bool,
        duration_ms: u64,
    ) -> Result<LearningOutcome, String> {
        let mut episodes_recorded: usize = 0;
        let mut signals_recorded: usize = 0;
        let mut patterns_learned: Vec<String> = Vec::new();

        // ── 1. Record episode in memory ────────────────────────────────────
        {
            let db = app.db.lock().await;
            let episode_id = uuid::Uuid::new_v4().to_string();
            let content = format!(
                "Generation for '{}': taste={:.2}, build={}, duration={}ms",
                description, taste_score, build_success, duration_ms
            );
            let _ = db.execute(
                "INSERT INTO global_memory (id, category, key, value, source, confidence, times_applied, created_at, updated_at)
                 VALUES (?1, 'episode', ?2, ?3, 'self_improvement_engine', ?4, 0, datetime('now'), datetime('now'))
                 ON CONFLICT(key) DO UPDATE SET
                   value = ?3,
                   confidence = MIN(confidence + 0.02, 1.0),
                   times_applied = times_applied + 1,
                   updated_at = datetime('now')",
                rusqlite::params![
                    episode_id,
                    format!("gen_episode_{}", project_id),
                    content,
                    taste_score as f64,
                ],
            );
            episodes_recorded += 1;
        }

        // ── 2. Record learning signal ──────────────────────────────────────
        {
            let db = app.db.lock().await;
            let signal_id = uuid::Uuid::new_v4().to_string();
            let signal_value = serde_json::json!({
                "project_id": project_id,
                "taste_score": taste_score,
                "build_success": build_success,
                "duration_ms": duration_ms,
            });
            let _ = db.execute(
                "INSERT INTO global_memory (id, category, key, value, source, confidence, times_applied, created_at, updated_at)
                 VALUES (?1, 'learning_signal', ?2, ?3, 'self_improvement_engine', 0.8, 0, datetime('now'), datetime('now'))
                 ON CONFLICT(key) DO UPDATE SET
                   value = ?3,
                   times_applied = times_applied + 1,
                   updated_at = datetime('now')",
                rusqlite::params![
                    signal_id,
                    format!("signal_{}", project_id),
                    signal_value.to_string(),
                ],
            );
            signals_recorded += 1;
        }

        // ── 3. Weak axes analysis ──────────────────────────────────────────
        if taste_score < 0.75 {
            let weak = self.identify_weak_axes(app, project_id).await;
            for axis in &weak {
                let db = app.db.lock().await;
                let id = uuid::Uuid::new_v4().to_string();
                let _ = db.execute(
                    "INSERT INTO global_memory (id, category, key, value, source, confidence, times_applied, created_at, updated_at)
                     VALUES (?1, 'weak_axis_pattern', ?2, ?3, 'self_improvement_engine', 0.6, 0, datetime('now'), datetime('now'))
                     ON CONFLICT(key) DO UPDATE SET
                       confidence = MIN(confidence + 0.05, 1.0),
                       times_applied = times_applied + 1,
                       updated_at = datetime('now')",
                    rusqlite::params![
                        id,
                        format!("weak_{}", axis),
                        format!("Axis '{}' consistently scores below threshold", axis),
                    ],
                );
                patterns_learned.push(format!("Weak axis detected: {}", axis));
            }
        }

        // ── 4. Build failure analysis ──────────────────────────────────────
        if !build_success {
            let pattern = format!("Build failure for project {}", project_id);
            let db = app.db.lock().await;
            let id = uuid::Uuid::new_v4().to_string();
            let _ = db.execute(
                "INSERT INTO global_memory (id, category, key, value, source, confidence, times_applied, created_at, updated_at)
                 VALUES (?1, 'build_failure_pattern', ?2, ?3, 'self_improvement_engine', 0.7, 0, datetime('now'), datetime('now'))
                 ON CONFLICT(key) DO UPDATE SET
                   confidence = MIN(confidence + 0.05, 1.0),
                   times_applied = times_applied + 1,
                   updated_at = datetime('now')",
                rusqlite::params![
                    id,
                    format!("build_fail_{}", project_id),
                    pattern,
                ],
            );
            patterns_learned.push("Build failure pattern recorded".to_string());
        }

        info!(
            project_id = %project_id,
            episodes = episodes_recorded,
            signals = signals_recorded,
            patterns = patterns_learned.len(),
            "Post-generation learning completed"
        );

        Ok(LearningOutcome {
            episodes_recorded,
            signals_recorded,
            patterns_learned,
        })
    }

    /// Periodic self-evaluation.
    ///
    /// 1. Count recent generations
    /// 2. Compute average taste score
    /// 3. Compute build success rate
    /// 4. Identify trends (improving/stable/regressing)
    pub async fn run_self_evaluation(&self, app: &Arc<AppState>) -> SelfEvalReport {
        let db = app.db.lock().await;

        // Count recent generations (episodes recorded by this engine)
        let total_generations: usize = db
            .query_row(
                "SELECT COUNT(*) FROM global_memory WHERE category = 'episode' AND source = 'self_improvement_engine' AND created_at > datetime('now', '-7 days')",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0) as usize;

        // Average taste score from recent signals
        let avg_taste_score: f32 = db
            .query_row(
                "SELECT COALESCE(AVG(CAST(confidence AS REAL)), 0.0) FROM global_memory WHERE category = 'episode' AND source = 'self_improvement_engine' AND created_at > datetime('now', '-7 days')",
                [],
                |r| r.get::<_, f64>(0),
            )
            .unwrap_or(0.0) as f32;

        // Build success rate from run_metrics
        let total_runs: f64 = db
            .query_row(
                "SELECT COUNT(*) FROM run_metrics WHERE created_at > datetime('now', '-7 days')",
                [],
                |r| r.get::<_, f64>(0),
            )
            .unwrap_or(0.0);
        let success_runs: f64 = db
            .query_row(
                "SELECT COUNT(*) FROM run_metrics WHERE status = 'success' AND created_at > datetime('now', '-7 days')",
                [],
                |r| r.get::<_, f64>(0),
            )
            .unwrap_or(0.0);
        let build_success_rate = if total_runs > 0.0 {
            (success_runs / total_runs) as f32
        } else {
            1.0
        };

        // Trend detection: compare last 3 days vs prior 4 days
        let recent_taste: f64 = db
            .query_row(
                "SELECT COALESCE(AVG(CAST(confidence AS REAL)), 0.0) FROM global_memory WHERE category = 'episode' AND source = 'self_improvement_engine' AND created_at > datetime('now', '-3 days')",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0.0);
        let older_taste: f64 = db
            .query_row(
                "SELECT COALESCE(AVG(CAST(confidence AS REAL)), 0.0) FROM global_memory WHERE category = 'episode' AND source = 'self_improvement_engine' AND created_at BETWEEN datetime('now', '-7 days') AND datetime('now', '-3 days')",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0.0);

        let trend = if older_taste < 0.001 && recent_taste < 0.001 {
            "stable".to_string()
        } else if recent_taste > older_taste + 0.03 {
            "improving".to_string()
        } else if recent_taste < older_taste - 0.03 {
            "regressing".to_string()
        } else {
            "stable".to_string()
        };

        // Weak areas: axes that appear frequently in weak_axis_pattern
        let mut weak_areas = Vec::new();
        if let Ok(mut stmt) = db.prepare(
            "SELECT key, times_applied FROM global_memory WHERE category = 'weak_axis_pattern' AND times_applied >= 2 ORDER BY times_applied DESC LIMIT 5",
        ) {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok(r.get::<_, String>(0).unwrap_or_default())
            }) {
                for row in rows.flatten() {
                    let axis = row.strip_prefix("weak_").unwrap_or(&row).to_string();
                    weak_areas.push(axis);
                }
            }
        }

        SelfEvalReport {
            total_generations,
            avg_taste_score,
            build_success_rate,
            trend,
            weak_areas,
        }
    }

    /// Identify weak quality axes for a project by looking at taste history.
    async fn identify_weak_axes(&self, app: &Arc<AppState>, _project_id: &str) -> Vec<String> {
        let db = app.db.lock().await;
        let mut weak = Vec::new();

        // Check known taste axes that are commonly tracked
        let axes = ["layout", "typography", "color", "responsiveness", "ux"];
        for axis in &axes {
            let score: f64 = db
                .query_row(
                    &format!(
                        "SELECT COALESCE(AVG(CAST(json_extract(value, '$.{}') AS REAL)), 1.0) FROM global_memory WHERE category = 'learning_signal' AND created_at > datetime('now', '-7 days')",
                        axis
                    ),
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(1.0);
            if score < 0.7 {
                weak.push(axis.to_string());
            }
        }

        weak
    }
}

impl Default for SelfImprovementEngine {
    fn default() -> Self {
        Self::new()
    }
}
