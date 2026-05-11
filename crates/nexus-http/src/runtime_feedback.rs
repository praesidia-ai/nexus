//! Runtime Feedback System — collects real-world usage data from generated apps.
//!
//! Generated apps can report:
//! - API latency per endpoint
//! - Error rates
//! - User actions (clicks, flows, navigation)
//! - Crash reports
//! - Performance metrics
//!
//! This data feeds back into:
//! - project_brain (per-project intelligence)
//! - global_memory (cross-project patterns)
//! - causal_learning (cause→effect evidence)
//!
//! Lightweight: simple HTTP endpoint, SQLite storage, no heavy infra.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Feedback sent by a generated app at runtime.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RuntimeFeedbackEntry {
    pub project_id: String,
    pub feedback_type: FeedbackType,
    pub endpoint: Option<String>,
    pub value: f64,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackType {
    ApiLatency,
    ErrorRate,
    UserAction,
    Crash,
    Performance,
}

impl FeedbackType {
    fn as_str(&self) -> &'static str {
        match self {
            FeedbackType::ApiLatency => "api_latency",
            FeedbackType::ErrorRate => "error_rate",
            FeedbackType::UserAction => "user_action",
            FeedbackType::Crash => "crash",
            FeedbackType::Performance => "performance",
        }
    }
}

/// Aggregated runtime stats for a project.
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeStats {
    pub project_id: String,
    pub avg_api_latency_ms: f64,
    pub error_rate: f64,
    pub total_user_actions: u32,
    pub crash_count: u32,
    pub avg_performance_score: f64,
    pub total_feedback_entries: u32,
}

/// Insights derived from runtime feedback.
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeInsight {
    pub category: String,
    pub insight: String,
    pub severity: InsightSeverity,
    pub suggestion: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InsightSeverity {
    Info,
    Warning,
    Critical,
}

// ---------------------------------------------------------------------------
// Record feedback
// ---------------------------------------------------------------------------

/// Record a single runtime feedback entry.
pub async fn record_feedback(app: &Arc<AppState>, entry: &RuntimeFeedbackEntry) {
    let db = app.db.lock().await;
    let id = uuid::Uuid::new_v4().to_string();
    let metadata_str = entry.metadata.as_ref().map(|m| m.to_string());

    match db.execute(
        "INSERT INTO runtime_feedback (id, project_id, feedback_type, endpoint, value, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            id,
            entry.project_id,
            entry.feedback_type.as_str(),
            entry.endpoint,
            entry.value,
            metadata_str,
        ],
    ) {
        Ok(n) => {
            tracing::debug!(rows = n, "Runtime feedback recorded");
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to record runtime feedback");
        }
    }
}

/// Record a batch of feedback entries (more efficient for high-throughput).
pub async fn record_feedback_batch(app: &Arc<AppState>, entries: &[RuntimeFeedbackEntry]) {
    let db = app.db.lock().await;
    for entry in entries {
        let id = uuid::Uuid::new_v4().to_string();
        let metadata_str = entry.metadata.as_ref().map(|m| m.to_string());
        let _ = db.execute(
            "INSERT INTO runtime_feedback (id, project_id, feedback_type, endpoint, value, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                id,
                entry.project_id,
                entry.feedback_type.as_str(),
                entry.endpoint,
                entry.value,
                metadata_str,
            ],
        );
    }
}

// ---------------------------------------------------------------------------
// Query & aggregate
// ---------------------------------------------------------------------------

