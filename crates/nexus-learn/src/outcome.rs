//! Outcome tracking — records every agent action and its result.

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcome {
    pub id: String,
    pub agent_id: String,
    pub project_id: String,
    pub action: String,
    pub tool_used: Option<String>,
    pub input_summary: String,
    pub output_summary: String,
    pub success: bool,
    pub quality_score: Option<f64>,
    pub user_feedback: Option<FeedbackType>,
    pub duration_ms: u64,
    pub tokens_used: u64,
    pub timestamp: DateTime<Utc>,
    pub context: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackType {
    Positive,
    Negative,
    Neutral,
    Corrected { correction: String },
}

pub struct OutcomeStore {
    db: Connection,
}

impl OutcomeStore {
    pub fn new(db_path: &std::path::Path) -> Result<Self, crate::LearnError> {
        let db = Connection::open(db_path)?;
        db.execute_batch(
            "
            PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS outcomes (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                project_id TEXT NOT NULL,
                action TEXT NOT NULL,
                tool_used TEXT,
                input_summary TEXT NOT NULL,
                output_summary TEXT NOT NULL,
                success INTEGER NOT NULL,
                quality_score REAL,
                user_feedback TEXT,
                duration_ms INTEGER NOT NULL,
                tokens_used INTEGER NOT NULL,
                timestamp TEXT NOT NULL,
                context TEXT NOT NULL DEFAULT '{}'
            );
            CREATE INDEX IF NOT EXISTS idx_outcomes_agent ON outcomes(agent_id);
            CREATE INDEX IF NOT EXISTS idx_outcomes_project ON outcomes(project_id);
            CREATE INDEX IF NOT EXISTS idx_outcomes_action ON outcomes(action);
            CREATE INDEX IF NOT EXISTS idx_outcomes_success ON outcomes(success);
            ",
        )?;
        Ok(Self { db })
    }

    pub fn record(&self, outcome: &Outcome) -> Result<(), crate::LearnError> {
        let feedback = outcome
            .user_feedback
            .as_ref()
            .map(|f| serde_json::to_string(f).unwrap_or_default());
        self.db.execute(
            "INSERT INTO outcomes (id, agent_id, project_id, action, tool_used, input_summary, output_summary, success, quality_score, user_feedback, duration_ms, tokens_used, timestamp, context)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            rusqlite::params![
                outcome.id,
                outcome.agent_id,
                outcome.project_id,
                outcome.action,
                outcome.tool_used,
                outcome.input_summary,
                outcome.output_summary,
                outcome.success as i32,
                outcome.quality_score,
                feedback,
                outcome.duration_ms as i64,
                outcome.tokens_used as i64,
                outcome.timestamp.to_rfc3339(),
                serde_json::to_string(&outcome.context).unwrap_or_default(),
            ],
        )?;
        Ok(())
    }

    pub fn success_rate(&self, agent_id: &str, action: &str) -> f64 {
        let result = self.db.query_row(
            "SELECT COUNT(*), SUM(success) FROM outcomes WHERE agent_id = ?1 AND action = ?2",
            rusqlite::params![agent_id, action],
            |row| {
                let total: i64 = row.get(0)?;
                let successes: i64 = row.get::<_, Option<i64>>(1)?.unwrap_or(0);
                Ok(if total > 0 {
                    successes as f64 / total as f64
                } else {
                    0.0
                })
            },
        );
        result.unwrap_or(0.0)
    }

    pub fn recent_failures(&self, project_id: &str, limit: usize) -> Vec<Outcome> {
        let mut results = Vec::new();
        if let Ok(mut stmt) = self.db.prepare(
            "SELECT id, agent_id, project_id, action, tool_used, input_summary, output_summary, success, quality_score, duration_ms, tokens_used, timestamp
             FROM outcomes WHERE project_id = ?1 AND success = 0 ORDER BY timestamp DESC LIMIT ?2",
        ) {
            let _ = stmt
                .query_map(rusqlite::params![project_id, limit as i64], |row| {
                    Ok(Outcome {
                        id: row.get(0)?,
                        agent_id: row.get(1)?,
                        project_id: row.get(2)?,
                        action: row.get(3)?,
                        tool_used: row.get(4)?,
                        input_summary: row.get(5)?,
                        output_summary: row.get(6)?,
                        success: row.get::<_, i32>(7)? != 0,
                        quality_score: row.get(8)?,
                        user_feedback: None,
                        duration_ms: row.get::<_, i64>(9)? as u64,
                        tokens_used: row.get::<_, i64>(10)? as u64,
                        timestamp: chrono::DateTime::parse_from_rfc3339(
                            &row.get::<_, String>(11)?,
                        )
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                        context: serde_json::Value::Null,
                    })
                })
                .map(|rows| {
                    for o in rows.flatten() {
                        results.push(o);
                    }
                });
        }
        results
    }

    pub fn outcomes_by_agent(&self, agent_id: &str, limit: usize) -> Vec<Outcome> {
        let mut results = Vec::new();
        if let Ok(mut stmt) = self.db.prepare(
            "SELECT id, agent_id, project_id, action, tool_used, input_summary, output_summary, success, quality_score, duration_ms, tokens_used, timestamp
             FROM outcomes WHERE agent_id = ?1 ORDER BY timestamp DESC LIMIT ?2",
        ) {
            let _ = stmt
                .query_map(rusqlite::params![agent_id, limit as i64], |row| {
                    Ok(Outcome {
                        id: row.get(0)?,
                        agent_id: row.get(1)?,
                        project_id: row.get(2)?,
                        action: row.get(3)?,
                        tool_used: row.get(4)?,
                        input_summary: row.get(5)?,
                        output_summary: row.get(6)?,
                        success: row.get::<_, i32>(7)? != 0,
                        quality_score: row.get(8)?,
                        user_feedback: None,
                        duration_ms: row.get::<_, i64>(9)? as u64,
                        tokens_used: row.get::<_, i64>(10)? as u64,
                        timestamp: chrono::DateTime::parse_from_rfc3339(
                            &row.get::<_, String>(11)?,
                        )
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                        context: serde_json::Value::Null,
                    })
                })
                .map(|rows| {
                    for o in rows.flatten() {
                        results.push(o);
                    }
                });
        }
        results
    }
}
