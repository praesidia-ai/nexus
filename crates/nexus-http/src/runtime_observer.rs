//! Runtime Observer — learns from real-world usage of generated apps.
//!
//! Tracks API usage frequency, endpoint failures, latency spikes, and user
//! flow patterns. Feeds signals into self_improvement, taste_engine, and
//! anticipation_engine.

use std::sync::Arc;

use serde::Serialize;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeEvent {
    /// Event type.
    pub event_type: RuntimeEventType,
    /// ISO-8601 timestamp.
    pub timestamp: String,
    /// Severity (1=info, 2=warning, 3=error, 4=critical).
    pub severity: u32,
    /// Project this event belongs to.
    pub project_id: String,
    /// Additional context.
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEventType {
    /// An API endpoint was called.
    ApiCall,
    /// An API endpoint returned an error.
    ApiError,
    /// Latency spike detected.
    LatencySpike,
    /// App crash detected.
    AppCrash,
    /// Health check failure.
    HealthFailure,
    /// Build failure.
    BuildFailure,
    /// User flow completed.
    FlowComplete,
    /// User flow abandoned.
    FlowAbandoned,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeHealthReport {
    /// Overall health status.
    pub status: HealthStatus,
    /// Number of events in the analysis window.
    pub total_events: usize,
    /// Error rate (0.0–1.0).
    pub error_rate: f64,
    /// Average API latency (ms).
    pub avg_latency_ms: f64,
    /// Number of crashes detected.
    pub crash_count: u32,
    /// Top failing endpoints.
    pub top_errors: Vec<EndpointHealth>,
    /// Recommendations.
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct EndpointHealth {
    pub endpoint: String,
    pub error_count: u32,
    pub avg_latency_ms: f64,
}

// ---------------------------------------------------------------------------
// Record events
// ---------------------------------------------------------------------------

/// Record a runtime event to the database.
pub async fn record_event(app: &Arc<AppState>, event: &RuntimeEvent) {
    let db = app.db.lock().await;
    let id = uuid::Uuid::new_v4().to_string();
    let event_type = serde_json::to_string(&event.event_type).unwrap_or_default();
    let metadata = event.metadata.to_string();

    // Store in audit_log (reuse existing table — it has the right shape)
    let _ = db.execute(
        "INSERT INTO audit_log (id, project_id, actor, action, target, details, risk_level, created_at)
         VALUES (?1, ?2, 'runtime_observer', ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            id,
            event.project_id,
            event_type,
            format!("severity:{}", event.severity),
            metadata,
            match event.severity {
                1 => "low",
                2 => "medium",
                3 | 4 => "high",
                _ => "low",
            },
            event.timestamp,
        ],
    );
}

// ---------------------------------------------------------------------------
// Analyze runtime health
// ---------------------------------------------------------------------------

/// Analyze runtime health for a project from observed events.
pub async fn analyze_runtime_health(
    app: &Arc<AppState>,
    project_id: &str,
) -> RuntimeHealthReport {
    let db = app.db.lock().await;

    // Count recent events by type (last 24h)
    let total_events: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM audit_log
             WHERE project_id = ?1 AND actor = 'runtime_observer'
               AND created_at > datetime('now', '-24 hours')",
            rusqlite::params![project_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let error_events: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM audit_log
             WHERE project_id = ?1 AND actor = 'runtime_observer'
               AND risk_level IN ('medium', 'high')
               AND created_at > datetime('now', '-24 hours')",
            rusqlite::params![project_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Count crashes (from healing_events)
    let crash_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM healing_events
             WHERE project_id = ?1 AND created_at > datetime('now', '-24 hours')",
            rusqlite::params![project_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Get app instance health
    let health_status_str: String = db
        .query_row(
            "SELECT COALESCE(health_status, 'unknown') FROM app_instances
             WHERE project_id = ?1 AND status = 'running'
             ORDER BY started_at DESC LIMIT 1",
            rusqlite::params![project_id],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| "unknown".into());

    // Top errors — group audit_log errors by action (endpoint proxy)
    let top_errors: Vec<EndpointHealth> = db
        .prepare(
            "SELECT action, COUNT(*) as cnt
             FROM audit_log
             WHERE project_id = ?1 AND actor = 'runtime_observer'
               AND risk_level IN ('medium', 'high')
               AND created_at > datetime('now', '-24 hours')
             GROUP BY action
             ORDER BY cnt DESC
             LIMIT 5",
        )
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map(rusqlite::params![project_id], |r| {
                Ok(EndpointHealth {
                    endpoint: r.get::<_, String>(0)?,
                    error_count: r.get::<_, u32>(1)?,
                    avg_latency_ms: 0.0,
                })
            })
            .ok()
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
        })
        .unwrap_or_default();

    drop(db);

    let error_rate = if total_events > 0 {
        error_events as f64 / total_events as f64
    } else {
        0.0
    };

    let status = if health_status_str == "unknown" || total_events == 0 {
        HealthStatus::Unknown
    } else if error_rate > 0.3 || crash_count > 2 {
        HealthStatus::Unhealthy
    } else if error_rate > 0.1 || crash_count > 0 {
        HealthStatus::Degraded
    } else {
        HealthStatus::Healthy
    };

    // Generate recommendations
    let mut recommendations = Vec::new();
    if crash_count > 0 {
        recommendations.push(format!(
            "App crashed {} time(s) in last 24h — run diagnostic agent",
            crash_count
        ));
    }
    if error_rate > 0.1 {
        recommendations.push(format!(
            "Error rate is {:.0}% — investigate failing endpoints",
            error_rate * 100.0
        ));
    }
    if total_events == 0 {
        recommendations.push("No runtime data available — start the app and run tests".into());
    }
    if matches!(status, HealthStatus::Healthy) && recommendations.is_empty() {
        recommendations.push("System is healthy — no action needed".into());
    }

    RuntimeHealthReport {
        status,
        total_events: total_events as usize,
        error_rate,
        avg_latency_ms: 0.0, // Requires HTTP-level monitoring (proxy/middleware)
        crash_count: crash_count as u32,
        top_errors,
        recommendations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_app() -> Arc<AppState> {
        let dir = tempfile::tempdir().unwrap();
        AppState::init(dir.path()).await.unwrap()
    }

    #[tokio::test]
    async fn empty_project_returns_unknown() {
        let app = test_app().await;
        let report = analyze_runtime_health(&app, "nonexistent").await;
        assert!(matches!(report.status, HealthStatus::Unknown));
        assert_eq!(report.total_events, 0);
    }

    #[tokio::test]
    async fn records_event_without_error() {
        let app = test_app().await;
        let event = RuntimeEvent {
            event_type: RuntimeEventType::ApiCall,
            timestamp: chrono::Utc::now().to_rfc3339(),
            severity: 1,
            project_id: "test-project".into(),
            metadata: serde_json::json!({"endpoint": "/api/health", "status": 200}),
        };
        record_event(&app, &event).await;
        // Should not panic
    }

    #[test]
    fn health_status_serializes() {
        let json = serde_json::to_string(&HealthStatus::Healthy).unwrap();
        assert_eq!(json, "\"healthy\"");
    }
}
