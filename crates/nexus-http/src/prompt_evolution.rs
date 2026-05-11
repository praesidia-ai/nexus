//! Prompt Evolution — tracks prompt→outcome mappings and mutates prompts for improvement.
//!
//! Every prompt used in the system is tracked with its outcome (success/failure),
//! token cost, and latency. Over time, prompts evolve through mutation strategies
//! that incorporate patterns from skill_dna and feedback_engine.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptVersion {
    pub version_id: String,
    /// The prompt purpose (e.g., "spec_generation", "ui_page", "api_route").
    pub purpose: String,
    /// The prompt template text.
    pub template: String,
    /// Success rate (0.0–1.0).
    pub success_rate: f64,
    /// Total times this version was used.
    pub usage_count: u32,
    /// Average token cost.
    pub avg_tokens: u64,
    /// Average latency in ms.
    pub avg_latency_ms: u64,
    /// Generation number (0 = original, 1+ = mutations).
    pub generation: u32,
    /// Parent version that this was mutated from.
    pub parent_id: Option<String>,
    /// Status: active, candidate, retired.
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptOutcome {
    pub purpose: String,
    pub success: bool,
    pub tokens_used: u64,
    pub latency_ms: u64,
    pub output_quality: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvolutionResult {
    pub original_purpose: String,
    pub new_version_id: String,
    pub mutation_type: MutationType,
    pub changes_applied: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationType {
    /// Add more specificity to the prompt.
    Specificity,
    /// Add examples.
    Examples,
    /// Add constraints.
    Constraints,
    /// Simplify / reduce prompt.
    Simplify,
    /// Restructure the prompt format.
    Restructure,
}

// ---------------------------------------------------------------------------
// Record outcomes
// ---------------------------------------------------------------------------

/// Record a prompt outcome for tracking.
pub async fn record_outcome(app: &Arc<AppState>, outcome: &PromptOutcome) {
    let db = app.db.lock().await;
    let id = uuid::Uuid::new_v4().to_string();

    // Store in global_memory as a prompt tracking entry
    let key = format!("prompt_perf_{}", outcome.purpose);
    let value = serde_json::json!({
        "success": outcome.success,
        "tokens": outcome.tokens_used,
        "latency_ms": outcome.latency_ms,
        "quality": outcome.output_quality,
    })
    .to_string();

    let _ = db.execute(
        "INSERT INTO global_memory (id, category, key, value, source, confidence, times_applied, created_at, updated_at)
         VALUES (?1, 'prompt_performance', ?2, ?3, 'prompt_evolution', 0.7, 1, datetime('now'), datetime('now'))
         ON CONFLICT(key) DO UPDATE SET
           value = ?3,
           times_applied = times_applied + 1,
           confidence = CASE
             WHEN ?4 = 1 THEN MIN(confidence + 0.02, 1.0)
             ELSE MAX(confidence - 0.05, 0.1)
           END,
           updated_at = datetime('now')",
        rusqlite::params![id, key, value, outcome.success as i32],
    );
}

// ---------------------------------------------------------------------------
// Evolve prompts
// ---------------------------------------------------------------------------

/// Evolve a prompt based on historical outcomes and learned patterns.
///
/// Returns an improved version of the prompt, incorporating:
/// - Patterns from skill_dna that worked
/// - Feedback signals from user
/// - Structural improvements based on outcome data
pub async fn evolve_prompt(
    app: &Arc<AppState>,
    base_prompt: &str,
    purpose: &str,
) -> (String, Vec<String>) {
    let mut changes = Vec::new();
    let mut evolved = base_prompt.to_string();

    let db = app.db.lock().await;

    // Check historical performance for this purpose
    let perf_key = format!("prompt_perf_{}", purpose);
    let historical: Option<String> = db
        .query_row(
            "SELECT value FROM global_memory WHERE key = ?1",
            rusqlite::params![perf_key],
            |r| r.get(0),
        )
        .ok();

    // Check for relevant skill DNA patterns
    let skill_fragments: Vec<String> = db
        .prepare(
            "SELECT prompt_fragment FROM skill_dna
             WHERE status = 'active' AND confidence > 0.6
             ORDER BY confidence DESC LIMIT 3",
        )
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map([], |r| r.get::<_, String>(0))
                .ok()
                .map(|rows| rows.filter_map(|r| r.ok()).filter(|s| !s.is_empty()).collect())
        })
        .unwrap_or_default();

    // Check for user feedback/suggestions
    let suggestions: Vec<String> = db
        .prepare(
            "SELECT value FROM global_memory
             WHERE category = 'suggestion' AND confidence > 0.5
             ORDER BY updated_at DESC LIMIT 3",
        )
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map([], |r| r.get::<_, String>(0))
                .ok()
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
        })
        .unwrap_or_default();

    drop(db);

    // ── Apply mutations ─────────────────────────────────────────────────

    // Mutation 1: Inject learned skill patterns
    if !skill_fragments.is_empty() {
        evolved.push_str("\n\n## Learned Patterns (apply these)\n");
        for frag in &skill_fragments {
            evolved.push_str(&format!("- {}\n", frag));
        }
        changes.push(format!(
            "Injected {} learned patterns from skill_dna",
            skill_fragments.len()
        ));
    }

    // Mutation 2: Apply user suggestions
    if !suggestions.is_empty() {
        evolved.push_str("\n## User Preferences\n");
        for sug in &suggestions {
            evolved.push_str(&format!("- {}\n", sug));
        }
        changes.push(format!("Applied {} user suggestions", suggestions.len()));
    }

    // Mutation 3: Specificity boost if historical success rate is low
    if let Some(ref perf_json) = historical {
        if let Ok(perf) = serde_json::from_str::<serde_json::Value>(perf_json) {
            if perf.get("success") == Some(&serde_json::Value::Bool(false)) {
                // Previous attempt failed — add more constraints
                evolved.push_str("\n## Critical Requirements (previous attempt failed)\n");
                evolved.push_str("- Output MUST be complete, production-ready code\n");
                evolved.push_str("- Do NOT use placeholder comments like 'TODO' or '...'\n");
                evolved.push_str("- Include ALL imports and dependencies\n");
                evolved.push_str("- Handle edge cases and errors\n");
                changes.push("Added specificity constraints (previous attempt failed)".into());
            }
        }
    }

    // Mutation 4: Token optimization if prompt is too long
    let token_estimate = evolved.len() / 4; // rough estimate
    if token_estimate > 8000 {
        // Truncate verbose sections rather than removing — preserve structure
        changes.push(format!(
            "Prompt is ~{} tokens — consider compression",
            token_estimate
        ));
    }

    if changes.is_empty() {
        changes.push("No evolution applied — base prompt is performing well".into());
    } else {
        info!(
            purpose = purpose,
            mutations = changes.len(),
            "Prompt evolved"
        );
    }

    (evolved, changes)
}

