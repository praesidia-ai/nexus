use crate::compression::{auto_compact, full_compact, micro_compact, CompressionResult};
use crate::layer1::ProjectIndex;
use crate::layer2::{ContextItem, ContextKind, ContextPriority, SessionMemory};
use crate::layer3::{FactCategory, KnowledgeFact, PersistentKnowledge};

pub struct ContextEngine {
    pub l1: ProjectIndex,
    pub l2: SessionMemory,
    pub l3: PersistentKnowledge,
}

impl ContextEngine {
    pub fn new(
        project_index: ProjectIndex,
        max_context_tokens: usize,
        knowledge_db_path: &std::path::Path,
    ) -> Result<Self, crate::error::ContextError> {
        let l3 = PersistentKnowledge::new(knowledge_db_path)
            .map_err(|e| crate::error::ContextError::Storage(e.to_string()))?;
        Ok(Self {
            l1: project_index,
            l2: SessionMemory::new(max_context_tokens),
            l3,
        })
    }

    /// Add content to the session (L2) and auto-compress if needed.
    pub fn push(
        &mut self,
        kind: ContextKind,
        content: String,
        priority: ContextPriority,
    ) -> Option<CompressionResult> {
        let token_estimate = content.len() / 4;
        let item = ContextItem {
            id: uuid::Uuid::new_v4().to_string(),
            kind,
            content,
            token_estimate,
            priority,
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            access_count: 0,
            pinned: priority == ContextPriority::Pinned,
        };
        self.l2.push(item);

        if self.l2.needs_compression() {
            Some(self.compress())
        } else {
            None
        }
    }

    /// Force compression at the appropriate level based on current usage.
    pub fn compress(&mut self) -> CompressionResult {
        let (current, max) = self.l2.current_usage();
        let usage = current as f64 / max as f64;

        if usage < 0.85 {
            micro_compact(&mut self.l2)
        } else if usage < 0.95 {
            auto_compact(&mut self.l2)
        } else {
            full_compact(&mut self.l2)
        }
    }

    /// Build the full context window for an LLM call.
    pub fn build_prompt_context(&self) -> String {
        let mut ctx = String::new();

        ctx.push_str(&self.l1.render());
        ctx.push_str("\n---\n");

        let facts = self.l3.query(&self.l1.project_id, None, 10);
        if !facts.is_empty() {
            ctx.push_str("## Known Facts\n");
            for fact in &facts {
                ctx.push_str(&format!(
                    "- [{}] {}\n",
                    fact.category_label(),
                    fact.content
                ));
            }
            ctx.push_str("\n---\n");
        }

        for item in self.l2.render_window() {
            ctx.push_str(&format!("[{}] {}\n", item.kind_label(), item.content));
        }

        ctx
    }

    /// Learn a new fact (store in L3).
    pub fn learn(&self, category: FactCategory, content: &str, source: &str) {
        let fact = KnowledgeFact {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: self.l1.project_id.clone(),
            category,
            content: content.to_string(),
            confidence: 1.0,
            source: source.to_string(),
            created_at: chrono::Utc::now(),
            last_verified: chrono::Utc::now(),
            access_count: 0,
            superseded_by: None,
        };
        if let Err(e) = self.l3.store(&fact) {
            tracing::warn!(error = %e, "Failed to store knowledge fact");
        }
    }

    pub fn usage(&self) -> (usize, usize) {
        self.l2.current_usage()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_engine(max_tokens: usize) -> (tempfile::TempDir, ContextEngine) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("knowledge.db");
        let idx = ProjectIndex::new("test-proj", "Test Project");
        let engine = ContextEngine::new(idx, max_tokens, &db_path).unwrap();
        (dir, engine)
    }

    #[test]
    fn push_tracks_usage() {
        let (_dir, mut engine) = temp_engine(10_000);
        let result = engine.push(
            ContextKind::UserMessage,
            "Hello world".to_string(),
            ContextPriority::Normal,
        );
        assert!(result.is_none());
        let (used, max) = engine.usage();
        assert!(used > 0);
        assert_eq!(max, 10_000);
    }

