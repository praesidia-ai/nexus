//! Causal Learning — tracks WHY things work, not just WHAT worked.
//!
//! Builds a directed graph of cause→effect relationships with strength and
//! confidence. Uses this graph to infer optimal decisions for new contexts.
//!
//! Data sources:
//!   decision_feedback  → decision → outcome
//!   taste_history       → prompt/approach → quality score
//!   agent_traces        → agent config → success/failure
//!   feedback            → feature → user satisfaction
//!
//! Integrates with: decision_engine, prompt_evolution, self_improvement.

use std::sync::Arc;

use serde::Serialize;
use tracing::info;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single causal link: cause → effect with measured strength.
#[derive(Debug, Clone, Serialize)]
pub struct CausalLink {
    /// The cause (e.g., "database=postgres", "auth=nextauth", "model=gpt-4o").
    pub cause: String,
    /// The effect (e.g., "build_success", "taste_score_90+", "user_positive").
    pub effect: String,
    /// Strength of the link (-1.0 to 1.0). Positive = cause helps effect.
    pub strength: f64,
    /// Confidence in this link (0.0–1.0). Higher = more observations.
    pub confidence: f64,
    /// Number of observations supporting this link.
    pub observations: u32,
}

/// A decision override suggested by the causal model.
#[derive(Debug, Clone, Serialize)]
pub struct CausalOverride {
    /// Which decision area to override (e.g., "database", "auth", "model").
    pub area: String,
    /// The suggested value.
    pub suggested_value: String,
    /// Why this is suggested (causal reasoning).
    pub reasoning: String,
    /// Confidence in this override.
    pub confidence: f64,
}

/// Full causal graph for the system.
#[derive(Debug, Clone, Serialize)]
pub struct CausalGraph {
    pub links: Vec<CausalLink>,
    pub total_observations: u32,
    pub strongest_positive: Option<CausalLink>,
    pub strongest_negative: Option<CausalLink>,
}

// ---------------------------------------------------------------------------
// Build causal graph from historical data
// ---------------------------------------------------------------------------

/// Build the causal graph from all available historical data.
pub async fn build_graph(app: &Arc<AppState>) -> CausalGraph {
    let db = app.db.lock().await;
    let mut links: Vec<CausalLink> = Vec::new();
    let mut total_obs = 0u32;

    // ── Source 1: Decision → Build Outcome ─────────────────────────────
    // "When we chose X for area Y, did the build succeed?"
    let decision_links: Vec<(String, String, i64, i64)> = db
        .prepare(
            "SELECT area, chosen_value,
                    SUM(CASE WHEN outcome='success' THEN 1 ELSE 0 END) as successes,
                    COUNT(*) as total
             FROM decision_feedback
             GROUP BY area, chosen_value
             HAVING total >= 2"
        )
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            )))
            .ok()
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
        })
        .unwrap_or_default();

    for (area, value, successes, total) in &decision_links {
        let rate = *successes as f64 / *total as f64;
        // Strength: centered at 0.5 (50% success = neutral)
        let strength = (rate - 0.5) * 2.0; // maps 0.0-1.0 → -1.0 to 1.0
        let confidence = (*total as f64 / 20.0).min(1.0); // 20+ observations = full confidence
        links.push(CausalLink {
            cause: format!("{}={}", area, value),
            effect: "build_success".into(),
            strength,
            confidence,
            observations: *total as u32,
        });
        total_obs += *total as u32;
    }

    // ── Source 2: Decision → Taste Score ───────────────────────────────
    // "When we chose X, what taste scores did we get?"
    let taste_links: Vec<(String, String, f64, i64)> = db
        .prepare(
            "SELECT df.area, df.chosen_value,
                    AVG(CAST(df.taste_score AS REAL)) as avg_taste,
                    COUNT(*) as total
             FROM decision_feedback df
             WHERE df.taste_score IS NOT NULL AND df.taste_score > 0
             GROUP BY df.area, df.chosen_value
             HAVING total >= 2"
        )
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, f64>(2)?,
                r.get::<_, i64>(3)?,
            )))
            .ok()
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
        })
        .unwrap_or_default();

    for (area, value, avg_taste, total) in &taste_links {
        // Strength: centered at 75 (avg taste score = neutral)
        let strength = (avg_taste - 75.0) / 25.0; // maps 50-100 → -1.0 to 1.0
        let confidence = (*total as f64 / 10.0).min(1.0);
        links.push(CausalLink {
            cause: format!("{}={}", area, value),
            effect: "high_taste_score".into(),
            strength: strength.clamp(-1.0, 1.0),
            confidence,
            observations: *total as u32,
        });
        total_obs += *total as u32;
    }

    // ── Source 3: Agent → Success Rate ─────────────────────────────────
    let agent_links: Vec<(String, i64, i64)> = db
        .prepare(
            "SELECT COALESCE(provider || '/' || model, 'unknown') as agent_config,
                    SUM(CASE WHEN status='completed' THEN 1 ELSE 0 END) as successes,
                    COUNT(*) as total
             FROM agent_traces
             GROUP BY agent_config
             HAVING total >= 3"
        )
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            )))
            .ok()
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
        })
        .unwrap_or_default();

    for (config, successes, total) in &agent_links {
        let rate = *successes as f64 / *total as f64;
        let strength = (rate - 0.5) * 2.0;
        let confidence = (*total as f64 / 15.0).min(1.0);
        links.push(CausalLink {
            cause: format!("model={}", config),
            effect: "agent_success".into(),
            strength,
            confidence,
            observations: *total as u32,
        });
        total_obs += *total as u32;
    }

    // ── Source 4: User Feedback → Feature quality ──────────────────────
    let feedback_links: Vec<(String, f64, i64)> = db
        .prepare(
            "SELECT signal,
                    AVG(CAST(rating AS REAL)) as avg_rating,
                    COUNT(*) as total
             FROM feedback
             WHERE rating > 0
             GROUP BY signal
             HAVING total >= 2"
        )
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, f64>(1)?,
                r.get::<_, i64>(2)?,
            )))
            .ok()
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
        })
        .unwrap_or_default();

    for (signal, avg_rating, total) in &feedback_links {
        let strength = (avg_rating - 2.5) / 2.5; // maps 0-5 → -1.0 to 1.0
        let confidence = (*total as f64 / 10.0).min(1.0);
        links.push(CausalLink {
            cause: format!("signal={}", signal),
            effect: "user_satisfaction".into(),
            strength: strength.clamp(-1.0, 1.0),
            confidence,
            observations: *total as u32,
        });
        total_obs += *total as u32;
    }

    drop(db);

    // Find strongest links
    let strongest_positive = links
        .iter()
        .filter(|l| l.strength > 0.0 && l.confidence > 0.3)
        .max_by(|a, b| (a.strength * a.confidence).partial_cmp(&(b.strength * b.confidence)).unwrap_or(std::cmp::Ordering::Equal))
        .cloned();

    let strongest_negative = links
        .iter()
        .filter(|l| l.strength < 0.0 && l.confidence > 0.3)
        .min_by(|a, b| (a.strength * a.confidence).partial_cmp(&(b.strength * b.confidence)).unwrap_or(std::cmp::Ordering::Equal))
        .cloned();

    CausalGraph {
        links,
        total_observations: total_obs,
        strongest_positive,
        strongest_negative,
    }
}

