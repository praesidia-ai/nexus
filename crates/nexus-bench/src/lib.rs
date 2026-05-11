//! Shared fixtures and helpers for Nexus benchmarks.

use nexus_agents_core::definition::{AgentDefinition, ExecutionMode};
use nexus_kernel::process::{Priority, ResourceAllocation};
use nexus_memory::episodic::{Episode, EpisodeType, Outcome};
use nexus_memory::semantic::{KnowledgeCategory, KnowledgeFact};

/// Create a fresh in-memory SQLite connection with the baseline schema applied.
pub fn open_bench_db() -> rusqlite::Connection {
    let conn = nexus_store::open_connection(std::path::Path::new(":memory:")).unwrap();
    conn
}

/// Create a minimal agent definition for scheduler benchmarks.
pub fn test_agent(id: &str, name: &str) -> AgentDefinition {
    AgentDefinition {
        id: id.to_string(),
        name: name.to_string(),
        system_prompt: "Benchmark agent".to_string(),
        skills: vec![],
        tools: vec![],
        model_preference: None,
        execution_mode: ExecutionMode::Interactive,
        max_iterations: 10,
        timeout_secs: 60,
    }
}

/// Default resource allocation for benchmark processes.
pub fn default_resources() -> ResourceAllocation {
    ResourceAllocation::default()
}

/// Default priority for benchmark processes.
pub fn default_priority() -> Priority {
    Priority::Normal
}

/// Build a sample Episode for memory benchmarks.
pub fn sample_episode(id: &str, project_id: &str, importance: f32, embedding: Vec<f32>) -> Episode {
    Episode {
        id: id.to_string(),
        tenant_id: project_id.to_string(),
        timestamp: "2026-04-09T10:00:00Z".to_string(),
        agent_id: Some("bench-agent".to_string()),
        team_id: None,
        project_id: Some(project_id.to_string()),
        episode_type: EpisodeType::AgentExecution {
            task: "Benchmark task".to_string(),
            tools_used: vec!["file_write".to_string()],
            iterations: 1,
        },
        summary: format!("Benchmark episode {id}"),
        details: serde_json::json!({"bench": true}),
        outcome: Outcome::Success {
            quality_score: Some(0.9),
        },
        tags: vec!["benchmark".to_string()],
        importance,
        embedding,
    }
}

/// Build a sample KnowledgeFact for memory benchmarks.
pub fn sample_fact(id: &str, tenant_id: &str, embedding: Vec<f32>) -> KnowledgeFact {
    KnowledgeFact {
        id: id.to_string(),
        tenant_id: tenant_id.to_string(),
        category: KnowledgeCategory::TechnicalFact,
        subject: format!("Bench subject {id}"),
        content: "Benchmark knowledge fact content".to_string(),
        confidence: 0.85,
        source_episodes: vec!["ep-bench".to_string()],
        embedding,
        last_confirmed: "2026-04-09T10:00:00Z".to_string(),
        times_confirmed: 1,
    }
}

/// Generate a random-ish f32 vector of given dimension, normalized.
pub fn random_embedding(dim: usize, seed: u64) -> Vec<f32> {
    let mut v: Vec<f32> = (0..dim)
        .map(|i| {
            let x = ((seed.wrapping_mul(6364136223846793005).wrapping_add(i as u64)) as f32)
                / u64::MAX as f32;
            x * 2.0 - 1.0
        })
        .collect();
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
    v
}

/// Build a tokio runtime suitable for benchmarks.
pub fn bench_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
}
