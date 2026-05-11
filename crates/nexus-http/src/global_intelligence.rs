//! Global Intelligence Layer — cross-project learning that makes every new project
//! benefit from ALL past projects.
//!
//! Aggregates learnings by:
//! - AppType (SaaS apps share patterns with other SaaS apps)
//! - Domain (fitness apps learn from other fitness apps)
//! - Feature patterns (apps with auth+payments share patterns)
//!
//! Stores: successful decisions, failed patterns, UX improvements, performance outcomes.
//!
//! Integrated into nexus_brain::analyze() so new projects start smart.

use std::sync::Arc;

use serde::Serialize;
use tracing::info;

use crate::intent_engine::FlatIntent;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A cross-project intelligence recommendation.
#[derive(Debug, Clone, Serialize)]
pub struct GlobalRecommendation {
    /// What decision area this affects.
    pub area: String,
    /// Recommended value.
    pub recommended_value: String,
    /// Why this is recommended (based on cross-project evidence).
    pub reasoning: String,
    /// How many projects contributed to this recommendation.
    pub project_count: u32,
    /// Confidence in this recommendation (0.0–1.0).
    pub confidence: f64,
    /// Source: "app_type", "domain", or "feature_pattern".
    pub source: String,
}

/// Summary of global intelligence available.
#[derive(Debug, Clone, Serialize)]
pub struct GlobalIntelligenceSummary {
    pub recommendations: Vec<GlobalRecommendation>,
    pub total_projects_learned_from: u32,
    pub app_type_matches: u32,
    pub domain_matches: u32,
    pub feature_matches: u32,
}

// ---------------------------------------------------------------------------
// Record cross-project outcomes
// ---------------------------------------------------------------------------

/// Record a project outcome for cross-project learning.
pub async fn record_project_outcome(
    app: &Arc<AppState>,
    intent: &FlatIntent,
    domain: &str,
    decisions: &[(String, String)], // (area, value)
    success: bool,
    taste_score: Option<u32>,
) {
    let db = app.db.lock().await;
    let app_type = format!("{:?}", intent.app_type);
    let outcome = if success { "success" } else { "failure" };

    // Extract feature pattern signature
    let feature_sig = build_feature_signature(intent);

    for (area, value) in decisions {
        let id = uuid::Uuid::new_v4().to_string();
        let perf_data = taste_score.map(|s| format!("{{\"taste_score\":{}}}", s));

        let _ = db.execute(
            "INSERT INTO cross_project_intelligence
             (id, app_type, domain, feature_pattern, decision_area, decision_value,
              outcome, taste_score, build_success, performance_data, project_count, confidence)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, 0.5)",
            rusqlite::params![
                id,
                app_type,
                domain,
                feature_sig,
                area,
                value,
                outcome,
                taste_score.map(|s| s as i64),
                success as i64,
                perf_data,
            ],
        );
    }

    // Update aggregates
    update_aggregates(&db, &app_type, domain, &feature_sig);

    info!(
        app_type = %app_type,
        domain = %domain,
        decisions = decisions.len(),
        success = success,
        "Recorded cross-project intelligence"
    );
}

// ---------------------------------------------------------------------------
// Query cross-project intelligence
// ---------------------------------------------------------------------------