/// Get prompt performance summary for all tracked purposes.
pub async fn get_performance_summary(app: &Arc<AppState>) -> Vec<PromptPerformance> {
    let db = app.db.lock().await;
    let mut stmt = match db.prepare(
        "SELECT key, value, confidence, times_applied
         FROM global_memory
         WHERE category = 'prompt_performance'
         ORDER BY times_applied DESC"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    stmt.query_map([], |row| {
        Ok(PromptPerformance {
            purpose: row
                .get::<_, String>(0)?
                .trim_start_matches("prompt_perf_")
                .to_string(),
            latest_outcome: row.get(1)?,
            confidence: row.get(2)?,
            total_uses: row.get(3)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptPerformance {
    pub purpose: String,
    pub latest_outcome: String,
    pub confidence: f64,
    pub total_uses: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn evolve_prompt_returns_base_when_no_data() {
        let dir = tempfile::tempdir().unwrap();
        let app = AppState::init(dir.path()).await.unwrap();
        let (evolved, changes) = evolve_prompt(&app, "Generate a page", "ui_page").await;
        assert!(evolved.contains("Generate a page"));
        assert!(!changes.is_empty());
    }

    #[tokio::test]
    async fn record_and_retrieve_outcome() {
        let dir = tempfile::tempdir().unwrap();
        let app = AppState::init(dir.path()).await.unwrap();
        let outcome = PromptOutcome {
            purpose: "test_purpose".into(),
            success: true,
            tokens_used: 500,
            latency_ms: 2000,
            output_quality: Some(0.85),
        };
        record_outcome(&app, &outcome).await;

        let summary = get_performance_summary(&app).await;
        assert!(
            summary.iter().any(|p| p.purpose == "test_purpose"),
            "Should find recorded outcome. Got: {:?}",
            summary
        );
    }
}
