//! Skill DNA — auto-extracted reusable intelligence from agent executions.
//!
//! After each successful agent run, the system extracts reusable patterns
//! and stores them as "skill DNA" — composable prompt fragments that can be
//! injected into future agent runs.
//!
//! Lifecycle:
//! 1. Extract — analyze execution traces for recurring patterns
//! 2. Store — persist as structured skill definitions
//! 3. Compose — dynamically build prompts from skill combinations
//! 4. Track — measure skill effectiveness over time
//! 5. Evolve — merge similar skills, prune ineffective ones

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDna {
    pub id: String,
    pub name: String,
    pub description: String,
    pub intent: String,
    pub tools: Vec<String>,
    pub patterns: Vec<SkillPattern>,
    pub examples: Vec<SkillExample>,
    pub constraints: Vec<String>,
    pub prompt_fragment: String,
    pub source_type: SkillSource,
    pub source_executions: Vec<String>,
    pub parent_ids: Vec<String>,
    pub generation: i64,
    pub metrics: SkillMetrics,
    pub status: SkillStatus,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPattern {
    pub pattern_type: String,
    pub description: String,
    pub code_template: Option<String>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillExample {
    pub input: String,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillMetrics {
    pub total_uses: i64,
    pub successes: i64,
    pub failures: i64,
    pub confidence: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SkillSource {
    Auto,
    Manual,
    Merged,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SkillStatus {
    Draft,
    Validated,
    Active,
    Archived,
}

// ---------------------------------------------------------------------------
// Extraction — auto-extract skills from execution traces
// ---------------------------------------------------------------------------

/// Extract skill DNA from a completed execution.
pub fn extract_from_execution(
    execution_id: &str,
    task_description: &str,
    agent_phases: &[PhaseInfo],
    files_changed: &[String],
    tools_used: &[String],
    success: bool,
) -> Option<SkillDna> {
    if !success {
        return None; // Only extract from successful executions
    }

    // Detect the dominant pattern
    let intent = infer_intent(task_description);
    let patterns = detect_patterns(agent_phases, files_changed);

    if patterns.is_empty() {
        return None; // Nothing reusable detected
    }

    let name = generate_skill_name(&intent, &patterns);
    let prompt_fragment = generate_prompt_fragment(&intent, &patterns, files_changed);

    Some(SkillDna {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        description: format!("Auto-extracted from execution {}", execution_id),
        intent,
        tools: tools_used.to_vec(),
        patterns,
        examples: vec![SkillExample {
            input: task_description.to_string(),
            output: format!("Modified {} files", files_changed.len()),
        }],
        constraints: vec![],
        prompt_fragment,
        source_type: SkillSource::Auto,
        source_executions: vec![execution_id.to_string()],
        parent_ids: vec![],
        generation: 0,
        metrics: SkillMetrics {
            total_uses: 1,
            successes: 1,
            failures: 0,
            confidence: 0.5, // starts at 50%, improves with more data
        },
        status: SkillStatus::Draft,
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

/// Helper type for phase information during extraction.
#[derive(Debug, Clone)]
pub struct PhaseInfo {
    pub phase: String,
    pub agent: String,
    pub files_touched: Vec<String>,
    pub tools_used: Vec<String>,
    pub success: bool,
}

fn infer_intent(description: &str) -> String {
    let lower = description.to_lowercase();

    if lower.contains("bug") || lower.contains("fix") || lower.contains("error") {
        return "bug_fix".into();
    }
    if lower.contains("add") || lower.contains("create") || lower.contains("implement") || lower.contains("build") {
        return "feature_add".into();
    }
    if lower.contains("refactor") || lower.contains("clean") || lower.contains("reorganize") {
        return "refactor".into();
    }
    if lower.contains("test") || lower.contains("spec") || lower.contains("coverage") {
        return "testing".into();
    }
    if lower.contains("style") || lower.contains("ui") || lower.contains("design") || lower.contains("css") {
        return "ui_improvement".into();
    }
    if lower.contains("api") || lower.contains("endpoint") || lower.contains("route") {
        return "api_development".into();
    }
    if lower.contains("database") || lower.contains("schema") || lower.contains("migration") {
        return "data_modeling".into();
    }
    "general".into()
}

fn detect_patterns(phases: &[PhaseInfo], files: &[String]) -> Vec<SkillPattern> {
    let mut patterns = Vec::new();

    // File extension analysis → detect tech patterns
    let extensions: Vec<&str> = files.iter()
        .filter_map(|f| f.rsplit('.').next())
        .collect();

    let ext_counts: HashMap<&str, usize> = extensions.iter()
        .fold(HashMap::new(), |mut map, ext| { *map.entry(ext).or_default() += 1; map });

    if ext_counts.get("tsx").unwrap_or(&0) + ext_counts.get("jsx").unwrap_or(&0) > 2 {
        patterns.push(SkillPattern {
            pattern_type: "react_component".into(),
            description: "Multiple React components created/modified".into(),
            code_template: None,
            confidence: 0.7,
        });
    }

    if *ext_counts.get("ts").unwrap_or(&0) > 0 && files.iter().any(|f| f.contains("api/") || f.contains("route")) {
        patterns.push(SkillPattern {
            pattern_type: "api_route".into(),
            description: "API route development pattern".into(),
            code_template: None,
            confidence: 0.7,
        });
    }

    if files.iter().any(|f| f.contains("schema") || f.contains("migration") || f.contains("prisma")) {
        patterns.push(SkillPattern {
            pattern_type: "database_schema".into(),
            description: "Database schema modification pattern".into(),
            code_template: None,
            confidence: 0.8,
        });
    }

    // Multi-phase patterns
    let phase_names: Vec<&str> = phases.iter().map(|p| p.phase.as_str()).collect();
    if phase_names.contains(&"architect") && phase_names.contains(&"coder") && phase_names.contains(&"tester") {
        patterns.push(SkillPattern {
            pattern_type: "full_feature".into(),
            description: "Complete feature implementation with design, code, and tests".into(),
            code_template: None,
            confidence: 0.9,
        });
    }

    patterns
}

fn generate_skill_name(intent: &str, patterns: &[SkillPattern]) -> String {
    let pattern_desc = patterns.first()
        .map(|p| p.pattern_type.as_str())
        .unwrap_or("general");

    format!("{}-{}", intent.replace('_', "-"), pattern_desc.replace('_', "-"))
}

fn generate_prompt_fragment(intent: &str, patterns: &[SkillPattern], files: &[String]) -> String {
    let mut fragment = format!("When performing {} tasks:\n", intent.replace('_', " "));

    for pattern in patterns {
        fragment.push_str(&format!("- Apply {} pattern: {}\n", pattern.pattern_type, pattern.description));
    }

    if !files.is_empty() {
        let sample_files: Vec<&str> = files.iter().take(5).map(|f| f.as_str()).collect();
        fragment.push_str(&format!("- Example file structure: {}\n", sample_files.join(", ")));
    }

    fragment
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

/// Store a new skill DNA in the database.
pub async fn store_skill(app: &Arc<AppState>, skill: &SkillDna) -> Result<(), String> {
    let db = app.db.lock().await;
    db.execute(
        "INSERT OR IGNORE INTO skill_dna
            (id, name, description, intent, tools, patterns, examples, constraints,
             prompt_fragment, source_type, source_executions, parent_ids, generation,
             total_uses, successes, failures, confidence, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        rusqlite::params![
            skill.id,
            skill.name,
            skill.description,
            skill.intent,
            serde_json::to_string(&skill.tools).unwrap_or_default(),
            serde_json::to_string(&skill.patterns).unwrap_or_default(),
            serde_json::to_string(&skill.examples).unwrap_or_default(),
            serde_json::to_string(&skill.constraints).unwrap_or_default(),
            skill.prompt_fragment,
            format!("{:?}", skill.source_type).to_lowercase(),
            serde_json::to_string(&skill.source_executions).unwrap_or_default(),
            serde_json::to_string(&skill.parent_ids).unwrap_or_default(),
            skill.generation,
            skill.metrics.total_uses,
            skill.metrics.successes,
            skill.metrics.failures,
            skill.metrics.confidence,
            format!("{:?}", skill.status).to_lowercase(),
        ],
    ).map_err(|e| format!("Store skill failed: {}", e))?;

    info!(id = %skill.id, name = %skill.name, "Stored skill DNA");
    Ok(())
}

/// Lightweight summary for listing — avoids deserializing large JSON fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDnaSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub intent: String,
    pub prompt_fragment: String,
    pub confidence: f64,
    pub total_uses: i64,
}

/// Get active skills relevant to an intent (summary view).
pub async fn get_skills_for_intent(app: &Arc<AppState>, intent: &str) -> Vec<SkillDnaSummary> {
    let db = app.db.lock().await;
    let mut stmt = match db.prepare(
        "SELECT id, name, description, intent, prompt_fragment, confidence, total_uses
         FROM skill_dna
         WHERE status = 'active' AND intent = ?1
         ORDER BY confidence DESC, total_uses DESC
         LIMIT 5"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let rows = stmt.query_map(rusqlite::params![intent], |row| {
        Ok(SkillDnaSummary {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            intent: row.get(3)?,
            prompt_fragment: row.get(4)?,
            confidence: row.get(5)?,
            total_uses: row.get(6)?,
        })
    }).ok();

    rows.map(|iter| iter.filter_map(|r| r.ok()).collect()).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Composition — build agent prompts from skills
// ---------------------------------------------------------------------------

/// Compose a system prompt by injecting relevant skill DNA fragments.
pub async fn compose_prompt(
    app: &Arc<AppState>,
    base_prompt: &str,
    task_description: &str,
) -> String {
    let intent = infer_intent(task_description);
    let skills = get_skills_for_intent(app, &intent).await;

    if skills.is_empty() {
        return base_prompt.to_string();
    }

    let mut composed = base_prompt.to_string();
    composed.push_str("\n\n## Learned Patterns (from successful past executions)\n\n");

    for skill in &skills {
        if !skill.prompt_fragment.is_empty() {
            composed.push_str(&format!(
                "### {} (confidence: {:.0}%)\n{}\n\n",
                skill.name,
                skill.confidence * 100.0,
                skill.prompt_fragment
            ));
        }
    }

    composed
}

/// Record that a skill was used (for tracking).
pub async fn record_skill_usage(app: &Arc<AppState>, skill_id: &str, success: bool) {
    let db = app.db.lock().await;
    let _ = db.execute(
        "UPDATE skill_dna SET
            total_uses = total_uses + 1,
            successes = successes + CASE WHEN ?1 THEN 1 ELSE 0 END,
            failures = failures + CASE WHEN ?1 THEN 0 ELSE 1 END,
            confidence = CAST(successes + CASE WHEN ?1 THEN 1 ELSE 0 END AS REAL)
                / CAST(total_uses + 1 AS REAL),
            updated_at = datetime('now')
         WHERE id = ?2",
        rusqlite::params![success, skill_id],
    );
}

/// Promote draft skills to active (after sufficient successful uses).
/// Returns the number of skills promoted.
pub async fn promote_if_ready(app: &Arc<AppState>) -> usize {
    let db = app.db.lock().await;

    // Promote skills with >= 3 uses and >= 70% confidence
    let count = db.execute(
        "UPDATE skill_dna SET status = 'active', updated_at = datetime('now')
         WHERE status = 'draft' AND total_uses >= 3 AND confidence >= 0.7",
        [],
    ).unwrap_or(0);

    if count > 0 {
        info!(count, "Promoted draft skills to active");
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_bug_fix_intent() {
        assert_eq!(infer_intent("fix the login bug"), "bug_fix");
    }

    #[test]
    fn infers_feature_add_intent() {
        assert_eq!(infer_intent("add a new dashboard page"), "feature_add");
    }

    #[test]
    fn extracts_skill_from_successful_execution() {
        let phases = vec![PhaseInfo {
            phase: "coder".into(),
            agent: "coder".into(),
            files_touched: vec!["app/page.tsx".into()],
            tools_used: vec!["file_write".into()],
            success: true,
        }];
        let files = vec!["app/page.tsx".into(), "app/layout.tsx".into(), "components/nav.tsx".into()];
        let tools = vec!["file_write".into(), "file_read".into()];

        let skill = extract_from_execution("exec_001", "add a navigation component", &phases, &files, &tools, true);
        assert!(skill.is_some());
        let skill = skill.unwrap();
        assert_eq!(skill.intent, "feature_add");
        assert!(!skill.prompt_fragment.is_empty());
    }

    #[test]
    fn does_not_extract_from_failures() {
        let skill = extract_from_execution("exec_002", "fix bug", &[], &[], &[], false);
        assert!(skill.is_none());
    }
}