/// Get aggregated runtime stats for a project.
pub async fn get_project_stats(app: &Arc<AppState>, project_id: &str) -> RuntimeStats {
    let db = app.db.lock().await;

    let avg_latency: f64 = db
        .query_row(
            "SELECT COALESCE(AVG(value), 0) FROM runtime_feedback
             WHERE project_id = ?1 AND feedback_type = 'api_latency'",
            rusqlite::params![project_id],
            |r| r.get(0),
        )
        .unwrap_or(0.0);

    let error_rate: f64 = db
        .query_row(
            "SELECT COALESCE(AVG(value), 0) FROM runtime_feedback
             WHERE project_id = ?1 AND feedback_type = 'error_rate'",
            rusqlite::params![project_id],
            |r| r.get(0),
        )
        .unwrap_or(0.0);

    let user_actions: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM runtime_feedback
             WHERE project_id = ?1 AND feedback_type = 'user_action'",
            rusqlite::params![project_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let crashes: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM runtime_feedback
             WHERE project_id = ?1 AND feedback_type = 'crash'",
            rusqlite::params![project_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let avg_perf: f64 = db
        .query_row(
            "SELECT COALESCE(AVG(value), 0) FROM runtime_feedback
             WHERE project_id = ?1 AND feedback_type = 'performance'",
            rusqlite::params![project_id],
            |r| r.get(0),
        )
        .unwrap_or(0.0);

    let total: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM runtime_feedback WHERE project_id = ?1",
            rusqlite::params![project_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    RuntimeStats {
        project_id: project_id.to_string(),
        avg_api_latency_ms: avg_latency,
        error_rate,
        total_user_actions: user_actions,
        crash_count: crashes,
        avg_performance_score: avg_perf,
        total_feedback_entries: total,
    }
}

/// Derive insights from runtime feedback and feed them into learning systems.
pub async fn derive_insights(app: &Arc<AppState>, project_id: &str) -> Vec<RuntimeInsight> {
    let stats = get_project_stats(app, project_id).await;
    let mut insights = Vec::new();

    // High latency insight
    if stats.avg_api_latency_ms > 2000.0 {
        insights.push(RuntimeInsight {
            category: "performance".into(),
            insight: format!(
                "Average API latency is {:.0}ms — above 2000ms threshold",
                stats.avg_api_latency_ms
            ),
            severity: InsightSeverity::Warning,
            suggestion: "Consider adding caching, optimizing DB queries, or reducing payload sizes".into(),
        });

        // Feed into causal learning
        crate::causal_learning::record_observation(
            app,
            &format!("project={}", project_id),
            "high_api_latency",
            false,
        )
        .await;
    }

    // High error rate
    if stats.error_rate > 0.05 {
        insights.push(RuntimeInsight {
            category: "reliability".into(),
            insight: format!(
                "Error rate is {:.1}% — above 5% threshold",
                stats.error_rate * 100.0
            ),
            severity: InsightSeverity::Critical,
            suggestion: "Review error logs, add input validation, improve error handling".into(),
        });

        crate::causal_learning::record_observation(
            app,
            &format!("project={}", project_id),
            "high_error_rate",
            false,
        )
        .await;
    }

    // Crashes detected
    if stats.crash_count > 0 {
        insights.push(RuntimeInsight {
            category: "stability".into(),
            insight: format!("{} crashes detected in runtime", stats.crash_count),
            severity: InsightSeverity::Critical,
            suggestion: "Add error boundaries, fix unhandled promise rejections, add null checks".into(),
        });
    }

    // Good performance (reinforce patterns)
    if stats.total_feedback_entries > 10 && stats.error_rate < 0.01 && stats.avg_api_latency_ms < 500.0 {
        insights.push(RuntimeInsight {
            category: "quality".into(),
            insight: "App performing well: low latency, minimal errors".into(),
            severity: InsightSeverity::Info,
            suggestion: "Patterns from this project should be reinforced for future builds".into(),
        });

        // Reinforce successful patterns
        crate::memory_unification::reinforce(
            app,
            &format!("runtime_success_{}", project_id),
            "runtime_quality",
        )
        .await;
    }

    // Store insights in project brain
    if !insights.is_empty() {
        let project_dir = app.data_dir.join("projects").join(project_id).join("generated");
        let mut intel = crate::project_brain::ProjectIntelligence::load(&project_dir);
        for insight in &insights {
            intel.record_successful_pattern(&format!(
                "[runtime] {}: {}",
                insight.category, insight.insight
            ));
        }
        intel.save(&project_dir);
    }

    info!(
        project_id = %project_id,
        insights = insights.len(),
        "Runtime insights derived"
    );

    insights
}

/// Feed runtime feedback into global memory for cross-project learning.
pub async fn feed_to_global_learning(app: &Arc<AppState>, project_id: &str) {
    let stats = get_project_stats(app, project_id).await;

    if stats.total_feedback_entries < 5 {
        return; // Not enough data
    }

    let db = app.db.lock().await;

    // Store performance profile in global memory
    let perf_key = format!("runtime_perf_{}", project_id);
    let perf_value = serde_json::json!({
        "avg_latency_ms": stats.avg_api_latency_ms,
        "error_rate": stats.error_rate,
        "user_actions": stats.total_user_actions,
        "crashes": stats.crash_count,
        "perf_score": stats.avg_performance_score,
    })
    .to_string();

    let id = uuid::Uuid::new_v4().to_string();
    let _ = db.execute(
        "INSERT INTO global_memory (id, category, key, value, source, confidence, times_applied, created_at, updated_at)
         VALUES (?1, 'runtime_feedback', ?2, ?3, 'runtime_feedback_system', 0.7, 1, datetime('now'), datetime('now'))
         ON CONFLICT(key) DO UPDATE SET
           value = ?3,
           confidence = MIN(confidence + 0.05, 1.0),
           times_applied = times_applied + 1,
           updated_at = datetime('now')",
        rusqlite::params![id, perf_key, perf_value],
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Keep tempdir alive by returning it with the app state
    async fn test_app() -> (Arc<AppState>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let app = AppState::init(dir.path()).await.unwrap();
        (app, dir)
    }

    #[tokio::test]
    async fn records_and_retrieves_feedback() {
        let (app, _dir) = test_app().await;
        let entry = RuntimeFeedbackEntry {
            project_id: "test-proj".into(),
            feedback_type: FeedbackType::ApiLatency,
            endpoint: Some("/api/users".into()),
            value: 150.0,
            metadata: None,
        };
        record_feedback(&app, &entry).await;

        let stats = get_project_stats(&app, "test-proj").await;
        assert_eq!(stats.total_feedback_entries, 1);
        assert!((stats.avg_api_latency_ms - 150.0).abs() < 0.1);
    }

    #[tokio::test]
    async fn batch_recording_works() {
        let (app, _dir) = test_app().await;
        let entries: Vec<RuntimeFeedbackEntry> = (0..5)
            .map(|i| RuntimeFeedbackEntry {
                project_id: "batch-proj".into(),
                feedback_type: FeedbackType::ApiLatency,
                endpoint: Some(format!("/api/endpoint_{}", i)),
                value: 100.0 + i as f64 * 50.0,
                metadata: None,
            })
            .collect();
        record_feedback_batch(&app, &entries).await;

        let stats = get_project_stats(&app, "batch-proj").await;
        assert_eq!(stats.total_feedback_entries, 5);
    }

    #[tokio::test]
    async fn derives_insights_for_high_latency() {
        let (app, _dir) = test_app().await;
        // Record high-latency feedback
        for _ in 0..3 {
            record_feedback(
                &app,
                &RuntimeFeedbackEntry {
                    project_id: "slow-proj".into(),
                    feedback_type: FeedbackType::ApiLatency,
                    endpoint: Some("/api/data".into()),
                    value: 5000.0,
                    metadata: None,
                },
            )
            .await;
        }

        let insights = derive_insights(&app, "slow-proj").await;
        assert!(
            insights.iter().any(|i| i.category == "performance"),
            "Should detect high latency"
        );
    }

    #[tokio::test]
    async fn empty_stats_for_unknown_project() {
        let (app, _dir) = test_app().await;
        let stats = get_project_stats(&app, "nonexistent").await;
        assert_eq!(stats.total_feedback_entries, 0);
        assert!((stats.avg_api_latency_ms).abs() < 0.001);
    }
}
