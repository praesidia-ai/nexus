//! Memory Unification — merges all learning systems into one queryable layer.
//!
//! Unifies: global_memory + project_brain + skill_dna
//! Provides: cross-project learning, pattern reinforcement, decay logic.

use std::sync::Arc;

use serde::Serialize;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Unified memory view
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct UnifiedMemory {
    /// Global patterns (from global_memory table).
    pub global_patterns: Vec<MemoryEntry>,
    /// Project-specific patterns (from ProjectIntelligence).
    pub project_patterns: Vec<String>,
    /// Skill patterns (from skill_dna table).
    pub skill_patterns: Vec<SkillEntry>,
    /// Aggregate success rates by category.
    pub success_rates: Vec<SuccessRate>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryEntry {
    pub key: String,
    pub value: String,
    pub category: String,
    pub confidence: f64,
    pub times_applied: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillEntry {
    pub name: String,
    pub intent: String,
    pub confidence: f64,
    pub total_uses: i64,
    pub successes: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SuccessRate {
    pub category: String,
    pub rate: f64,
    pub sample_size: i64,
}

// ---------------------------------------------------------------------------
// Load unified memory
// ---------------------------------------------------------------------------

/// Load unified memory from all sources.
pub async fn load(app: &Arc<AppState>, project_id: Option<&str>) -> UnifiedMemory {
    let db = app.db.lock().await;

    // Global patterns
    let global_patterns = load_global_patterns(&db);

    // Skill patterns
    let skill_patterns = load_skill_patterns(&db);

    // Success rates
    let success_rates = compute_success_rates(&db);

    drop(db);

    // Project patterns
    let project_patterns = if let Some(pid) = project_id {
        let project_dir = app.data_dir.join("projects").join(pid).join("generated");
        let intel = crate::project_brain::ProjectIntelligence::load(&project_dir);
        intel.successful_patterns
    } else {
        Vec::new()
    };

    UnifiedMemory {
        global_patterns,
        project_patterns,
        skill_patterns,
        success_rates,
    }
}

fn load_global_patterns(db: &rusqlite::Connection) -> Vec<MemoryEntry> {
    let mut stmt = match db.prepare(
        "SELECT key, value, category, confidence, times_applied
         FROM global_memory
         WHERE confidence > 0.3
         ORDER BY confidence DESC, times_applied DESC
         LIMIT 50"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    stmt.query_map([], |row| {
        Ok(MemoryEntry {
            key: row.get(0)?,
            value: row.get(1)?,
            category: row.get(2)?,
            confidence: row.get(3)?,
            times_applied: row.get(4)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

fn load_skill_patterns(db: &rusqlite::Connection) -> Vec<SkillEntry> {
    let mut stmt = match db.prepare(
        "SELECT name, intent, confidence, total_uses, successes
         FROM skill_dna
         WHERE status IN ('active', 'validated') AND confidence > 0.3
         ORDER BY confidence DESC
         LIMIT 30"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    stmt.query_map([], |row| {
        Ok(SkillEntry {
            name: row.get(0)?,
            intent: row.get(1)?,
            confidence: row.get(2)?,
            total_uses: row.get(3)?,
            successes: row.get(4)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

fn compute_success_rates(db: &rusqlite::Connection) -> Vec<SuccessRate> {
    let mut rates = Vec::new();

    // Decision success rate
    if let Ok(row) = db.query_row(
        "SELECT COUNT(*) as total,
                SUM(CASE WHEN outcome = 'success' THEN 1 ELSE 0 END) as successes
         FROM decision_feedback",
        [],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
    ) {
        let (total, successes) = row;
        if total > 0 {
            rates.push(SuccessRate {
                category: "decisions".into(),
                rate: successes as f64 / total as f64,
                sample_size: total,
            });
        }
    }

    // Build success rate
    if let Ok(row) = db.query_row(
        "SELECT COUNT(*) as total,
                SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END) as successes
         FROM run_metrics",
        [],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
    ) {
        let (total, successes) = row;
        if total > 0 {
            rates.push(SuccessRate {
                category: "builds".into(),
                rate: successes as f64 / total as f64,
                sample_size: total,
            });
        }
    }

    // Agent success rate
    if let Ok(row) = db.query_row(
        "SELECT COUNT(*) as total,
                SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END) as successes
         FROM agent_traces",
        [],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
    ) {
        let (total, successes) = row;
        if total > 0 {
            rates.push(SuccessRate {
                category: "agents".into(),
                rate: successes as f64 / total as f64,
                sample_size: total,
            });
        }
    }

    rates
}

// ---------------------------------------------------------------------------
// Reinforce / Decay
// ---------------------------------------------------------------------------

/// Reinforce a pattern that was successful.
pub async fn reinforce(app: &Arc<AppState>, key: &str, category: &str) {
    let db = app.db.lock().await;
    let id = uuid::Uuid::new_v4().to_string();
    // Single UPSERT — atomic, no race condition
    if let Err(e) = db.execute(
        "INSERT INTO global_memory
         (id, category, key, value, source, confidence, times_applied, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?3, 'reinforcement', 0.6, 1, datetime('now'), datetime('now'))
         ON CONFLICT(key) DO UPDATE SET
           confidence = MIN(confidence + 0.03, 1.0),
           times_applied = times_applied + 1,
           updated_at = datetime('now')",
        rusqlite::params![id, category, key],
    ) {
        tracing::warn!(key = %key, error = %e, "Failed to reinforce memory pattern");
    }
}

/// Decay old, unused patterns.
pub async fn decay_old_patterns(app: &Arc<AppState>) {
    let db = app.db.lock().await;

    // Decay global memory that hasn't been used recently
    let _ = db.execute(
        "UPDATE global_memory
         SET confidence = MAX(confidence - 0.01, 0.0),
             updated_at = datetime('now')
         WHERE updated_at < datetime('now', '-30 days')
           AND confidence > 0.1",
        [],
    );

    // Archive dead global memory
    let _ = db.execute(
        "DELETE FROM global_memory WHERE confidence <= 0.05 AND times_applied < 3",
        [],
    );

    // Decay old skill DNA
    let _ = db.execute(
        "UPDATE skill_dna
         SET confidence = MAX(confidence - 0.01, 0.0),
             updated_at = datetime('now')
         WHERE updated_at < datetime('now', '-30 days')
           AND status = 'active'
           AND confidence > 0.1",
        [],
    );
}

/// Convert unified memory to LLM context.
pub fn to_context(memory: &UnifiedMemory) -> String {
    let mut ctx = String::new();

    if !memory.global_patterns.is_empty() {
        ctx.push_str("## Learned Global Patterns\n");
        for p in memory.global_patterns.iter().take(10) {
            ctx.push_str(&format!(
                "- [{}] {}: {} (confidence: {:.0}%)\n",
                p.category, p.key, p.value, p.confidence * 100.0
            ));
        }
    }

    if !memory.project_patterns.is_empty() {
        ctx.push_str("\n## Project-Specific Patterns\n");
        for p in &memory.project_patterns {
            ctx.push_str(&format!("- {}\n", p));
        }
    }

    if !memory.skill_patterns.is_empty() {
        ctx.push_str("\n## Active Skills\n");
        for s in memory.skill_patterns.iter().take(5) {
            ctx.push_str(&format!(
                "- {} ({}): {:.0}% confidence, {}/{} success\n",
                s.name, s.intent, s.confidence * 100.0, s.successes, s.total_uses
            ));
        }
    }

    if !memory.success_rates.is_empty() {
        ctx.push_str("\n## System Success Rates\n");
        for r in &memory.success_rates {
            ctx.push_str(&format!(
                "- {}: {:.0}% (n={})\n",
                r.category, r.rate * 100.0, r.sample_size
            ));
        }
    }

    ctx
}
