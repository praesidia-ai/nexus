//! nexus-memory — Persistent agent memory with episodic and semantic tiers.
//!
//! Provides a unified [`MemorySystem`] combining:
//!
//! - **Episodic memory** — records of specific events and experiences (what happened).
//! - **Semantic memory** — distilled knowledge and facts (what was learned).
//! - **Vector store** — cosine-similarity utilities for embedding-based retrieval.
//!
//! All storage is backed by SQLite for simplicity and portability. Embeddings are
//! stored as binary blobs and similarity is computed in Rust at query time.

pub mod episodic;
pub mod error;
pub mod semantic;
pub mod vector_store;

pub use error::MemoryError;

use serde::Serialize;
use std::path::Path;

/// Pre-formatted context recalled from memory, ready to inject into an agent prompt.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RecalledContext {
    /// Summaries of recent relevant episodes.
    pub episodes: Vec<String>,
    /// Relevant knowledge facts.
    pub facts: Vec<String>,
    /// Pre-formatted block ready to inject into an agent prompt.
    pub context_block: String,
}

/// The unified memory system combining episodic and semantic memory.
pub struct MemorySystem {
    pub episodic: episodic::EpisodicMemory,
    pub semantic: semantic::SemanticMemory,
}

impl MemorySystem {
    /// Create a new memory system rooted at the given data directory.
    ///
    /// Initialises SQLite tables for both episodic and semantic stores.
    pub fn new(data_dir: &Path) -> Result<Self, MemoryError> {
        let episodic = episodic::EpisodicMemory::new(data_dir);
        episodic.init_tables()?;
        let semantic = semantic::SemanticMemory::new(data_dir);
        semantic.init_tables()?;
        Ok(Self { episodic, semantic })
    }

    /// Get a health summary of the memory system.
    pub fn health(&self, tenant_id: &str) -> MemoryHealth {
        MemoryHealth {
            episode_count: self.episodic.count(tenant_id).unwrap_or(0),
            fact_count: self.semantic.count(tenant_id).unwrap_or(0),
        }
    }

