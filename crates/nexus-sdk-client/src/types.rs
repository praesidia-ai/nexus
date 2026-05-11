//! Response types returned by the Nexus API.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Result of a full app generation via `/oneshot`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationResult {
    /// Unique identifier for the generated project.
    pub project_id: String,
    /// Human-readable project name.
    pub project_name: String,
    /// Taste quality score (0.0 to 1.0).
    pub taste_score: f32,
    /// Number of files generated.
    pub files_count: usize,
    /// Total generation time in milliseconds.
    pub duration_ms: u64,
    /// URL where the running app can be accessed, if available.
    pub app_url: Option<String>,
}

/// Result of running a single agent on a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    /// The agent's textual output.
    pub output: String,
    /// Files that were created or modified by the agent.
    pub files_modified: Vec<String>,
    /// Number of LLM iterations the agent used.
    pub iterations_used: u32,
    /// Total execution time in milliseconds.
    pub duration_ms: u64,
}

/// Result of running a multi-agent team on a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamResult {
    /// Artifacts produced by the team (file paths, URLs, etc.).
    pub artifacts: Vec<String>,
    /// Total number of inter-agent messages exchanged.
    pub messages_count: usize,
    /// Estimated USD cost of all LLM calls.
    pub cost_usd: f64,
    /// Total execution time in milliseconds.
    pub duration_ms: u64,
}

/// Quality assessment of a generated project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityScore {
    /// Overall quality score (0.0 to 1.0).
    pub overall: f32,
    /// Per-axis scores (e.g. "layout", "typography", "color", "responsiveness").
    pub axes: HashMap<String, f32>,
}

/// Result of intent analysis on a natural-language description.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentResult {
    /// Detected application type (e.g. "dashboard", "e-commerce", "blog").
    pub app_type: String,
    /// Estimated complexity (e.g. "simple", "medium", "complex").
    pub complexity: String,
    /// Application domain (e.g. "finance", "health", "education").
    pub domain: String,
    /// Confidence of the intent analysis (0.0 to 1.0).
    pub confidence: f32,
}

/// A single entry from the persistent memory system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Unique identifier for this memory entry.
    pub id: String,
    /// The stored content.
    pub content: String,
    /// Category or classification of the memory.
    pub category: String,
    /// Confidence or relevance score (0.0 to 1.0).
    pub confidence: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_generation_result() {
        let json = r#"{
            "project_id": "abc-123",
            "project_name": "Todo App",
            "taste_score": 0.87,
            "files_count": 12,
            "duration_ms": 45000,
            "app_url": "http://localhost:3000"
        }"#;
        let result: GenerationResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.project_id, "abc-123");
        assert_eq!(result.files_count, 12);
        assert!((result.taste_score - 0.87).abs() < f32::EPSILON);
    }

    #[test]
    fn deserialize_agent_result() {
        let json = r#"{
            "output": "Task completed",
            "files_modified": ["src/main.rs"],
            "iterations_used": 3,
            "duration_ms": 12000
        }"#;
        let result: AgentResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.iterations_used, 3);
        assert_eq!(result.files_modified.len(), 1);
    }

    #[test]
    fn deserialize_quality_score() {
        let json = r#"{
            "overall": 0.92,
            "axes": {"layout": 0.95, "typography": 0.88}
        }"#;
        let result: QualityScore = serde_json::from_str(json).unwrap();
        assert_eq!(result.axes.len(), 2);
        assert!((result.overall - 0.92).abs() < f32::EPSILON);
    }

    #[test]
    fn deserialize_intent_result() {
        let json = r#"{
            "app_type": "dashboard",
            "complexity": "medium",
            "domain": "finance",
            "confidence": 0.91
        }"#;
        let result: IntentResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.app_type, "dashboard");
    }

    #[test]
    fn deserialize_memory_entry() {
        let json = r#"{
            "id": "mem-1",
            "content": "User prefers dark themes",
            "category": "preferences",
            "confidence": 0.85
        }"#;
        let result: MemoryEntry = serde_json::from_str(json).unwrap();
        assert_eq!(result.category, "preferences");
    }

    #[test]
    fn deserialize_team_result() {
        let json = r#"{
            "artifacts": ["output.zip"],
            "messages_count": 24,
            "cost_usd": 0.45,
            "duration_ms": 90000
        }"#;
        let result: TeamResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.messages_count, 24);
    }
}
