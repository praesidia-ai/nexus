//! Agent Versioning + Mutation — track, mutate, and A/B test agent versions.
//!
//! Each agent role (architect, coder, reviewer, etc.) can have multiple versions.
//! Versions compete based on fitness score (success rate × speed × quality).
//!
//! Lifecycle:
//! 1. Champion — the current best version (active for all new tasks)
//! 2. Candidate — a mutated variant being A/B tested
//! 3. Retired — replaced by a better version
//!
//! Mutation types:
//! - Prompt tweak: adjust system prompt wording
//! - Tool adjustment: add/remove/reorder tools
//! - Iteration limit: increase/decrease max iterations
//! - Model change: try a different LLM model

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentVersion {
    pub id: String,
    pub agent_role: String,
    pub version: i64,
    pub parent_id: Option<String>,
    pub system_prompt: String,
    pub tools: Vec<String>,
    pub config: VersionConfig,
    pub mutation_type: Option<String>,
    pub mutation_desc: Option<String>,
    pub status: VersionStatus,
    pub metrics: VersionMetrics,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionConfig {
    pub max_iterations: u32,
    pub temperature: f32,
    pub model: Option<String>,
}

impl Default for VersionConfig {
    fn default() -> Self {
        Self {
            max_iterations: 15,
            temperature: 0.2,
            model: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VersionStatus {
    Candidate,
    Active,
    Champion,
    Retired,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VersionMetrics {
    pub total_uses: i64,
    pub successes: i64,
    pub failures: i64,
    pub avg_iterations: f64,
    pub avg_duration_ms: f64,
    pub fitness_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationType {
    PromptTweak,
    ToolAdd,
    ToolRemove,
    IterationAdj,
    ModelChange,
    TemperatureAdj,
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

/// Record a completed agent execution and update version metrics.
pub async fn record_execution(
    app: &Arc<AppState>,
    version_id: &str,
    success: bool,
    iterations: u32,
    duration_ms: u64,
) {
    let db = app.db.lock().await;
    let result = db.execute(
        "UPDATE agent_versions SET
            total_uses = total_uses + 1,
            successes = successes + CASE WHEN ?1 THEN 1 ELSE 0 END,
            failures = failures + CASE WHEN ?1 THEN 0 ELSE 1 END,
            avg_iterations = (avg_iterations * total_uses + ?2) / (total_uses + 1),
            avg_duration_ms = (avg_duration_ms * total_uses + ?3) / (total_uses + 1),
            fitness_score = CAST(successes + CASE WHEN ?1 THEN 1 ELSE 0 END AS REAL)
                / CAST(total_uses + 1 AS REAL)
                * (1.0 - MIN(?3 / 60000.0, 0.5)),
            updated_at = datetime('now')
         WHERE id = ?4",
        rusqlite::params![success, iterations as f64, duration_ms as f64, version_id],
    );

    if let Err(e) = result {
        warn!("Failed to record agent execution: {}", e);
    }
}

/// Shared row mapper — converts a query row into AgentVersion.
/// Expected column order: id, agent_role, version, parent_id, system_prompt, tools, config,
///   mutation_type, mutation_desc, status, total_uses, successes, failures,
///   avg_iterations, avg_duration_ms, fitness_score, created_at
fn row_to_agent_version(row: &rusqlite::Row) -> rusqlite::Result<AgentVersion> {
    Ok(AgentVersion {
        id: row.get(0)?,
        agent_role: row.get(1)?,
        version: row.get(2)?,
        parent_id: row.get(3)?,
        system_prompt: row.get(4)?,
        tools: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
        config: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default(),
        mutation_type: row.get(7)?,
        mutation_desc: row.get(8)?,
        status: match row.get::<_, String>(9)?.as_str() {
            "champion" => VersionStatus::Champion,
            "active" => VersionStatus::Active,
            "retired" => VersionStatus::Retired,
            _ => VersionStatus::Candidate,
        },
        metrics: VersionMetrics {
            total_uses: row.get(10)?,
            successes: row.get(11)?,
            failures: row.get(12)?,
            avg_iterations: row.get(13)?,
            avg_duration_ms: row.get(14)?,
            fitness_score: row.get(15)?,
        },
        created_at: row.get(16)?,
    })
}

const AGENT_VERSION_COLUMNS: &str =
    "id, agent_role, version, parent_id, system_prompt, tools, config, \
     mutation_type, mutation_desc, status, total_uses, successes, failures, \
     avg_iterations, avg_duration_ms, fitness_score, created_at";

/// Get the champion version for an agent role (highest fitness with sufficient data).
pub async fn get_champion(app: &Arc<AppState>, agent_role: &str) -> Option<AgentVersion> {
    let db = app.db.lock().await;
    let query = format!(
        "SELECT {} FROM agent_versions WHERE agent_role = ?1 AND status IN ('champion', 'active') \
         ORDER BY fitness_score DESC, total_uses DESC LIMIT 1",
        AGENT_VERSION_COLUMNS
    );
    let mut stmt = db.prepare(&query).ok()?;
    stmt.query_row(rusqlite::params![agent_role], row_to_agent_version).ok()
}

/// Create the initial (v1) version for an agent role.
pub async fn create_initial_version(
    app: &Arc<AppState>,
    agent_role: &str,
    system_prompt: &str,
    tools: &[String],
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let tools_json = serde_json::to_string(tools).unwrap_or_else(|_| "[]".into());
    let config_json = serde_json::to_string(&VersionConfig::default()).unwrap_or_else(|_| "{}".into());

    let db = app.db.lock().await;
    db.execute(
        "INSERT INTO agent_versions (id, agent_role, version, system_prompt, tools, config, status)
         VALUES (?1, ?2, 1, ?3, ?4, ?5, 'champion')",
        rusqlite::params![id, agent_role, system_prompt, tools_json, config_json],
    ).map_err(|e| format!("Failed to create agent version: {}", e))?;

    info!(role = agent_role, id = %id, "Created initial agent version v1");
    Ok(id)
}

/// Generate a mutated version from the current champion.
pub async fn mutate_champion(
    app: &Arc<AppState>,
    agent_role: &str,
    mutation: MutationType,
    description: &str,
) -> Result<String, String> {
    let champion = get_champion(app, agent_role).await
        .ok_or_else(|| format!("No champion found for role: {}", agent_role))?;

    let id = uuid::Uuid::new_v4().to_string();
    let new_version = champion.version + 1;

    let (new_prompt, new_tools, new_config) = apply_mutation(
        &champion.system_prompt,
        &champion.tools,
        &champion.config,
        &mutation,
    );

    let tools_json = serde_json::to_string(&new_tools).unwrap_or_else(|_| "[]".into());
    let config_json = serde_json::to_string(&new_config).unwrap_or_else(|_| "{}".into());
    let mutation_type_str = format!("{:?}", mutation).to_lowercase();

    let db = app.db.lock().await;
    db.execute(
        "INSERT INTO agent_versions (id, agent_role, version, parent_id, system_prompt, tools, config,
            mutation_type, mutation_desc, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'candidate')",
        rusqlite::params![
            id, agent_role, new_version, champion.id,
            new_prompt, tools_json, config_json,
            mutation_type_str, description,
        ],
    ).map_err(|e| format!("Failed to create mutant: {}", e))?;

    info!(role = agent_role, version = new_version, mutation = mutation_type_str, "Created mutant agent version");
    Ok(id)
}

/// Check if a candidate should be promoted to champion.
/// Requires: >= 10 uses AND fitness_score > champion's fitness.
pub async fn check_promotion(app: &Arc<AppState>, agent_role: &str) -> Option<String> {
    let db = app.db.lock().await;

    // Get current champion fitness
    let champion_fitness: f64 = db.query_row(
        "SELECT COALESCE(MAX(fitness_score), 0.0) FROM agent_versions
         WHERE agent_role = ?1 AND status = 'champion'",
        rusqlite::params![agent_role],
        |row| row.get(0),
    ).unwrap_or(0.0);

    // Find best candidate that beats the champion
    let result: Result<(String, f64), _> = db.query_row(
        "SELECT id, fitness_score FROM agent_versions
         WHERE agent_role = ?1 AND status = 'candidate' AND total_uses >= 10
           AND fitness_score > ?2
         ORDER BY fitness_score DESC LIMIT 1",
        rusqlite::params![agent_role, champion_fitness],
        |row| Ok((row.get(0)?, row.get(1)?)),
    );

    match result {
        Ok((candidate_id, new_fitness)) => {
            // Promote candidate, retire old champion
            let _ = db.execute(
                "UPDATE agent_versions SET status = 'retired', updated_at = datetime('now')
                 WHERE agent_role = ?1 AND status = 'champion'",
                rusqlite::params![agent_role],
            );
            let _ = db.execute(
                "UPDATE agent_versions SET status = 'champion', updated_at = datetime('now')
                 WHERE id = ?1",
                rusqlite::params![candidate_id],
            );

            info!(
                role = agent_role,
                old_fitness = champion_fitness,
                new_fitness = new_fitness,
                "Promoted new champion agent version"
            );
            Some(candidate_id)
        }
        Err(_) => None,
    }
}

/// List all versions for a role.
pub async fn list_versions(app: &Arc<AppState>, agent_role: &str) -> Vec<AgentVersion> {
    let db = app.db.lock().await;
    let query = format!(
        "SELECT {} FROM agent_versions WHERE agent_role = ?1 ORDER BY version DESC",
        AGENT_VERSION_COLUMNS
    );
    let mut stmt = match db.prepare(&query) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let rows = stmt.query_map(rusqlite::params![agent_role], row_to_agent_version).ok();

    rows.map(|iter| iter.filter_map(|r| r.ok()).collect()).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Mutation Logic
// ---------------------------------------------------------------------------

fn apply_mutation(
    prompt: &str,
    tools: &[String],
    config: &VersionConfig,
    mutation: &MutationType,
) -> (String, Vec<String>, VersionConfig) {
    let mut new_prompt = prompt.to_string();
    let mut new_tools = tools.to_vec();
    let mut new_config = config.clone();

    match mutation {
        MutationType::PromptTweak => {
            // Add specificity to the prompt
            new_prompt.push_str(
                "\n\nIMPORTANT: Focus on producing minimal, correct changes. \
                 Avoid unnecessary modifications. Explain your reasoning briefly before acting."
            );
        }
        MutationType::ToolAdd => {
            // Add grep if not present (common improvement)
            if !new_tools.contains(&"grep".to_string()) {
                new_tools.push("grep".to_string());
            }
        }
        MutationType::ToolRemove => {
            // Remove least-used tool (heuristic: remove last non-essential tool)
            let essential = ["file_read", "file_write", "bash"];
            if let Some(pos) = new_tools.iter().rposition(|t| !essential.contains(&t.as_str())) {
                new_tools.remove(pos);
            }
        }
        MutationType::IterationAdj => {
            // Increase iterations by 20% (allows more exploration)
            new_config.max_iterations = ((new_config.max_iterations as f64 * 1.2).ceil()) as u32;
        }
        MutationType::ModelChange => {
            // No-op here — model change is handled by the caller
        }
        MutationType::TemperatureAdj => {
            // Slightly increase temperature for more creative exploration
            new_config.temperature = (new_config.temperature + 0.05).min(0.5);
        }
    }

    (new_prompt, new_tools, new_config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_adds_to_prompt() {
        let config = VersionConfig::default();
        let (new_prompt, _, _) = apply_mutation(
            "You are an architect.",
            &["file_read".into()],
            &config,
            &MutationType::PromptTweak,
        );
        assert!(new_prompt.contains("minimal, correct"));
    }

    #[test]
    fn mutation_adjusts_iterations() {
        let config = VersionConfig { max_iterations: 10, ..Default::default() };
        let (_, _, new_config) = apply_mutation(
            "prompt",
            &[],
            &config,
            &MutationType::IterationAdj,
        );
        assert_eq!(new_config.max_iterations, 12);
    }
}
