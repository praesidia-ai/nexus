//! User Learning System — stores and evolves user preferences across sessions.
//!
//! Observes user behavior and infers preferences across three categories:
//! - `ui_style` — visual style preferences (luxurious, playful, corporate, minimal, bold)
//! - `tech_stack` — framework and tool preferences (nextjs, postgres, shadcn, etc.)
//! - `product_pattern` — product archetypes (saas, marketplace, ecommerce, etc.)
//!
//! Preferences are persisted in `user_preferences` table and surfaced as context
//! injected into intent analysis and code generation prompts.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreference {
    pub category: String,
    pub key: String,
    pub value: String,
    pub confidence: f64,
    pub times_observed: i64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreferenceObservation {
    pub category: String,
    pub key: String,
    pub value: String,
    /// How this was observed: "build_success", "user_override", "explicit_choice", "pattern_match"
    pub source: String,
    /// How much to boost confidence (0.0–1.0).
    pub signal_strength: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub ui_styles: Vec<UserPreference>,
    pub tech_stack: Vec<UserPreference>,
    pub product_patterns: Vec<UserPreference>,
    pub total_builds: i64,
    pub dominant_style: Option<String>,
    pub dominant_stack: Option<String>,
    pub dominant_pattern: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptedContext {
    /// Prompt context string to inject into LLM prompts.
    pub prompt_context: String,
    /// Specific tech stack preferences.
    pub preferred_stack: Vec<(String, String)>,
    /// UI style preference.
    pub preferred_ui_style: Option<String>,
    /// Product pattern preference.
    pub preferred_pattern: Option<String>,
}

// ---------------------------------------------------------------------------
// Core operations
// ---------------------------------------------------------------------------

/// Record a preference observation and update stored preferences.
pub async fn observe(app: &Arc<AppState>, obs: &PreferenceObservation) {
    let db = app.db.lock().await;

    let confidence_delta = obs.signal_strength * 0.2;

    // Preferences are currently global (tenant_id='default'). When we add
    // per-tenant profiles, plumb tenant_id through `PreferenceObservation`
    // and change both the INSERT value and the ON CONFLICT clause to match
    // the UNIQUE(tenant_id, category, key) constraint added in migration 003.
    let _ = db.execute(
        "INSERT INTO user_preferences (id, tenant_id, category, key, value, confidence, times_observed, source)
         VALUES (lower(hex(randomblob(16))), 'default', ?1, ?2, ?3, ?4, 1, ?5)
         ON CONFLICT(tenant_id, category, key) DO UPDATE SET
           value = CASE WHEN excluded.confidence > confidence THEN excluded.value ELSE value END,
           confidence = MIN(1.0, confidence + ?4),
           times_observed = times_observed + 1,
           source = ?5,
           updated_at = datetime('now')",
        rusqlite::params![
            obs.category,
            obs.key,
            obs.value,
            confidence_delta,
            obs.source,
        ],
    );

    info!(
        category = %obs.category,
        key = %obs.key,
        value = %obs.value,
        confidence_delta,
        "User preference observed"
    );
}

/// Record multiple observations at once (e.g., after a successful build).
pub async fn observe_batch(app: &Arc<AppState>, observations: &[PreferenceObservation]) {
    for obs in observations {
        observe(app, obs).await;
    }
}

/// Infer and store preferences from an architecture decision outcome.
pub async fn infer_from_decision(
    app: &Arc<AppState>,
    area: &str,
    value: &str,
    build_success: bool,
) {
    let signal_strength = if build_success { 0.8 } else { 0.1 };
    let source = if build_success { "build_success" } else { "build_failure" };

    observe(app, &PreferenceObservation {
        category: "tech_stack".into(),
        key: area.to_string(),
        value: value.to_string(),
        source: source.into(),
        signal_strength,
    })
    .await;
}

/// Infer UI style preferences from an intent description.
pub async fn infer_from_intent(app: &Arc<AppState>, description: &str, app_type: &str) {
    let lower = description.to_lowercase();

    // UI style signals
    let style = if lower.contains("luxur") || lower.contains("premium") || lower.contains("elegant") {
        Some("luxurious")
    } else if lower.contains("playful") || lower.contains("fun") || lower.contains("kids") || lower.contains("game") {
        Some("playful")
    } else if lower.contains("minimal") || lower.contains("clean") || lower.contains("simple") {
        Some("minimal")
    } else if lower.contains("bold") || lower.contains("dark") || lower.contains("brutalist") {
        Some("bold")
    } else if lower.contains("corporate") || lower.contains("enterprise") || lower.contains("professional") {
        Some("corporate")
    } else {
        None
    };

    if let Some(s) = style {
        observe(app, &PreferenceObservation {
            category: "ui_style".into(),
            key: "preferred_style".into(),
            value: s.into(),
            source: "pattern_match".into(),
            signal_strength: 0.5,
        })
        .await;
    }

    // Product pattern signals
    observe(app, &PreferenceObservation {
        category: "product_pattern".into(),
        key: "app_type".into(),
        value: app_type.to_string(),
        source: "pattern_match".into(),
        signal_strength: 0.6,
    })
    .await;
}

// ---------------------------------------------------------------------------
// Profile retrieval
// ---------------------------------------------------------------------------

/// Get the full user profile with all preferences.
pub async fn get_profile(app: &Arc<AppState>) -> UserProfile {
    let db = app.db.lock().await;

    let get_by_category = |cat: &str| -> Vec<UserPreference> {
        let mut stmt = match db.prepare(
            "SELECT category, key, value, confidence, times_observed, source
             FROM user_preferences WHERE category = ?1
             ORDER BY confidence DESC, times_observed DESC"
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        stmt.query_map(rusqlite::params![cat], |row| {
            Ok(UserPreference {
                category: row.get(0)?,
                key: row.get(1)?,
                value: row.get(2)?,
                confidence: row.get(3)?,
                times_observed: row.get(4)?,
                source: row.get(5)?,
            })
        })
        .ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };

    let ui_styles = get_by_category("ui_style");
    let tech_stack = get_by_category("tech_stack");
    let product_patterns = get_by_category("product_pattern");

    let total_builds: i64 = db
        .query_row("SELECT COUNT(*) FROM decision_feedback", [], |r| r.get(0))
        .unwrap_or(0);

    let dominant_style = ui_styles.first().map(|p| p.value.clone());
    let dominant_stack = tech_stack.first().map(|p| format!("{}={}", p.key, p.value));
    let dominant_pattern = product_patterns.first().map(|p| p.value.clone());

    UserProfile {
        ui_styles,
        tech_stack,
        product_patterns,
        total_builds,
        dominant_style,
        dominant_stack,
        dominant_pattern,
    }
}

/// Build a preference context string to inject into LLM prompts.
pub async fn build_adapted_context(app: &Arc<AppState>) -> AdaptedContext {
    let profile = get_profile(app).await;
    let mut ctx_parts = Vec::new();

    if let Some(ref style) = profile.dominant_style {
        ctx_parts.push(format!("Preferred UI style: {}", style));
    }

    let high_confidence_stack: Vec<(String, String)> = profile
        .tech_stack
        .iter()
        .filter(|p| p.confidence >= 0.6)
        .map(|p| (p.key.clone(), p.value.clone()))
        .collect();

    if !high_confidence_stack.is_empty() {
        let stack_str = high_confidence_stack
            .iter()
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect::<Vec<_>>()
            .join(", ");
        ctx_parts.push(format!("Preferred tech stack: {}", stack_str));
    }

    if let Some(ref pattern) = profile.dominant_pattern {
        ctx_parts.push(format!("User typically builds: {} apps", pattern));
    }

    if profile.total_builds > 0 {
        ctx_parts.push(format!("Experienced builder: {} projects", profile.total_builds));
    }

    let prompt_context = if ctx_parts.is_empty() {
        String::new()
    } else {
        format!(
            "## User Preferences (auto-learned)\n{}\nHonor these preferences unless the current request explicitly overrides them.\n",
            ctx_parts.iter().map(|s| format!("- {}", s)).collect::<Vec<_>>().join("\n")
        )
    };

    AdaptedContext {
        prompt_context,
        preferred_stack: high_confidence_stack,
        preferred_ui_style: profile.dominant_style,
        preferred_pattern: profile.dominant_pattern,
    }
}

/// Explicitly set a user preference (user-provided, highest confidence).
pub async fn set_explicit(app: &Arc<AppState>, category: &str, key: &str, value: &str) {
    observe(app, &PreferenceObservation {
        category: category.to_string(),
        key: key.to_string(),
        value: value.to_string(),
        source: "explicit".into(),
        signal_strength: 1.0,
    })
    .await;
}

/// Decay all preference confidences slightly (aging mechanism).
/// Call periodically to let stale preferences fade.
pub async fn decay_confidences(app: &Arc<AppState>) {
    let db = app.db.lock().await;
    let _ = db.execute(
        "UPDATE user_preferences SET confidence = MAX(0.1, confidence * 0.95) WHERE source != 'explicit'",
        [],
    );
}