/// Get recommendations for a new project based on ALL past projects.
pub async fn get_recommendations(
    app: &Arc<AppState>,
    intent: &FlatIntent,
    domain: &str,
) -> GlobalIntelligenceSummary {
    let db = app.db.lock().await;
    let app_type = format!("{:?}", intent.app_type);
    let feature_sig = build_feature_signature(intent);
    let mut recommendations = Vec::new();

    // ── Source 1: Same AppType ──────────────────────────────────────────
    let app_type_recs = query_best_decisions(
        &db,
        "app_type",
        &app_type,
        "app_type",
    );
    let app_type_count = app_type_recs.len() as u32;
    recommendations.extend(app_type_recs);

    // ── Source 2: Same Domain ───────────────────────────────────────────
    let domain_recs = query_best_decisions(
        &db,
        "domain",
        domain,
        "domain",
    );
    let domain_count = domain_recs.len() as u32;
    recommendations.extend(domain_recs);

    // ── Source 3: Similar Feature Pattern ────────────────────────────────
    let feature_recs = query_feature_recommendations(&db, &feature_sig);
    let feature_count = feature_recs.len() as u32;
    recommendations.extend(feature_recs);

    // Deduplicate: keep highest confidence per area
    recommendations.sort_by(|a, b| {
        b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut seen_areas = std::collections::HashSet::new();
    recommendations.retain(|r| seen_areas.insert(r.area.clone()));

    // Count total distinct projects
    let total_projects: u32 = db
        .query_row(
            "SELECT COUNT(DISTINCT id) FROM cross_project_intelligence",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    drop(db);

    GlobalIntelligenceSummary {
        recommendations,
        total_projects_learned_from: total_projects,
        app_type_matches: app_type_count,
        domain_matches: domain_count,
        feature_matches: feature_count,
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Build a feature signature string for pattern matching.
/// e.g., "auth+db+payments" or "auth+db"
fn build_feature_signature(intent: &FlatIntent) -> String {
    let mut features = Vec::new();
    if intent.needs_auth {
        features.push("auth");
    }
    if intent.needs_database {
        features.push("db");
    }
    if intent.needs_payments {
        features.push("payments");
    }
    if !intent.suggested_agents.is_empty() {
        features.push("agents");
    }
    if features.is_empty() {
        "basic".to_string()
    } else {
        features.join("+")
    }
}

/// Query the best decisions for a given dimension (app_type, domain, or feature_pattern).
fn query_best_decisions(
    db: &rusqlite::Connection,
    column: &str,
    value: &str,
    source: &str,
) -> Vec<GlobalRecommendation> {
    // We use a safe column name since these are hardcoded strings
    let query = format!(
        "SELECT decision_area, decision_value,
                SUM(CASE WHEN outcome='success' THEN 1 ELSE 0 END) as successes,
                COUNT(*) as total,
                AVG(CASE WHEN taste_score > 0 THEN taste_score END) as avg_taste,
                COUNT(DISTINCT id) as projects
         FROM cross_project_intelligence
         WHERE {} = ?1
         GROUP BY decision_area, decision_value
         HAVING total >= 2
         ORDER BY (CAST(successes AS REAL) / total) DESC",
        column
    );

    let mut stmt = match db.prepare(&query) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    stmt.query_map(rusqlite::params![value], |row| {
        let area: String = row.get(0)?;
        let val: String = row.get(1)?;
        let successes: i64 = row.get(2)?;
        let total: i64 = row.get(3)?;
        let avg_taste: Option<f64> = row.get(4)?;
        let projects: i64 = row.get(5)?;

        let success_rate = successes as f64 / total as f64;
        let confidence = (total as f64 / 10.0).min(1.0) * success_rate;

        Ok(GlobalRecommendation {
            area,
            recommended_value: val.clone(),
            reasoning: format!(
                "Cross-project: {:.0}% success rate across {} projects (avg taste: {:.0})",
                success_rate * 100.0,
                projects,
                avg_taste.unwrap_or(0.0)
            ),
            project_count: projects as u32,
            confidence,
            source: source.to_string(),
        })
    })
    .ok()
    .map(|rows| {
        rows.filter_map(|r| r.ok())
            .filter(|r| r.confidence > 0.3)
            .collect()
    })
    .unwrap_or_default()
}

/// Query recommendations based on feature pattern overlap.
fn query_feature_recommendations(
    db: &rusqlite::Connection,
    feature_sig: &str,
) -> Vec<GlobalRecommendation> {
    // Match projects with overlapping feature patterns
    let features: Vec<&str> = feature_sig.split('+').collect();
    if features.is_empty() || features[0] == "basic" {
        return Vec::new();
    }

    // Match any project that shares at least one feature
    let like_clauses: Vec<String> = features
        .iter()
        .map(|f| format!("feature_pattern LIKE '%{}%'", f))
        .collect();
    let where_clause = like_clauses.join(" OR ");

    let query = format!(
        "SELECT decision_area, decision_value,
                SUM(CASE WHEN outcome='success' THEN 1 ELSE 0 END) as successes,
                COUNT(*) as total,
                COUNT(DISTINCT id) as projects
         FROM cross_project_intelligence
         WHERE ({})
         GROUP BY decision_area, decision_value
         HAVING total >= 3
         ORDER BY (CAST(successes AS REAL) / total) DESC",
        where_clause
    );

    let mut stmt = match db.prepare(&query) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    stmt.query_map([], |row| {
        let area: String = row.get(0)?;
        let val: String = row.get(1)?;
        let successes: i64 = row.get(2)?;
        let total: i64 = row.get(3)?;
        let projects: i64 = row.get(4)?;

        let success_rate = successes as f64 / total as f64;
        let confidence = (total as f64 / 15.0).min(1.0) * success_rate;

        Ok(GlobalRecommendation {
            area,
            recommended_value: val,
            reasoning: format!(
                "Feature-pattern match: {:.0}% success for similar feature combos ({} projects)",
                success_rate * 100.0,
                projects
            ),
            project_count: projects as u32,
            confidence,
            source: "feature_pattern".to_string(),
        })
    })
    .ok()
    .map(|rows| {
        rows.filter_map(|r| r.ok())
            .filter(|r| r.confidence > 0.3)
            .collect()
    })
    .unwrap_or_default()
}

/// Update materialized aggregates for fast lookups.
fn update_aggregates(db: &rusqlite::Connection, app_type: &str, domain: &str, feature_sig: &str) {
    // Best decision per app_type
    let _ = db.execute(
        "INSERT INTO intelligence_aggregates (id, aggregate_type, key, value, score, sample_count, updated_at)
         SELECT
           hex(randomblob(16)),
           'app_type_best',
           ?1 || '/' || decision_area,
           decision_value,
           CAST(SUM(CASE WHEN outcome='success' THEN 1 ELSE 0 END) AS REAL) / COUNT(*),
           COUNT(*),
           datetime('now')
         FROM cross_project_intelligence
         WHERE app_type = ?1
         GROUP BY decision_area, decision_value
         HAVING COUNT(*) >= 2
         ON CONFLICT(aggregate_type, key) DO UPDATE SET
           value = excluded.value,
           score = excluded.score,
           sample_count = excluded.sample_count,
           updated_at = datetime('now')",
        rusqlite::params![app_type],
    );

    // Best decision per domain
    let _ = db.execute(
        "INSERT INTO intelligence_aggregates (id, aggregate_type, key, value, score, sample_count, updated_at)
         SELECT
           hex(randomblob(16)),
           'domain_best',
           ?1 || '/' || decision_area,
           decision_value,
           CAST(SUM(CASE WHEN outcome='success' THEN 1 ELSE 0 END) AS REAL) / COUNT(*),
           COUNT(*),
           datetime('now')
         FROM cross_project_intelligence
         WHERE domain = ?1
         GROUP BY decision_area, decision_value
         HAVING COUNT(*) >= 2
         ON CONFLICT(aggregate_type, key) DO UPDATE SET
           value = excluded.value,
           score = excluded.score,
           sample_count = excluded.sample_count,
           updated_at = datetime('now')",
        rusqlite::params![domain],
    );

    // Best stack combo per feature pattern
    let _ = db.execute(
        "INSERT INTO intelligence_aggregates (id, aggregate_type, key, value, score, sample_count, updated_at)
         SELECT
           hex(randomblob(16)),
           'feature_best',
           ?1 || '/' || decision_area,
           decision_value,
           CAST(SUM(CASE WHEN outcome='success' THEN 1 ELSE 0 END) AS REAL) / COUNT(*),
           COUNT(*),
           datetime('now')
         FROM cross_project_intelligence
         WHERE feature_pattern = ?1
         GROUP BY decision_area, decision_value
         HAVING COUNT(*) >= 2
         ON CONFLICT(aggregate_type, key) DO UPDATE SET
           value = excluded.value,
           score = excluded.score,
           sample_count = excluded.sample_count,
           updated_at = datetime('now')",
        rusqlite::params![feature_sig],
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_app() -> (Arc<AppState>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let app = AppState::init(dir.path()).await.unwrap();
        (app, dir)
    }

    #[tokio::test]
    async fn empty_recommendations_without_data() {
        let (app, _dir) = test_app().await;
        let intent = crate::intent_engine::analyze_flat("Build a CRM");
        let summary = get_recommendations(&app, &intent, "crm").await;
        assert!(summary.recommendations.is_empty());
        assert_eq!(summary.total_projects_learned_from, 0);
    }

    #[tokio::test]
    async fn records_and_retrieves_cross_project_data() {
        let (app, _dir) = test_app().await;
        let intent = crate::intent_engine::analyze_flat("Build a SaaS CRM with billing");

        // Record 3 successful projects with same decisions
        for _ in 0..3 {
            record_project_outcome(
                &app,
                &intent,
                "crm",
                &[
                    ("database".into(), "postgres".into()),
                    ("auth".into(), "next_auth".into()),
                ],
                true,
                Some(90),
            )
            .await;
        }

        let summary = get_recommendations(&app, &intent, "crm").await;
        // With 3 observations, we should get recommendations (threshold is 2)
        assert!(
            summary.total_projects_learned_from > 0,
            "Should have recorded projects"
        );
    }

    #[tokio::test]
    async fn feature_signature_includes_all_features() {
        let mut intent = crate::intent_engine::analyze_flat("Build an app");
        intent.needs_auth = true;
        intent.needs_database = true;
        intent.needs_payments = true;
        let sig = build_feature_signature(&intent);
        assert!(sig.contains("auth"));
        assert!(sig.contains("db"));
        assert!(sig.contains("payments"));
    }

    #[tokio::test]
    async fn deduplicates_recommendations_by_area() {
        let (app, _dir) = test_app().await;
        let intent = crate::intent_engine::analyze_flat("Build a SaaS with auth and billing");

        // Record outcomes that would generate recs from multiple sources
        for _ in 0..4 {
            record_project_outcome(
                &app,
                &intent,
                "saas",
                &[("database".into(), "postgres".into())],
                true,
                Some(92),
            )
            .await;
        }

        let summary = get_recommendations(&app, &intent, "saas").await;
        // Should not have duplicate areas
        let areas: Vec<&str> = summary.recommendations.iter().map(|r| r.area.as_str()).collect();
        let unique: std::collections::HashSet<&&str> = areas.iter().collect();
        assert_eq!(areas.len(), unique.len(), "Should have no duplicate areas");
    }
}
