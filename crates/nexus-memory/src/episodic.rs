//! Episodic memory — records of specific events and experiences.
//!
//! Each episode captures a discrete event: a code generation, an agent execution,
//! user feedback, an error-and-recovery cycle, or a decision. Episodes are stored
//! in SQLite with JSON columns for flexible schema evolution and embeddings stored
//! as binary blobs for cosine-similarity recall.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::debug;

use crate::error::MemoryError;
use crate::vector_store::cosine_similarity;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub id: String,
    pub tenant_id: String,
    pub timestamp: String,
    pub agent_id: Option<String>,
    pub team_id: Option<String>,
    pub project_id: Option<String>,
    pub episode_type: EpisodeType,
    pub summary: String,
    pub details: serde_json::Value,
    pub outcome: Outcome,
    pub tags: Vec<String>,
    pub importance: f32,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EpisodeType {
    Generation {
        description: String,
        stack: Vec<String>,
        taste_score: f32,
    },
    AgentExecution {
        task: String,
        tools_used: Vec<String>,
        iterations: u32,
    },
    TeamExecution {
        task: String,
        protocol: String,
        members: Vec<String>,
    },
    UserFeedback {
        sentiment: f32,
        context: String,
    },
    ErrorAndRecovery {
        error_class: String,
        fix_applied: String,
    },
    DecisionMade {
        area: String,
        choice: String,
        rationale: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Outcome {
    Success {
        quality_score: Option<f32>,
    },
    PartialSuccess {
        what_worked: String,
        what_failed: String,
    },
    Failure {
        reason: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct RecallFilters {
    pub tenant_id: Option<String>,
    pub project_id: Option<String>,
    pub tags: Vec<String>,
    pub min_importance: Option<f32>,
    pub since: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConsolidationReport {
    pub episodes_before: usize,
    pub episodes_after: usize,
    pub consolidated: usize,
    pub deleted: usize,
}

// ---------------------------------------------------------------------------
// Helpers — embedding serialization
// ---------------------------------------------------------------------------

fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect()
}

fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| {
            let arr: [u8; 4] = chunk.try_into().unwrap();
            f32::from_le_bytes(arr)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// EpisodicMemory
// ---------------------------------------------------------------------------

pub struct EpisodicMemory {
    db_path: PathBuf,
}

impl EpisodicMemory {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            db_path: data_dir.join("episodic_memory.db"),
        }
    }

    fn open_conn(&self) -> Result<Connection, MemoryError> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        Ok(conn)
    }

    /// Create the episodes table and indices.
    pub fn init_tables(&self) -> Result<(), MemoryError> {
        let conn = self.open_conn()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS episodes (
                id           TEXT PRIMARY KEY,
                tenant_id    TEXT NOT NULL,
                timestamp    TEXT NOT NULL,
                agent_id     TEXT,
                team_id      TEXT,
                project_id   TEXT,
                episode_type TEXT NOT NULL,
                summary      TEXT NOT NULL,
                details      TEXT NOT NULL,
                outcome      TEXT NOT NULL,
                tags         TEXT NOT NULL,
                importance   REAL NOT NULL DEFAULT 0.5,
                embedding    BLOB NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_episodes_tenant    ON episodes(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_episodes_project   ON episodes(project_id);
            CREATE INDEX IF NOT EXISTS idx_episodes_timestamp ON episodes(timestamp);
            CREATE INDEX IF NOT EXISTS idx_episodes_importance ON episodes(importance);",
        )?;
        Ok(())
    }

    /// Record a new episode.
    pub fn record(&self, episode: &Episode) -> Result<(), MemoryError> {
        let conn = self.open_conn()?;
        let episode_type_json = serde_json::to_string(&episode.episode_type)
            .map_err(|e| MemoryError::Storage(e.to_string()))?;
        let outcome_json = serde_json::to_string(&episode.outcome)
            .map_err(|e| MemoryError::Storage(e.to_string()))?;
        let tags_json = serde_json::to_string(&episode.tags)
            .map_err(|e| MemoryError::Storage(e.to_string()))?;
        let embedding_bytes = embedding_to_bytes(&episode.embedding);

        conn.execute(
            "INSERT INTO episodes (id, tenant_id, timestamp, agent_id, team_id, project_id,
             episode_type, summary, details, outcome, tags, importance, embedding)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                episode.id,
                episode.tenant_id,
                episode.timestamp,
                episode.agent_id,
                episode.team_id,
                episode.project_id,
                episode_type_json,
                episode.summary,
                episode.details.to_string(),
                outcome_json,
                tags_json,
                episode.importance,
                embedding_bytes,
            ],
        )?;
        debug!(episode_id = %episode.id, "Recorded episode");
        Ok(())
    }

    /// Recall episodes similar to a query embedding, filtered by optional criteria.
    ///
    /// Loads candidate rows matching the filters, computes cosine similarity in Rust,
    /// and returns the top `limit` results sorted by descending similarity.
    pub fn recall(
        &self,
        query_embedding: &[f32],
        filters: &RecallFilters,
        limit: usize,
    ) -> Result<Vec<Episode>, MemoryError> {
        let conn = self.open_conn()?;

        // Build dynamic WHERE clause
        let mut conditions = Vec::new();
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(ref tid) = filters.tenant_id {
            conditions.push(format!("tenant_id = ?{}", param_values.len() + 1));
            param_values.push(Box::new(tid.clone()));
        }
        if let Some(ref pid) = filters.project_id {
            conditions.push(format!("project_id = ?{}", param_values.len() + 1));
            param_values.push(Box::new(pid.clone()));
        }
        if let Some(min_imp) = filters.min_importance {
            conditions.push(format!("importance >= ?{}", param_values.len() + 1));
            param_values.push(Box::new(min_imp as f64));
        }
        if let Some(ref since) = filters.since {
            conditions.push(format!("timestamp >= ?{}", param_values.len() + 1));
            param_values.push(Box::new(since.clone()));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            "SELECT id, tenant_id, timestamp, agent_id, team_id, project_id,
                    episode_type, summary, details, outcome, tags, importance, embedding
             FROM episodes {where_clause}"
        );

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|b| b.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(RawEpisodeRow {
                    id: row.get(0)?,
                    tenant_id: row.get(1)?,
                    timestamp: row.get(2)?,
                    agent_id: row.get(3)?,
                    team_id: row.get(4)?,
                    project_id: row.get(5)?,
                    episode_type: row.get(6)?,
                    summary: row.get(7)?,
                    details: row.get(8)?,
                    outcome: row.get(9)?,
                    tags: row.get(10)?,
                    importance: row.get(11)?,
                    embedding: row.get(12)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Filter by tags in Rust (SQLite JSON array matching is cumbersome)
        let tag_filter = &filters.tags;
        let mut scored: Vec<(f32, RawEpisodeRow)> = rows
            .into_iter()
            .filter(|r| {
                if tag_filter.is_empty() {
                    return true;
                }
                let row_tags: Vec<String> =
                    serde_json::from_str(&r.tags).unwrap_or_default();
                tag_filter.iter().any(|t| row_tags.contains(t))
            })
            .map(|r| {
                let emb = bytes_to_embedding(&r.embedding);
                let sim = cosine_similarity(query_embedding, &emb);
                (sim, r)
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        scored
            .into_iter()
            .map(|(_, r)| r.into_episode())
            .collect()
    }

    /// Recall episodes for a specific project, ordered by recency.
    pub fn project_history(
        &self,
        project_id: &str,
        limit: usize,
    ) -> Result<Vec<Episode>, MemoryError> {
        let conn = self.open_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, tenant_id, timestamp, agent_id, team_id, project_id,
                    episode_type, summary, details, outcome, tags, importance, embedding
             FROM episodes
             WHERE project_id = ?1
             ORDER BY timestamp DESC
             LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![project_id, limit as i64], |row| {
                Ok(RawEpisodeRow {
                    id: row.get(0)?,
                    tenant_id: row.get(1)?,
                    timestamp: row.get(2)?,
                    agent_id: row.get(3)?,
                    team_id: row.get(4)?,
                    project_id: row.get(5)?,
                    episode_type: row.get(6)?,
                    summary: row.get(7)?,
                    details: row.get(8)?,
                    outcome: row.get(9)?,
                    tags: row.get(10)?,
                    importance: row.get(11)?,
                    embedding: row.get(12)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        rows.into_iter().map(|r| r.into_episode()).collect()
    }

    /// Consolidate old episodes: delete low-importance episodes older than `retention_days`.
    ///
    /// Episodes with importance >= 0.8 are kept regardless of age.
    pub fn consolidate(&self, retention_days: u64) -> Result<ConsolidationReport, MemoryError> {
        let conn = self.open_conn()?;

        let episodes_before: usize = conn.query_row(
            "SELECT COUNT(*) FROM episodes",
            [],
            |row| row.get(0),
        )?;

        let cutoff = chrono::Utc::now()
            - chrono::Duration::days(retention_days as i64);
        let cutoff_str = cutoff.format("%Y-%m-%dT%H:%M:%SZ").to_string();

        let deleted = conn.execute(
            "DELETE FROM episodes WHERE timestamp < ?1 AND importance < 0.8",
            params![cutoff_str],
        )?;

        let episodes_after: usize = conn.query_row(
            "SELECT COUNT(*) FROM episodes",
            [],
            |row| row.get(0),
        )?;

        Ok(ConsolidationReport {
            episodes_before,
            episodes_after,
            consolidated: 0, // future: cluster + summarize before deleting
            deleted,
        })
    }

    /// Recall recent episodes for a project, optionally filtered by agent_id and tags.
    ///
    /// This is a lightweight query that does not require embeddings. Results are
    /// ordered by timestamp DESC (most recent first) and limited to `limit` rows.
    pub fn recent_by_project(
        &self,
        project_id: &str,
        agent_id: Option<&str>,
        tags: &[&str],
        limit: usize,
    ) -> Result<Vec<Episode>, MemoryError> {
        let conn = self.open_conn()?;

        let (sql, param_values): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = {
            let mut conditions = vec!["project_id = ?1".to_string()];
            let mut params: Vec<Box<dyn rusqlite::types::ToSql>> =
                vec![Box::new(project_id.to_string())];

            if let Some(aid) = agent_id {
                conditions.push(format!("agent_id = ?{}", params.len() + 1));
                params.push(Box::new(aid.to_string()));
            }

            let where_clause = conditions.join(" AND ");
            let sql = format!(
                "SELECT id, tenant_id, timestamp, agent_id, team_id, project_id,
                        episode_type, summary, details, outcome, tags, importance, embedding
                 FROM episodes
                 WHERE {}
                 ORDER BY timestamp DESC
                 LIMIT ?{}",
                where_clause,
                params.len() + 1
            );
            params.push(Box::new(limit as i64));
            (sql, params)
        };

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|b| b.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(RawEpisodeRow {
                    id: row.get(0)?,
                    tenant_id: row.get(1)?,
                    timestamp: row.get(2)?,
                    agent_id: row.get(3)?,
                    team_id: row.get(4)?,
                    project_id: row.get(5)?,
                    episode_type: row.get(6)?,
                    summary: row.get(7)?,
                    details: row.get(8)?,
                    outcome: row.get(9)?,
                    tags: row.get(10)?,
                    importance: row.get(11)?,
                    embedding: row.get(12)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Filter by tags in Rust (SQLite JSON array matching is cumbersome)
        let tag_filter: Vec<String> = tags.iter().map(|t| t.to_string()).collect();
        let filtered: Vec<RawEpisodeRow> = if tag_filter.is_empty() {
            rows
        } else {
            rows.into_iter()
                .filter(|r| {
                    let row_tags: Vec<String> =
                        serde_json::from_str(&r.tags).unwrap_or_default();
                    tag_filter.iter().any(|t| row_tags.contains(t))
                })
                .collect()
        };

        filtered.into_iter().map(|r| r.into_episode()).collect()
    }

    /// Count total episodes for a tenant.
    pub fn count(&self, tenant_id: &str) -> Result<usize, MemoryError> {
        let conn = self.open_conn()?;
        let count: usize = conn.query_row(
            "SELECT COUNT(*) FROM episodes WHERE tenant_id = ?1",
            params![tenant_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }
}

// ---------------------------------------------------------------------------
// Internal row type for deserialization
// ---------------------------------------------------------------------------

struct RawEpisodeRow {
    id: String,
    tenant_id: String,
    timestamp: String,
    agent_id: Option<String>,
    team_id: Option<String>,
    project_id: Option<String>,
    episode_type: String,
    summary: String,
    details: String,
    outcome: String,
    tags: String,
    importance: f64,
    embedding: Vec<u8>,
}

impl RawEpisodeRow {
    fn into_episode(self) -> Result<Episode, MemoryError> {
        let episode_type: EpisodeType = serde_json::from_str(&self.episode_type)
            .map_err(|e| MemoryError::Storage(format!("Bad episode_type JSON: {e}")))?;
        let outcome: Outcome = serde_json::from_str(&self.outcome)
            .map_err(|e| MemoryError::Storage(format!("Bad outcome JSON: {e}")))?;
        let details: serde_json::Value = serde_json::from_str(&self.details)
            .map_err(|e| MemoryError::Storage(format!("Bad details JSON: {e}")))?;
        let tags: Vec<String> = serde_json::from_str(&self.tags)
            .map_err(|e| MemoryError::Storage(format!("Bad tags JSON: {e}")))?;
        let embedding = bytes_to_embedding(&self.embedding);

        Ok(Episode {
            id: self.id,
            tenant_id: self.tenant_id,
            timestamp: self.timestamp,
            agent_id: self.agent_id,
            team_id: self.team_id,
            project_id: self.project_id,
            episode_type,
            summary: self.summary,
            details,
            outcome,
            tags,
            importance: self.importance as f32,
            embedding,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_memory() -> (tempfile::TempDir, EpisodicMemory) {
        let dir = tempfile::tempdir().unwrap();
        let mem = EpisodicMemory::new(dir.path());
        mem.init_tables().unwrap();
        (dir, mem)
    }

    fn sample_episode(id: &str, importance: f32, embedding: Vec<f32>) -> Episode {
        Episode {
            id: id.to_string(),
            tenant_id: "t1".to_string(),
            timestamp: "2026-01-15T10:00:00Z".to_string(),
            agent_id: Some("nova".to_string()),
            team_id: None,
            project_id: Some("proj1".to_string()),
            episode_type: EpisodeType::Generation {
                description: "Built login page".to_string(),
                stack: vec!["react".to_string(), "tailwind".to_string()],
                taste_score: 85.0,
            },
            summary: "Generated a login page with React and Tailwind".to_string(),
            details: serde_json::json!({"files": 3}),
            outcome: Outcome::Success {
                quality_score: Some(0.85),
            },
            tags: vec!["frontend".to_string(), "auth".to_string()],
            importance,
            embedding,
        }
    }

    #[test]
    fn record_and_count() {
        let (_dir, mem) = temp_memory();
        let ep = sample_episode("ep1", 0.7, vec![1.0, 0.0, 0.0]);
        mem.record(&ep).unwrap();
        assert_eq!(mem.count("t1").unwrap(), 1);
        assert_eq!(mem.count("t2").unwrap(), 0);
    }

    #[test]
    fn recall_by_embedding_similarity() {
        let (_dir, mem) = temp_memory();

        // Two episodes with different embeddings
        let ep1 = sample_episode("ep1", 0.7, vec![1.0, 0.0, 0.0]);
        let mut ep2 = sample_episode("ep2", 0.6, vec![0.0, 1.0, 0.0]);
        ep2.summary = "Built dashboard".to_string();

        mem.record(&ep1).unwrap();
        mem.record(&ep2).unwrap();

        // Query close to ep1
        let query = vec![0.9, 0.1, 0.0];
        let results = mem
            .recall(&query, &RecallFilters::default(), 10)
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "ep1"); // most similar
    }

    #[test]
    fn project_history_returns_project_episodes() {
        let (_dir, mem) = temp_memory();
        let ep1 = sample_episode("ep1", 0.7, vec![1.0, 0.0, 0.0]);
        let mut ep2 = sample_episode("ep2", 0.5, vec![0.0, 1.0, 0.0]);
        ep2.project_id = Some("proj2".to_string());

        mem.record(&ep1).unwrap();
        mem.record(&ep2).unwrap();

        let hist = mem.project_history("proj1", 10).unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].id, "ep1");
    }

    #[test]
    fn consolidate_deletes_old_low_importance() {
        let (_dir, mem) = temp_memory();

        // Old low-importance episode
        let mut old_ep = sample_episode("old", 0.3, vec![1.0, 0.0, 0.0]);
        old_ep.timestamp = "2020-01-01T00:00:00Z".to_string();

        // Old high-importance episode (should be kept)
        let mut old_important = sample_episode("old_important", 0.9, vec![0.0, 1.0, 0.0]);
        old_important.timestamp = "2020-01-01T00:00:00Z".to_string();

        // Recent episode (use a future date so it's never older than retention_days)
        let mut recent = sample_episode("recent", 0.3, vec![0.0, 0.0, 1.0]);
        recent.timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

        mem.record(&old_ep).unwrap();
        mem.record(&old_important).unwrap();
        mem.record(&recent).unwrap();

        let report = mem.consolidate(30).unwrap();
        assert_eq!(report.episodes_before, 3);
        assert_eq!(report.deleted, 1); // only the old low-importance one
        assert_eq!(report.episodes_after, 2);
    }
}