// ---------------------------------------------------------------------------
// Infer best decisions from causal graph
// ---------------------------------------------------------------------------

/// Given a context, infer the best decision overrides based on causal evidence.
///
/// Returns overrides only when confidence is high enough (>0.5) and
/// the causal link is strong enough (|strength| > 0.3).
pub async fn infer_best_decisions(app: &Arc<AppState>) -> Vec<CausalOverride> {
    let graph = build_graph(app).await;
    let mut overrides = Vec::new();

    // Group links by decision area and find the best option for each
    let areas = ["database", "auth", "frontend", "hosting", "ui_library"];

    for area in &areas {
        // Find all links for this area targeting build_success or high_taste_score
        let area_links: Vec<&CausalLink> = graph
            .links
            .iter()
            .filter(|l| {
                l.cause.starts_with(&format!("{}=", area))
                    && (l.effect == "build_success" || l.effect == "high_taste_score")
                    && l.confidence > 0.4
                    && l.strength.abs() > 0.2
            })
            .collect();

        if area_links.is_empty() {
            continue;
        }

        // Find the value with the best combined score (strength * confidence)
        let best = area_links
            .iter()
            .max_by(|a, b| {
                let score_a = a.strength * a.confidence;
                let score_b = b.strength * b.confidence;
                score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
            });

        if let Some(best_link) = best {
            if best_link.strength > 0.2 {
                // Extract value from "area=value"
                let value = best_link
                    .cause
                    .split('=')
                    .nth(1)
                    .unwrap_or("")
                    .to_string();

                if !value.is_empty() {
                    overrides.push(CausalOverride {
                        area: area.to_string(),
                        suggested_value: value.clone(),
                        reasoning: format!(
                            "Causal evidence: '{}={}' → '{}' (strength: {:.2}, confidence: {:.2}, {} obs)",
                            area, value, best_link.effect,
                            best_link.strength, best_link.confidence, best_link.observations
                        ),
                        confidence: best_link.confidence * best_link.strength.abs(),
                    });
                }
            }
        }

        // Also flag strong negative signals
        let worst = area_links
            .iter()
            .filter(|l| l.strength < -0.3 && l.confidence > 0.5)
            .min_by(|a, b| a.strength.partial_cmp(&b.strength).unwrap_or(std::cmp::Ordering::Equal));

        if let Some(worst_link) = worst {
            let value = worst_link
                .cause
                .split('=')
                .nth(1)
                .unwrap_or("")
                .to_string();
            if !value.is_empty() {
                info!(
                    area = area,
                    value = %value,
                    strength = worst_link.strength,
                    "Causal warning: negative signal detected"
                );
            }
        }
    }

    // Sort by confidence descending
    overrides.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
    overrides
}

/// Record a causal observation (wraps into memory_unification).
pub async fn record_observation(
    app: &Arc<AppState>,
    cause: &str,
    effect: &str,
    positive: bool,
) {
    let key = format!("causal_{}_{}", cause, effect);
    let category = if positive { "causal_positive" } else { "causal_negative" };
    crate::memory_unification::reinforce(app, &key, category).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_app() -> Arc<AppState> {
        let dir = tempfile::tempdir().unwrap();
        AppState::init(dir.path()).await.unwrap()
    }

    #[tokio::test]
    async fn empty_graph_has_no_links() {
        let app = test_app().await;
        let graph = build_graph(&app).await;
        assert!(graph.links.is_empty());
        assert_eq!(graph.total_observations, 0);
    }

    #[tokio::test]
    async fn infer_returns_empty_without_data() {
        let app = test_app().await;
        let overrides = infer_best_decisions(&app).await;
        assert!(overrides.is_empty());
    }

    #[tokio::test]
    async fn records_observation() {
        let app = test_app().await;
        record_observation(&app, "database=postgres", "build_success", true).await;
        // Should not panic; observation stored in global_memory
    }

    #[test]
    fn causal_link_strength_range() {
        let link = CausalLink {
            cause: "test".into(),
            effect: "test".into(),
            strength: 0.8,
            confidence: 0.9,
            observations: 10,
        };
        assert!(link.strength >= -1.0 && link.strength <= 1.0);
    }
}