    #[test]
    fn push_triggers_compression_at_threshold() {
        let (_dir, mut engine) = temp_engine(100);
        // Push enough to exceed 80% of 100 tokens
        let result = engine.push(
            ContextKind::UserMessage,
            "x".repeat(400), // 100 tokens
            ContextPriority::Normal,
        );
        assert!(result.is_some());
    }

    #[test]
    fn build_prompt_context_includes_l1() {
        let (_dir, engine) = temp_engine(10_000);
        let ctx = engine.build_prompt_context();
        assert!(ctx.contains("# Project: Test Project"));
    }

    #[test]
    fn build_prompt_context_includes_l2_items() {
        let (_dir, mut engine) = temp_engine(10_000);
        engine.push(
            ContextKind::UserMessage,
            "User said hello".to_string(),
            ContextPriority::Normal,
        );
        let ctx = engine.build_prompt_context();
        assert!(ctx.contains("[user] User said hello"));
    }

    #[test]
    fn build_prompt_context_includes_l3_facts() {
        let (_dir, engine) = temp_engine(10_000);
        engine.learn(FactCategory::Architecture, "Use event sourcing", "design-review");
        let ctx = engine.build_prompt_context();
        assert!(ctx.contains("## Known Facts"));
        assert!(ctx.contains("[arch] Use event sourcing"));
    }

    #[test]
    fn learn_deduplicates() {
        let (_dir, engine) = temp_engine(10_000);
        engine.learn(FactCategory::Convention, "snake_case naming", "lint");
        engine.learn(FactCategory::Convention, "snake_case naming", "lint");

        let facts = engine.l3.query("test-proj", None, 10);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].access_count, 1);
    }

    #[test]
    fn compress_uses_appropriate_level() {
        let (_dir, mut engine) = temp_engine(1000);
        // Fill to ~82% — should trigger micro
        engine.l2.push(ContextItem {
            id: "a".to_string(),
            kind: ContextKind::ToolResult,
            content: "data".to_string(),
            token_estimate: 820,
            priority: ContextPriority::Low,
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            access_count: 0,
            pinned: false,
        });
        let result = engine.compress();
        assert!(matches!(
            result.level,
            crate::compression::CompressionLevel::Micro
        ));
    }

    #[test]
    fn compress_auto_at_90_percent() {
        let (_dir, mut engine) = temp_engine(1000);
        engine.l2.push(ContextItem {
            id: "a".to_string(),
            kind: ContextKind::ToolResult,
            content: "data".to_string(),
            token_estimate: 900,
            priority: ContextPriority::Low,
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            access_count: 0,
            pinned: false,
        });
        let result = engine.compress();
        assert!(matches!(
            result.level,
            crate::compression::CompressionLevel::Auto
        ));
    }

    #[test]
    fn compress_full_at_96_percent() {
        let (_dir, mut engine) = temp_engine(1000);
        engine.l2.push(ContextItem {
            id: "a".to_string(),
            kind: ContextKind::ToolResult,
            content: "data".to_string(),
            token_estimate: 960,
            priority: ContextPriority::Low,
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            access_count: 0,
            pinned: false,
        });
        let result = engine.compress();
        assert!(matches!(
            result.level,
            crate::compression::CompressionLevel::Full
        ));
    }

    #[test]
    fn full_roundtrip_with_all_layers() {
        let (_dir, mut engine) = temp_engine(10_000);

        engine.l1.tech_stack = vec!["Rust".into(), "PostgreSQL".into()];
        engine.l1.add_decision("Use Axum", "Performance and ergonomics");

        engine.learn(FactCategory::Pattern, "Repository pattern for data access", "team-discussion");

        engine.push(
            ContextKind::SystemInstruction,
            "You are a helpful assistant.".to_string(),
            ContextPriority::Critical,
        );
        engine.push(
            ContextKind::UserMessage,
            "Explain the architecture".to_string(),
            ContextPriority::Normal,
        );

        let ctx = engine.build_prompt_context();
        assert!(ctx.contains("# Project: Test Project"));
        assert!(ctx.contains("Tech: Rust, PostgreSQL"));
        assert!(ctx.contains("Decision: Use Axum"));
        assert!(ctx.contains("[pattern] Repository pattern"));
        assert!(ctx.contains("[system] You are a helpful assistant."));
        assert!(ctx.contains("[user] Explain the architecture"));
    }
}
