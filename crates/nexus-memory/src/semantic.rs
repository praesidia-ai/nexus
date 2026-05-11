//! Semantic memory — distilled knowledge and facts.
//!
//! Unlike episodic memory (which records events), semantic memory stores
//! generalised knowledge: user preferences, project patterns, technical facts,
//! error patterns, and domain knowledge. Facts are reinforced when re-learned
//! and decay when not confirmed over time.

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
pub struct KnowledgeFact {
    pub id: String,
    pub tenant_id: String,
    pub category: KnowledgeCategory,
    pub subject: String,
    pub content: String,
    pub confidence: f32,
    pub source_episodes: Vec<String>,
    pub embedding: Vec<f32>,
    pub last_confirmed: String,
    pub times_confirmed: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeCategory {
    UserPreference,
    ProjectPattern,
    TechnicalFact,
    QualityPattern,
    ErrorPattern,
    StackCompatibility,
    DomainKnowledge,
}

impl std::fmt::Display for KnowledgeCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::UserPreference => "user_preference",
            Self::ProjectPattern => "project_pattern",
            Self::TechnicalFact => "technical_fact",
            Self::QualityPattern => "quality_pattern",
            Self::ErrorPattern => "error_pattern",
            Self::StackCompatibility => "stack_compatibility",
            Self::DomainKnowledge => "domain_knowledge",
        };
        write!(f, "{s}")
    }
}

// ---------------------------------------------------------------------------
// Helpers
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
// SemanticMemory
// ---------------------------------------------------------------------------

pub struct SemanticMemory {
    db_path: PathBuf,
}