    /// Recall recent relevant episodes and facts by project_id and optional tags.
    ///
    /// This is a lightweight recall that does **not** require embeddings. It uses
    /// direct SQL filtering by project_id, optional agent_id, and optional tags,
    /// returning the most recent episodes and highest-confidence facts.
    pub fn recall_context(
        &self,
        project_id: &str,
        agent_id: Option<&str>,
        tags: &[&str],
        limit: usize,
    ) -> Result<RecalledContext, MemoryError> {
        // Query episodic memory: recent episodes for this project, optionally filtered by agent
        let episodes = self.episodic.recent_by_project(
            project_id,
            agent_id,
            tags,
            limit,
        )?;

        // Query semantic memory: high-confidence facts for this tenant (project_id as tenant)
        let facts = self.semantic.top_facts_by_tenant(project_id, limit)?;

        // Build episode summaries
        let episode_summaries: Vec<String> = episodes
            .iter()
            .map(|ep| format!("[{}] {}", ep.timestamp, ep.summary))
            .collect();

        // Build fact summaries
        let fact_summaries: Vec<String> = facts
            .iter()
            .map(|f| format!("[{}] {}: {} (confidence: {:.2})", f.category, f.subject, f.content, f.confidence))
            .collect();

        // Format into a context block
        let context_block = if episode_summaries.is_empty() && fact_summaries.is_empty() {
            String::new()
        } else {
            let mut block = String::from("## Memory Context\n");
            if !episode_summaries.is_empty() {
                block.push_str("### Recent Events\n");
                for s in &episode_summaries {
                    block.push_str(&format!("- {}\n", s));
                }
            }
            if !fact_summaries.is_empty() {
                block.push_str("### Known Facts\n");
                for s in &fact_summaries {
                    block.push_str(&format!("- {}\n", s));
                }
            }
            block
        };

        Ok(RecalledContext {
            episodes: episode_summaries,
            facts: fact_summaries,
            context_block,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryHealth {
    pub episode_count: usize,
    pub fact_count: usize,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_system_initialises() {
        let dir = tempfile::tempdir().unwrap();
        let sys = MemorySystem::new(dir.path()).unwrap();
        let health = sys.health("test-tenant");
        assert_eq!(health.episode_count, 0);
        assert_eq!(health.fact_count, 0);
    }

    #[test]
    fn recall_context_empty_when_no_memories() {
        let dir = tempfile::tempdir().unwrap();
        let sys = MemorySystem::new(dir.path()).unwrap();
        let recalled = sys
            .recall_context("nonexistent-project", None, &[], 10)
            .unwrap();
        assert!(recalled.episodes.is_empty());
        assert!(recalled.facts.is_empty());
        assert!(recalled.context_block.is_empty());
    }

    #[test]
    fn recall_context_returns_formatted_context_when_data_exists() {
        let dir = tempfile::tempdir().unwrap();
        let sys = MemorySystem::new(dir.path()).unwrap();

        // Add an episode with project_id
        let ep = episodic::Episode {
            id: "ep1".to_string(),
            tenant_id: "proj1".to_string(),
            timestamp: "2026-01-15T10:00:00Z".to_string(),
            agent_id: Some("nova".to_string()),
            team_id: None,
            project_id: Some("proj1".to_string()),
            episode_type: episodic::EpisodeType::AgentExecution {
                task: "Build login page".to_string(),
                tools_used: vec!["file_write".to_string()],
                iterations: 1,
            },
            summary: "Agent nova built the login page".to_string(),
            details: serde_json::json!({}),
            outcome: episodic::Outcome::Success {
                quality_score: Some(0.9),
            },
            tags: vec!["frontend".to_string()],
            importance: 0.7,
            embedding: vec![],
        };
        sys.episodic.record(&ep).unwrap();

        // Add a fact with tenant_id = "proj1"
        let fact = semantic::KnowledgeFact {
            id: "f1".to_string(),
            tenant_id: "proj1".to_string(),
            category: semantic::KnowledgeCategory::TechnicalFact,
            subject: "React hooks".to_string(),
            content: "useEffect runs after every render".to_string(),
            confidence: 0.85,
            source_episodes: vec!["ep1".to_string()],
            embedding: vec![1.0, 0.0],
            last_confirmed: "2026-01-15T10:00:00Z".to_string(),
            times_confirmed: 1,
        };
        sys.semantic.learn(&fact).unwrap();

        let recalled = sys.recall_context("proj1", None, &[], 10).unwrap();
        assert_eq!(recalled.episodes.len(), 1);
        assert_eq!(recalled.facts.len(), 1);
        assert!(recalled.context_block.contains("## Memory Context"));
        assert!(recalled.context_block.contains("### Recent Events"));
        assert!(recalled.context_block.contains("Agent nova built the login page"));
        assert!(recalled.context_block.contains("### Known Facts"));
        assert!(recalled.context_block.contains("React hooks"));
        assert!(recalled.context_block.contains("confidence: 0.85"));
    }

    #[test]
    fn health_reflects_stored_data() {
        let dir = tempfile::tempdir().unwrap();
        let sys = MemorySystem::new(dir.path()).unwrap();

        // Add an episode
        let ep = episodic::Episode {
            id: "ep1".to_string(),
            tenant_id: "t1".to_string(),
            timestamp: "2026-01-15T10:00:00Z".to_string(),
            agent_id: None,
            team_id: None,
            project_id: None,
            episode_type: episodic::EpisodeType::DecisionMade {
                area: "architecture".to_string(),
                choice: "microservices".to_string(),
                rationale: "scalability".to_string(),
            },
            summary: "Chose microservices".to_string(),
            details: serde_json::json!({}),
            outcome: episodic::Outcome::Success {
                quality_score: None,
            },
            tags: vec![],
            importance: 0.5,
            embedding: vec![1.0, 0.0],
        };
        sys.episodic.record(&ep).unwrap();

        // Add a fact
        let fact = semantic::KnowledgeFact {
            id: "f1".to_string(),
            tenant_id: "t1".to_string(),
            category: semantic::KnowledgeCategory::TechnicalFact,
            subject: "test".to_string(),
            content: "test content".to_string(),
            confidence: 0.8,
            source_episodes: vec![],
            embedding: vec![1.0, 0.0],
            last_confirmed: "2026-01-15T10:00:00Z".to_string(),
            times_confirmed: 1,
        };
        sys.semantic.learn(&fact).unwrap();

        let health = sys.health("t1");
        assert_eq!(health.episode_count, 1);
        assert_eq!(health.fact_count, 1);
    }
}