impl SemanticMemory {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            db_path: data_dir.join("semantic_memory.db"),
        }
    }

    fn open_conn(&self) -> Result<Connection, MemoryError> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        Ok(conn)
    }

    /// Create the knowledge_facts table and indices.
    pub fn init_tables(&self) -> Result<(), MemoryError> {
        let conn = self.open_conn()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS knowledge_facts (
                id               TEXT PRIMARY KEY,
                tenant_id        TEXT NOT NULL,
                category         TEXT NOT NULL,
                subject          TEXT NOT NULL,
                content          TEXT NOT NULL,
                confidence       REAL NOT NULL DEFAULT 0.5,
                source_episodes  TEXT NOT NULL DEFAULT '[]',
                embedding        BLOB NOT NULL,
                last_confirmed   TEXT NOT NULL,
                times_confirmed  INTEGER NOT NULL DEFAULT 1
            );
            CREATE INDEX IF NOT EXISTS idx_facts_tenant     ON knowledge_facts(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_facts_category   ON knowledge_facts(category);
            CREATE INDEX IF NOT EXISTS idx_facts_confidence ON knowledge_facts(confidence);
            CREATE INDEX IF NOT EXISTS idx_facts_subject    ON knowledge_facts(subject);",
        )?;
        Ok(())
    }

    /// Query knowledge similar to a query embedding, optionally filtered by categories.
    pub fn query(
        &self,
        query_embedding: &[f32],
        categories: Option<&[KnowledgeCategory]>,
        limit: usize,
    ) -> Result<Vec<KnowledgeFact>, MemoryError> {
        let conn = self.open_conn()?;

        let (sql, param_values): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
            if let Some(cats) = categories {
                if cats.is_empty() {
                    let sql = "SELECT id, tenant_id, category, subject, content, confidence,
                               source_episodes, embedding, last_confirmed, times_confirmed
                        FROM knowledge_facts".to_string();
                    (sql, vec![])
                } else {
                    let placeholders: Vec<String> = cats
                        .iter()
                        .enumerate()
                        .map(|(i, _)| format!("?{}", i + 1))
                        .collect();
                    let sql = format!(
                        "SELECT id, tenant_id, category, subject, content, confidence,
                                source_episodes, embedding, last_confirmed, times_confirmed
                         FROM knowledge_facts
                         WHERE category IN ({})",
                        placeholders.join(", ")
                    );
                    let params: Vec<Box<dyn rusqlite::types::ToSql>> = cats
                        .iter()
                        .map(|c| Box::new(c.to_string()) as Box<dyn rusqlite::types::ToSql>)
                        .collect();
                    (sql, params)
                }
            } else {
                let sql = "SELECT id, tenant_id, category, subject, content, confidence,
                           source_episodes, embedding, last_confirmed, times_confirmed
                    FROM knowledge_facts".to_string();
                (sql, vec![])
            };

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|b| b.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(RawFactRow {
                    id: row.get(0)?,
                    tenant_id: row.get(1)?,
                    category: row.get(2)?,
                    subject: row.get(3)?,
                    content: row.get(4)?,
                    confidence: row.get(5)?,
                    source_episodes: row.get(6)?,
                    embedding: row.get(7)?,
                    last_confirmed: row.get(8)?,
                    times_confirmed: row.get(9)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut scored: Vec<(f32, RawFactRow)> = rows
            .into_iter()
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
            .map(|(_, r)| r.into_fact())
            .collect()
    }

    /// Learn or reinforce a fact.
    ///
    /// If a fact with embedding similarity > 0.9 already exists (same tenant + category),
    /// reinforce it by incrementing `times_confirmed` and updating confidence.
    /// Otherwise, insert a new fact.
    pub fn learn(&self, fact: &KnowledgeFact) -> Result<(), MemoryError> {
        let conn = self.open_conn()?;

        // Check for existing similar facts in the same category and tenant
        let mut stmt = conn.prepare(
            "SELECT id, embedding, confidence, times_confirmed
             FROM knowledge_facts
             WHERE tenant_id = ?1 AND category = ?2",
        )?;
        let existing: Vec<(String, Vec<u8>, f64, u32)> = stmt
            .query_map(params![fact.tenant_id, fact.category.to_string()], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Find the most similar existing fact
        let mut best_match: Option<(String, f32, f64, u32)> = None;
        for (id, emb_bytes, conf, times) in &existing {
            let emb = bytes_to_embedding(emb_bytes);
            let sim = cosine_similarity(&fact.embedding, &emb);
            if sim > 0.9
                && best_match
                    .as_ref()
                    .is_none_or(|(_, best_sim, _, _)| sim > *best_sim)
            {
                best_match = Some((id.clone(), sim, *conf, *times));
            }
        }

        if let Some((existing_id, _sim, old_conf, old_times)) = best_match {
            // Reinforce existing fact
            let new_times = old_times + 1;
            let new_conf = (old_conf as f32 * 0.7 + fact.confidence * 0.3).min(1.0);
            let source_json = serde_json::to_string(&fact.source_episodes)
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
            conn.execute(
                "UPDATE knowledge_facts
                 SET confidence = ?1, times_confirmed = ?2,
                     last_confirmed = ?3, source_episodes = ?4, content = ?5
                 WHERE id = ?6",
                params![
                    new_conf as f64,
                    new_times,
                    fact.last_confirmed,
                    source_json,
                    fact.content,
                    existing_id,
                ],
            )?;
            debug!(fact_id = %existing_id, times = new_times, "Reinforced existing fact");
        } else {
            // Insert new fact
            let source_json = serde_json::to_string(&fact.source_episodes)
                .map_err(|e| MemoryError::Storage(e.to_string()))?;
            let embedding_bytes = embedding_to_bytes(&fact.embedding);
            conn.execute(
                "INSERT INTO knowledge_facts
                 (id, tenant_id, category, subject, content, confidence,
                  source_episodes, embedding, last_confirmed, times_confirmed)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    fact.id,
                    fact.tenant_id,
                    fact.category.to_string(),
                    fact.subject,
                    fact.content,
                    fact.confidence as f64,
                    source_json,
                    embedding_bytes,
                    fact.last_confirmed,
                    fact.times_confirmed,
                ],
            )?;
            debug!(fact_id = %fact.id, "Learned new fact");
        }

        Ok(())
    }

    /// Decay confidence on all facts by a multiplicative factor.
    ///
    /// Returns the number of facts affected.
    pub fn decay(&self, decay_rate: f32) -> Result<u32, MemoryError> {
        let conn = self.open_conn()?;
        let affected = conn.execute(
            "UPDATE knowledge_facts SET confidence = confidence * ?1
             WHERE confidence > 0.01",
            params![decay_rate as f64],
        )?;
        // Clean up facts that have decayed below threshold
        conn.execute(
            "DELETE FROM knowledge_facts WHERE confidence < 0.01",
            [],
        )?;
        Ok(affected as u32)
    }

    /// Retrieve the highest-confidence facts for a tenant, without requiring embeddings.
    ///
    /// Results are ordered by confidence DESC and limited to `limit` rows.
    pub fn top_facts_by_tenant(
        &self,
        tenant_id: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeFact>, MemoryError> {
        let conn = self.open_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, tenant_id, category, subject, content, confidence,
                    source_episodes, embedding, last_confirmed, times_confirmed
             FROM knowledge_facts
             WHERE tenant_id = ?1
             ORDER BY confidence DESC
             LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![tenant_id, limit as i64], |row| {
                Ok(RawFactRow {
                    id: row.get(0)?,
                    tenant_id: row.get(1)?,
                    category: row.get(2)?,
                    subject: row.get(3)?,
                    content: row.get(4)?,
                    confidence: row.get(5)?,
                    source_episodes: row.get(6)?,
                    embedding: row.get(7)?,
                    last_confirmed: row.get(8)?,
                    times_confirmed: row.get(9)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        rows.into_iter().map(|r| r.into_fact()).collect()
    }

    /// Count total facts for a tenant.
    pub fn count(&self, tenant_id: &str) -> Result<usize, MemoryError> {
        let conn = self.open_conn()?;
        let count: usize = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_facts WHERE tenant_id = ?1",
            params![tenant_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Delete a specific fact by id (useful for GDPR compliance).
    pub fn delete(&self, fact_id: &str) -> Result<bool, MemoryError> {
        let conn = self.open_conn()?;
        let deleted = conn.execute(
            "DELETE FROM knowledge_facts WHERE id = ?1",
            params![fact_id],
        )?;
        Ok(deleted > 0)
    }
}

// ---------------------------------------------------------------------------
// Internal row type
// ---------------------------------------------------------------------------

struct RawFactRow {
    id: String,
    tenant_id: String,
    category: String,
    subject: String,
    content: String,
    confidence: f64,
    source_episodes: String,
    embedding: Vec<u8>,
    last_confirmed: String,
    times_confirmed: u32,
}

impl RawFactRow {
    fn into_fact(self) -> Result<KnowledgeFact, MemoryError> {
        let category: KnowledgeCategory = serde_json::from_str(&format!("\"{}\"", self.category))
            .map_err(|e| MemoryError::Storage(format!("Bad category: {e}")))?;
        let source_episodes: Vec<String> = serde_json::from_str(&self.source_episodes)
            .map_err(|e| MemoryError::Storage(format!("Bad source_episodes JSON: {e}")))?;
        let embedding = bytes_to_embedding(&self.embedding);

        Ok(KnowledgeFact {
            id: self.id,
            tenant_id: self.tenant_id,
            category,
            subject: self.subject,
            content: self.content,
            confidence: self.confidence as f32,
            source_episodes,
            embedding,
            last_confirmed: self.last_confirmed,
            times_confirmed: self.times_confirmed,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_memory() -> (tempfile::TempDir, SemanticMemory) {
        let dir = tempfile::tempdir().unwrap();
        let mem = SemanticMemory::new(dir.path());
        mem.init_tables().unwrap();
        (dir, mem)
    }

    fn sample_fact(id: &str, embedding: Vec<f32>) -> KnowledgeFact {
        KnowledgeFact {
            id: id.to_string(),
            tenant_id: "t1".to_string(),
            category: KnowledgeCategory::TechnicalFact,
            subject: "React hooks".to_string(),
            content: "useEffect runs after every render by default".to_string(),
            confidence: 0.8,
            source_episodes: vec!["ep1".to_string()],
            embedding,
            last_confirmed: "2026-01-15T10:00:00Z".to_string(),
            times_confirmed: 1,
        }
    }

    #[test]
    fn learn_and_count() {
        let (_dir, mem) = temp_memory();
        let fact = sample_fact("f1", vec![1.0, 0.0, 0.0]);
        mem.learn(&fact).unwrap();
        assert_eq!(mem.count("t1").unwrap(), 1);
        assert_eq!(mem.count("t2").unwrap(), 0);
    }

    #[test]
    fn learn_reinforces_similar_fact() {
        let (_dir, mem) = temp_memory();

        let fact1 = sample_fact("f1", vec![1.0, 0.0, 0.0]);
        mem.learn(&fact1).unwrap();

        // Learn a very similar fact (same direction embedding)
        let fact2 = sample_fact("f2", vec![0.99, 0.01, 0.0]);
        mem.learn(&fact2).unwrap();

        // Should still be 1 fact (reinforced, not duplicated)
        assert_eq!(mem.count("t1").unwrap(), 1);
    }

    #[test]
    fn learn_inserts_dissimilar_fact() {
        let (_dir, mem) = temp_memory();

        let fact1 = sample_fact("f1", vec![1.0, 0.0, 0.0]);
        mem.learn(&fact1).unwrap();

        // Learn a very different fact
        let mut fact2 = sample_fact("f2", vec![0.0, 1.0, 0.0]);
        fact2.subject = "CSS Grid".to_string();
        mem.learn(&fact2).unwrap();

        assert_eq!(mem.count("t1").unwrap(), 2);
    }

    #[test]
    fn query_returns_most_similar() {
        let (_dir, mem) = temp_memory();

        let f1 = sample_fact("f1", vec![1.0, 0.0, 0.0]);
        let mut f2 = sample_fact("f2", vec![0.0, 1.0, 0.0]);
        f2.category = KnowledgeCategory::UserPreference;
        f2.subject = "Dark mode".to_string();

        mem.learn(&f1).unwrap();
        mem.learn(&f2).unwrap();

        let results = mem.query(&[0.9, 0.1, 0.0], None, 10).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "f1");
    }

    #[test]
    fn query_filters_by_category() {
        let (_dir, mem) = temp_memory();

        let f1 = sample_fact("f1", vec![1.0, 0.0, 0.0]);
        let mut f2 = sample_fact("f2", vec![0.9, 0.1, 0.0]);
        f2.category = KnowledgeCategory::UserPreference;

        mem.learn(&f1).unwrap();
        mem.learn(&f2).unwrap();

        let results = mem
            .query(
                &[1.0, 0.0, 0.0],
                Some(&[KnowledgeCategory::UserPreference]),
                10,
            )
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "f2");
    }

    #[test]
    fn decay_reduces_confidence() {
        let (_dir, mem) = temp_memory();

        let fact = sample_fact("f1", vec![1.0, 0.0, 0.0]);
        mem.learn(&fact).unwrap();

        let affected = mem.decay(0.9).unwrap();
        assert_eq!(affected, 1);

        // Query and check confidence decreased
        let results = mem.query(&[1.0, 0.0, 0.0], None, 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].confidence < 0.8, "Confidence should have decayed");
    }

    #[test]
    fn delete_removes_fact() {
        let (_dir, mem) = temp_memory();

        let fact = sample_fact("f1", vec![1.0, 0.0, 0.0]);
        mem.learn(&fact).unwrap();
        assert_eq!(mem.count("t1").unwrap(), 1);

        let deleted = mem.delete("f1").unwrap();
        assert!(deleted);
        assert_eq!(mem.count("t1").unwrap(), 0);

        // Deleting non-existent returns false
        let deleted = mem.delete("nonexistent").unwrap();
        assert!(!deleted);
    }
}
